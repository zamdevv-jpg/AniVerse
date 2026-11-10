use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, RwLock},
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Result, anyhow};
use chrono::{Datelike, Duration as ChronoDuration, FixedOffset, NaiveDate, Utc};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use tokio::task::JoinSet;

use crate::models::{
    AnimeDetail, BrowsePage, BrowseParams, Card, DownloadLink, EpisodeDetail, EpisodeLink, Genre,
    HeroSlide, HomeData, InfoItem, QualityOption, ScheduleDay, StreamSource, VideoServer, encode,
    is_adult_card, is_adult_genre, normalize_title_for_dedup,
};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const PAGE_TTL: StdDuration = StdDuration::from_secs(600);
const DETAIL_TTL: StdDuration = StdDuration::from_secs(1200);
const HOME_TTL: StdDuration = StdDuration::from_secs(240);
const HEALTH_TTL: StdDuration = StdDuration::from_secs(600);
const PROBE_TIMEOUT: StdDuration = StdDuration::from_secs(6);

/// Batas waktu total untuk mengambil popularitas MyAnimeList. Beranda tidak
/// boleh menunggu API eksternal lebih lama dari ini; bila lewat, hero lokal
/// yang dipakai supaya halaman tetap responsif.
const MAL_BUDGET: StdDuration = StdDuration::from_secs(4);
const MAL_CACHE_TTL: StdDuration = StdDuration::from_secs(6 * 60 * 60);
const MAL_FAIL_TTL: StdDuration = StdDuration::from_secs(10 * 60);
const MAL_CACHE_KEY: &str = "mal:top-anime:bypopularity:v3";
const MAL_FAIL_KEY: &str = "mal:top-anime:bypopularity:fail:v1";

/// Klien sumber data Nimegami. Semua halaman diambil sebagai HTML
/// server-rendered lalu diurai menjadi model aplikasi.
pub struct NimegamiSource {
    base_url: String,
    schedule_api_url: String,
    mal_top_api_url: String,
    client: reqwest::Client,
    proxy_client: reqwest::Client,
    mal_client: reqwest::Client,
    cache: Arc<Cache>,
    health: Arc<RwLock<HashMap<String, (Instant, bool)>>>,
    /// Hasil parsing beranda disimpan sebentar karena halaman sumber cukup
    /// besar dan diurai ulang pada setiap navigasi.
    parsed_home: RwLock<Option<(Instant, ParsedHome)>>,
    /// Snapshot kartu terbaru untuk rekomendasi instan tanpa jaringan.
    recommend: RwLock<Option<(Instant, Vec<Card>)>>,
}

type ParsedHome = (Vec<Card>, Vec<Card>, Vec<HeroSlide>);

impl NimegamiSource {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let schedule_api_url = std::env::var("MAL_SCHEDULE_API")
            .unwrap_or_else(|_| "https://jikan.lucashdo.com/v1/schedules".to_string());
        // `api.jikan.moe` sering diblokir dari jaringan Indonesia, jadi default
        // memakai mirror yang sama dengan sumber jadwal. Nilainya tetap bisa
        // ditimpa lewat env `MAL_TOP_API`.
        let mal_top_api_url = std::env::var("MAL_TOP_API")
            .unwrap_or_else(|_| "https://jikan.lucashdo.com/v1/top/anime".to_string());
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(StdDuration::from_secs(25))
            .connect_timeout(StdDuration::from_secs(8))
            .pool_idle_timeout(StdDuration::from_secs(90))
            .redirect(reqwest::redirect::Policy::limited(10))
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::ACCEPT,
                    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
                        .parse()
                        .expect("valid accept header"),
                );
                headers.insert(
                    reqwest::header::ACCEPT_LANGUAGE,
                    "id-ID,id;q=0.9,en-US;q=0.8,en;q=0.7"
                        .parse()
                        .expect("valid language header"),
                );
                headers.insert(
                    reqwest::header::REFERER,
                    format!("{base_url}/").parse().expect("valid referer"),
                );
                headers
            })
            .build()?;

        Ok(Self {
            base_url,
            schedule_api_url,
            mal_top_api_url,
            client,
            proxy_client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(StdDuration::from_secs(8))
                .pool_idle_timeout(StdDuration::from_secs(90))
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 8
                        || !is_allowed_video_url(attempt.url().as_str())
                    {
                        attempt.stop()
                    } else {
                        attempt.follow()
                    }
                }))
                .build()?,
            mal_client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(StdDuration::from_secs(8))
                .connect_timeout(StdDuration::from_secs(4))
                .build()?,
            cache: Arc::new(Cache::default()),
            health: Arc::new(RwLock::new(HashMap::new())),
            parsed_home: RwLock::new(None),
            recommend: RwLock::new(None),
        })
    }

    /// Meneruskan permintaan video ke CDN sumber tanpa header Referer lokal.
    ///
    /// Berkasdrive menolak permintaan yang membawa Referer `127.0.0.1`, jadi
    /// pemutar tidak bisa memuat MP4 langsung dari browser. Proxy ini mengirim
    /// header yang benar sekaligus meneruskan `Range` agar seek tetap bekerja.
    pub async fn proxy_video(
        &self,
        url: &str,
        range: Option<&str>,
    ) -> Result<reqwest::Response> {
        if !is_allowed_video_url(url) {
            return Err(anyhow!("host video tidak diizinkan untuk diproksi"));
        }
        let mut request = self
            .proxy_client
            .get(url)
            .header(reqwest::header::REFERER, format!("{}/", self.base_url))
            .header(
                reqwest::header::ORIGIN,
                self.base_url.clone(),
            )
            .header(reqwest::header::ACCEPT, "video/*,*/*;q=0.8");
        if let Some(range) = range {
            request = request.header(reqwest::header::RANGE, range);
        }
        request.send().await.map_err(Into::into)
    }

    pub fn host(&self) -> String {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(&self.base_url)
            .to_string()
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    async fn html(&self, path: &str, ttl: StdDuration) -> Result<String> {
        let key = path.to_string();
        if let Some(body) = self.cache.fresh(&key) {
            return Ok(body);
        }

        // Halaman lama langsung dikirim, pembaruan berjalan di latar belakang.
        // Dengan begitu membuka anime yang pernah dibuka terasa instan.
        if let Some(body) = self.cache.stale(&key) {
            self.refresh_html_in_background(key, path.to_string(), ttl);
            return Ok(body);
        }

        match self.fetch(path).await {
            Ok(body) => {
                self.cache.put(key, body.clone(), ttl);
                Ok(body)
            }
            Err(error) => Err(error),
        }
    }

    /// Memperbarui halaman di cache tanpa menahan permintaan pengguna.
    fn refresh_html_in_background(&self, key: String, path: String, ttl: StdDuration) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        if !self.cache.begin_refresh(&key) {
            return;
        }

        let client = self.client.clone();
        let url = self.url(&path);
        let cache = self.cache.clone();
        handle.spawn(async move {
            match fetch_body(&client, &url).await {
                Ok(body) => cache.put(key.clone(), body, ttl),
                Err(error) => {
                    tracing::debug!(path, %error, "gagal memperbarui cache nimegami");
                }
            }
            cache.end_refresh(&key);
        });
    }

    async fn fetch(&self, path: &str) -> Result<String> {
        fetch_body(&self.client, &self.url(path)).await
    }

    /* ---------------------------------------------------------------------
       Beranda
       --------------------------------------------------------------------- */

    pub async fn home(&self) -> Result<HomeData> {
        // Halaman Nimegami dan daftar popularitas MAL diambil bersamaan agar
        // beranda tidak menunggu dua jaringan secara berurutan.
        let (html_result, rankings) = tokio::join!(self.html("/", HOME_TTL), self.mal_rankings());
        let html = html_result?;
        let (latest, mut popular, website_hero) = self.parse_home(&html);
        self.store_recommendations(&latest);

        let hero = match rankings {
            Some(entries) if !entries.is_empty() => {
                let slides = build_popularity_hero(&entries, &latest, &popular, &website_hero);
                if slides.is_empty() { website_hero } else { slides }
            }
            _ => website_hero,
        };

        for (index, card) in popular.iter_mut().enumerate() {
            card.rank = index + 1;
        }

        Ok(HomeData {
            latest,
            popular,
            hero,
        })
    }

    /// Hanya kartu terbaru dari beranda, tanpa menunggu API eksternal.
    ///
    /// Halaman detail dan tonton cuma butuh daftar rekomendasi ringan, jadi
    /// jalur ini sengaja tidak menyentuh MyAnimeList. Bila snapshot rekomendasi
    /// masih segar, tidak ada permintaan jaringan sama sekali.
    pub async fn latest(&self, limit: usize) -> Vec<Card> {
        if let Some(cards) = self.cached_recommendations(limit) {
            return cards;
        }
        let Ok(html) = self.html("/", HOME_TTL).await else {
            return Vec::new();
        };
        let latest = self.parse_home(&html).0;
        self.store_recommendations(&latest);
        latest.into_iter().take(limit).collect()
    }

    /// Rekomendasi instan dari snapshot terakhir, tanpa jaringan.
    pub fn latest_cached(&self, limit: usize) -> Vec<Card> {
        self.cached_recommendations(limit).unwrap_or_default()
    }

    fn store_recommendations(&self, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        if let Ok(mut cache) = self.recommend.write() {
            *cache = Some((Instant::now(), cards.to_vec()));
        }
    }

    fn cached_recommendations(&self, limit: usize) -> Option<Vec<Card>> {
        let cache = self.recommend.read().ok()?;
        let (stored_at, cards) = cache.as_ref()?;
        if stored_at.elapsed() >= StdDuration::from_secs(30 * 60) {
            return None;
        }
        Some(cards.iter().take(limit).cloned().collect())
    }

    /// Urai halaman beranda dengan cache singkat. TTL-nya lebih pendek dari
    /// `HOME_TTL` supaya konten baru tetap muncul tanpa mengulang parsing
    /// 280 KB HTML pada setiap navigasi.
    fn parse_home(&self, html: &str) -> ParsedHome {
        if let Ok(cache) = self.parsed_home.read()
            && let Some((stored_at, parsed)) = cache.as_ref()
            && stored_at.elapsed() < StdDuration::from_secs(90)
        {
            return parsed.clone();
        }

        let parsed = parse_home_html(html, &self.base_url);
        if let Ok(mut cache) = self.parsed_home.write() {
            *cache = Some((Instant::now(), parsed.clone()));
        }
        parsed
    }

    /// Daftar anime terpopuler MyAnimeList, dengan cache dan batas waktu.
    ///
    /// Kegagalan disimpan sebentar (`MAL_FAIL_TTL`) supaya beranda tidak
    /// menunggu timeout pada setiap permintaan ketika sumber sedang mati.
    async fn mal_rankings(&self) -> Option<Vec<Value>> {
        if let Some(body) = self.cache.fresh(MAL_CACHE_KEY)
            && let Ok(entries) = serde_json::from_str::<Vec<Value>>(&body)
        {
            return Some(entries);
        }
        if self.cache.fresh(MAL_FAIL_KEY).is_some() {
            return self.mal_stale_rankings();
        }

        match tokio::time::timeout(MAL_BUDGET, self.fetch_mal_rankings()).await {
            Ok(Ok(entries)) if !entries.is_empty() => {
                if let Ok(body) = serde_json::to_string(&entries) {
                    self.cache.put(MAL_CACHE_KEY.to_string(), body, MAL_CACHE_TTL);
                }
                Some(entries)
            }
            Ok(Ok(_)) => {
                tracing::warn!("popularitas MyAnimeList kosong, memakai pilihan beranda");
                self.cache
                    .put(MAL_FAIL_KEY.to_string(), String::new(), MAL_FAIL_TTL);
                self.mal_stale_rankings()
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "gagal mengambil popularitas MyAnimeList, memakai pilihan beranda");
                self.cache
                    .put(MAL_FAIL_KEY.to_string(), String::new(), MAL_FAIL_TTL);
                self.mal_stale_rankings()
            }
            Err(_) => {
                tracing::warn!(
                    "popularitas MyAnimeList melewati batas waktu, memakai pilihan beranda"
                );
                self.cache
                    .put(MAL_FAIL_KEY.to_string(), String::new(), MAL_FAIL_TTL);
                self.mal_stale_rankings()
            }
        }
    }

    /// Cache lama tetap dipakai saat refresh gagal supaya hero tidak berganti
    /// tiba-tiba hanya karena API eksternal sedang down.
    fn mal_stale_rankings(&self) -> Option<Vec<Value>> {
        let body = self.cache.stale(MAL_CACHE_KEY)?;
        serde_json::from_str::<Vec<Value>>(&body).ok()
    }

    /// Mengambil dua halaman teratas secara paralel.
    ///
    /// Parameter `limit` sengaja tidak dikirim karena mirror jikan-edge
    /// menolaknya dengan HTTP 400; ukuran halaman bawaan sumber sudah cukup.
    async fn fetch_mal_rankings(&self) -> Result<Vec<Value>> {
        let base = self.mal_top_api_url.trim_end_matches('/');
        let separator = if base.contains('?') { '&' } else { '?' };
        let mut jobs = JoinSet::new();

        for page in 1..=2 {
            let url = format!("{base}{separator}filter=bypopularity&page={page}");
            let client = self.mal_client.clone();
            jobs.spawn(async move {
                let response = client.get(url).send().await?.error_for_status()?;
                let payload: Value = response.json().await?;
                Ok::<Vec<Value>, anyhow::Error>(
                    payload
                        .get("data")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                )
            });
        }

        let mut rankings = Vec::new();
        let mut seen = HashSet::new();
        let mut last_error = None;
        while let Some(joined) = jobs.join_next().await {
            match joined {
                Ok(Ok(entries)) => {
                    for entry in entries {
                        let key = entry
                            .get("mal_id")
                            .or_else(|| entry.get("malId"))
                            .map(Value::to_string)
                            .or_else(|| {
                                entry
                                    .get("title")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                            });
                        if let Some(key) = key
                            && !seen.insert(key)
                        {
                            continue;
                        }
                        rankings.push(entry);
                    }
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(error) => last_error = Some(anyhow!(error)),
            }
        }

        if rankings.is_empty() {
            return Err(last_error
                .unwrap_or_else(|| anyhow!("respons popularitas MyAnimeList kosong")));
        }

        Ok(rankings)
    }

    pub async fn popular(&self, limit: usize) -> Result<Vec<Card>> {
        let html = self
            .html("/anime-terbaik/", StdDuration::from_secs(600))
            .await?;
        let document = Html::parse_document(&html);
        Ok(parse_cards(document.root_element(), &self.base_url, limit))
    }

    /* ---------------------------------------------------------------------
       Katalog & pencarian
       --------------------------------------------------------------------- */

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let path = format!("/anime/?q={}", encode(query));
        let html = self.html(&path, StdDuration::from_secs(300)).await?;
        let document = Html::parse_document(&html);
        Ok(parse_cards(document.root_element(), &self.base_url, limit))
    }

    pub async fn browse(&self, params: &BrowseParams) -> Result<BrowsePage> {
        let mut query = Vec::new();
        if !params.q.is_empty() {
            query.push(format!("q={}", encode(&params.q)));
        }
        let sort = match params.order.as_str() {
            "update" | "" => "updated",
            "latest" => "newest",
            "popular" => "popular",
            "rating" => "rating",
            "title" => "title",
            other => other,
        };
        query.push(format!("sort={}", encode(sort)));
        for genre in &params.genres {
            query.push(format!("genre={}", encode(genre)));
        }
        if params.page > 1 {
            query.push(format!("page={}", params.page));
        }

        let base_path = if params.kind.eq_ignore_ascii_case("movie") {
            "/movie/"
        } else {
            "/anime/"
        };
        let path = format!("{base_path}?{}", query.join("&"));
        let html = self.html(&path, PAGE_TTL).await?;
        let document = Html::parse_document(&html);

        let cards = parse_cards(document.root_element(), &self.base_url, 60);
        let has_next = has_next_page(&document, params.page);
        let genres = parse_genres(&document);

        Ok(BrowsePage {
            cards,
            page: params.page,
            has_next,
            genres,
        })
    }

    pub async fn genres(&self) -> Result<Vec<Genre>> {
        let html = self.html("/anime/", DETAIL_TTL).await?;
        let document = Html::parse_document(&html);
        let mut genres = parse_genres(&document);
        if genres.is_empty() {
            genres = default_genres();
        }
        Ok(genres)
    }

    /* ---------------------------------------------------------------------
       Detail anime
       --------------------------------------------------------------------- */

    pub async fn detail(&self, raw_slug: &str) -> Result<AnimeDetail> {
        let slug = clean_slug(raw_slug);
        if slug.is_empty() {
            return Err(anyhow!("Slug anime kosong"));
        }
        self.load_detail_page(&slug)
            .await
            .map(|(_, detail)| detail)
    }

    async fn load_detail_page(&self, slug: &str) -> Result<(String, AnimeDetail)> {
        let path = format!("/{slug}/");
        let html = self.html(&path, DETAIL_TTL).await?;
        let detail = build_detail(&html, slug, &self.base_url)?;
        Ok((html, detail))
    }

    async fn resolve_episode_series(
        &self,
        requested_slug: &str,
    ) -> Result<(String, AnimeDetail, String)> {
        match self.load_detail_page(requested_slug).await {
            Ok((html, detail)) => Ok((html, detail, requested_slug.to_string())),
            Err(original_error) => {
                let query = episode_series_search_query(requested_slug);
                let target_tokens = title_tokens(&query);
                let Ok(mut cards) = self.search(&query, 24).await else {
                    return Err(original_error);
                };

                cards.sort_by(|a, b| {
                    let score_a = title_match_score(&target_tokens, &a.title, &a.slug);
                    let score_b = title_match_score(&target_tokens, &b.title, &b.slug);
                    score_b.total_cmp(&score_a)
                });

                for card in cards {
                    if title_match_score(&target_tokens, &card.title, &card.slug) < 0.80 {
                        continue;
                    }
                    if let Ok((html, detail)) = self.load_detail_page(&card.slug).await {
                        return Ok((html, detail, card.slug));
                    }
                }

                Err(original_error)
            }
        }
    }

    /* ---------------------------------------------------------------------
       Episode & sumber video
       --------------------------------------------------------------------- */

    pub async fn episode(&self, raw_slug: &str) -> Result<EpisodeDetail> {
        let (requested_series_slug, episode_number) = parse_episode_reference(raw_slug);
        let (html, detail, anime_slug) = self
            .resolve_episode_series(&requested_series_slug)
            .await?;

        let entries = parse_stream_entries(&html);
        let mut episode_entries: Vec<StreamEntry> = entries
            .iter()
            .filter(|entry| entry.episode == Some(episode_number))
            .cloned()
            .collect();

        // Buang duplikat persis (id + provider + kualitas).
        let mut seen = HashSet::new();
        episode_entries.retain(|entry| {
            seen.insert(format!("{}|{}|{}", entry.id, entry.provider, entry.quality))
        });

        let mut resolved: HashMap<String, ResolvedStream> = HashMap::new();
        let mut jobs = JoinSet::new();
        for entry in &episode_entries {
            if !is_resolvable_player(&entry.play_url) {
                continue;
            }
            let page = entry.play_url.clone();
            let cached_key = page.clone();
            if let Some(body) = self.cache.fresh(&cached_key) {
                resolved.insert(page.clone(), parse_stream_page(&body));
                continue;
            }
            let client = self.client.clone();
            let base = self.base_url.clone();
            jobs.spawn(async move {
                let result = fetch_stream_page(&client, &base, &page).await;
                (page, result)
            });
        }

        while let Some(joined) = jobs.join_next().await {
            if let Ok((page, Some((body, stream)))) = joined {
                self.cache.put(page.clone(), body, PAGE_TTL);
                resolved.insert(page, stream);
            }
        }

        let mut streams: Vec<StreamSource> = Vec::new();

        for entry in &episode_entries {
            let quality = if entry.quality.trim().is_empty() {
                "original".to_string()
            } else {
                entry.quality.clone()
            };
            let rank = quality_rank(&quality);
            let page_url = absolute_url(&entry.play_url, &self.base_url);

            if is_resolvable_player(&entry.play_url) {
                match resolved.get(&entry.play_url) {
                    Some(stream)
                        if !stream.src.is_empty()
                            && is_allowed_video_url(&resolve_stream_url(
                                &stream.src,
                                &entry.play_url,
                                &self.base_url,
                            )) =>
                    {
                        let media_url = resolve_stream_url(
                            &stream.src,
                            &entry.play_url,
                            &self.base_url,
                        );
                        streams.push(StreamSource {
                            quality,
                            rank,
                            label: entry.label.clone(),
                            url: media_url.clone(),
                            play_url: format!("/api/video?url={}", encode(&media_url)),
                            page: page_url,
                            direct: true,
                            available: false,
                            note: String::new(),
                        });
                    }
                    _ => streams.push(StreamSource {
                        quality,
                        rank,
                        label: entry.label.clone(),
                        url: page_url.clone(),
                        play_url: page_url.clone(),
                        page: page_url,
                        direct: false,
                        available: false,
                        note: "Server tidak merespons".into(),
                    }),
                }
            } else {
                streams.push(StreamSource {
                    quality,
                    rank,
                    label: entry.label.clone(),
                    url: page_url.clone(),
                    play_url: page_url.clone(),
                    page: page_url,
                    direct: false,
                    available: false,
                    note: "Server eksternal".into(),
                });
            }
        }

        // Cek kesehatan semua sumber (MP4 langsung maupun embed) secara paralel.
        let mut probe_targets: Vec<String> = Vec::new();
        let mut seen_targets = HashSet::new();
        for stream in &streams {
            if stream.url.starts_with("http") && seen_targets.insert(stream.url.clone()) {
                probe_targets.push(stream.url.clone());
            }
        }

        self.refresh_health_in_background(probe_targets.into_iter().take(32).collect());

        for stream in &mut streams {
            // Jangan menahan halaman tonton hanya untuk mengecek setiap server.
            // Status yang belum diketahui dianggap siap; pemutar akan berpindah
            // ke server berikutnya bila sumber gagal saat benar-benar diputar.
            stream.available = self.cached_health(&stream.url).unwrap_or(true);
            if !stream.available {
                stream.note = "Tidak dapat dibuka".into();
            } else if stream.direct {
                stream.note.clear();
            } else {
                stream.note = "Buka di tab baru".into();
            }
        }

        // Kualitas: satu opsi per resolusi, memakai server terbaik yang hidup.
        let qualities = build_qualities(&streams);

        // Urutkan: langsung & hidup lebih dulu, lalu resolusi tertinggi.
        streams.sort_by(|a, b| {
            b.available
                .cmp(&a.available)
                .then(b.direct.cmp(&a.direct))
                .then(b.rank.cmp(&a.rank))
                .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });

        let best_url = qualities
            .iter()
            .find(|quality| quality.available && !quality.url.is_empty())
            .map(|quality| quality.url.clone())
            .unwrap_or_default();

        let servers: Vec<VideoServer> = streams
            .iter()
            .filter(|stream| !stream.direct && stream.available)
            .take(12)
            .map(|stream| VideoServer {
                name: if stream.quality.is_empty() {
                    stream.label.clone()
                } else {
                    format!("{} · {}", stream.label, stream.quality)
                },
                src: stream.url.clone(),
            })
            .collect();

        let downloads = build_downloads(&episode_entries, &self.base_url);

        let episode_numbers: Vec<u32> = entries.iter().filter_map(|entry| entry.episode).collect();
        let max_episode = episode_numbers.iter().copied().max().unwrap_or(episode_number);
        let prev_slug = if episode_number > 1 {
            format!("{anime_slug}-ep-{}", episode_number - 1)
        } else {
            String::new()
        };
        let next_slug = if episode_number < max_episode {
            format!("{anime_slug}-ep-{}", episode_number + 1)
        } else {
            String::new()
        };

        let recommended = self.latest_cached(12);

        Ok(EpisodeDetail {
            slug: clean_slug(raw_slug),
            title: format!("{} Episode {}", detail.title, episode_number),
            series_detail: detail.clone(),
            series_title: detail.title,
            series_slug: anime_slug.to_string(),
            series_image: detail.image,
            episode_number: episode_number.to_string(),
            released: String::new(),
            kind: detail.kind,
            embed_url: best_url,
            source_url: format!("{}/{anime_slug}/#nonton", self.base_url),
            servers,
            streams,
            qualities,
            downloads,
            prev_slug,
            next_slug,
            recommended,
        })
    }

    /* ---------------------------------------------------------------------
       Jadwal MyAnimeList
       --------------------------------------------------------------------- */

    pub async fn schedule(&self, week_start: NaiveDate) -> Result<Vec<ScheduleDay>> {
        let cache_key = "mal:schedules:v2";
        let body = if let Some(body) = self.cache.fresh(cache_key) {
            body
        } else {
            let response = self.mal_client.get(&self.schedule_api_url).send().await;
            match response {
                Ok(response) if response.status().is_success() => {
                    let body = response.text().await?;
                    self.cache.put(
                        cache_key.to_string(),
                        body.clone(),
                        StdDuration::from_secs(6 * 60 * 60),
                    );
                    body
                }
                Ok(response) => self.cache.stale(cache_key).ok_or_else(|| {
                    anyhow!("Sumber jadwal MyAnimeList mengembalikan HTTP {}", response.status())
                })?,
                Err(error) => self
                    .cache
                    .stale(cache_key)
                    .ok_or_else(|| anyhow!("gagal mengakses sumber jadwal MyAnimeList: {error}"))?,
            }
        };

        let payload: Value = serde_json::from_str(&body)
            .map_err(|error| anyhow!("format jadwal MyAnimeList tidak terbaca: {error}"))?;
        let data = payload
            .get("data")
            .ok_or_else(|| anyhow!("sumber jadwal MyAnimeList tidak mengirim data"))?;
        let mut entries_by_weekday: [Vec<Card>; 7] = std::array::from_fn(|_| Vec::new());

        if let Some(items) = data.as_array() {
            for item in items {
                if let Some((weekday, card)) = parse_mal_schedule_card(item, None) {
                    entries_by_weekday[weekday].push(card);
                }
            }
        } else if let Some(days) = data.as_object() {
            for (day_name, items) in days {
                let Some(weekday) = mal_weekday_index(day_name) else {
                    continue;
                };
                let Some(items) = items.as_array() else {
                    continue;
                };
                for item in items {
                    if let Some((_, card)) = parse_mal_schedule_card(item, Some(weekday)) {
                        entries_by_weekday[weekday].push(card);
                    }
                }
            }
        } else {
            return Err(anyhow!("format jadwal MyAnimeList tidak dikenali"));
        }

        if entries_by_weekday.iter().all(Vec::is_empty) {
            return Err(anyhow!("jadwal MyAnimeList kosong atau format datanya berubah"));
        }

        let jakarta = FixedOffset::east_opt(7 * 60 * 60).expect("offset WIB valid");
        let today = Utc::now().with_timezone(&jakarta).date_naive();

        let weekdays = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];
        let months = [
            "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
        ];
        let mut days = Vec::with_capacity(7);

        for offset in 0..7 {
            let date = week_start + ChronoDuration::days(offset as i64);
            let weekday_index = date.weekday().num_days_from_monday() as usize;
            let mut entries = std::mem::take(&mut entries_by_weekday[weekday_index]);
            entries.sort_by(|a, b| a.time.cmp(&b.time).then(a.title.cmp(&b.title)));
            let weekday = weekdays[weekday_index].to_string();
            let month_short = months[date.month0() as usize].to_string();
            days.push(ScheduleDay {
                day: format!("{} · {}", weekday, indonesian_date(date)),
                weekday,
                date_number: date.day().to_string(),
                month_short,
                id: date.format("%Y-%m-%d").to_string(),
                is_today: date == today,
                entries,
            });
        }

        Ok(days)
    }

    fn cached_health(&self, url: &str) -> Option<bool> {
        let now = Instant::now();
        let entries = self.health.read().ok()?;
        let (checked_at, ok) = entries.get(url).copied()?;
        (now.duration_since(checked_at) < HEALTH_TTL).then_some(ok)
    }

    fn store_health(health: &RwLock<HashMap<String, (Instant, bool)>>, url: String, ok: bool) {
        let Ok(mut entries) = health.write() else {
            return;
        };
        entries.insert(url, (Instant::now(), ok));
        if entries.len() > 800 {
            let cutoff = Instant::now()
                .checked_sub(HEALTH_TTL)
                .unwrap_or_else(Instant::now);
            entries.retain(|_, (checked_at, _)| *checked_at >= cutoff);
        }
    }

    fn refresh_health_in_background(&self, urls: Vec<String>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let client = self.client.clone();
        let base = self.base_url.clone();
        let health = self.health.clone();
        handle.spawn(async move {
            let mut jobs = JoinSet::new();
            for url in urls {
                let is_fresh = health
                    .read()
                    .ok()
                    .and_then(|entries| entries.get(&url).copied())
                    .is_some_and(|(checked_at, _)| checked_at.elapsed() < HEALTH_TTL);
                if is_fresh {
                    continue;
                }
                let client = client.clone();
                let base = base.clone();
                jobs.spawn(async move {
                    let ok = probe_url(&client, &url, &base).await;
                    (url, ok)
                });
            }

            while let Some(joined) = jobs.join_next().await {
                if let Ok((url, ok)) = joined {
                    Self::store_health(&health, url, ok);
                }
            }
        });
    }
}

/* -------------------------------------------------------------------------
   Stream entry parsing (RSC payload Nimegami)
   ------------------------------------------------------------------------- */

/// Mengambil satu halaman dari sumber. Dipakai baik untuk permintaan langsung
/// maupun untuk pembaruan cache di latar belakang.
async fn fetch_body(client: &reqwest::Client, url: &str) -> Result<String> {
    match client.get(url).send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return Err(anyhow!("Nimegami mengembalikan HTTP {status} untuk {url}"));
            }
            let body = response
                .text()
                .await
                .map_err(|error| anyhow!("gagal membaca respons Nimegami: {error}"))?;
            if body.trim().is_empty() {
                return Err(anyhow!("respons Nimegami kosong untuk {url}"));
            }
            Ok(body)
        }
        Err(error) => Err(anyhow!("tidak bisa menghubungi Nimegami: {error}")),
    }
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct StreamEntry {
    id: String,
    play_url: String,
    download_url: String,
    mode: String,
    provider: String,
    label: String,
    quality: String,
    episode: Option<u32>,
    kind: String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct ResolvedStream {
    src: String,
    quality: String,
    title: String,
}

fn parse_stream_entries(html: &str) -> Vec<StreamEntry> {
    let unescaped = html.replace("\\\"", "\"").replace("\\/", "/");
    let mut entries = Vec::new();

    for piece in unescaped.split("{\"id\":\"").skip(1) {
        let end = piece.find('"').unwrap_or(0);
        let id = piece[..end].to_string();
        let rest = &piece[..piece.len().min(6000)];

        let play_url = json_field(rest, "playUrl").unwrap_or_default();
        if play_url.is_empty() || !play_url.starts_with("http") && !play_url.starts_with('/') {
            continue;
        }

        let episode = json_number(rest, "episode");
        let kind = json_field(rest, "kind").unwrap_or_default();
        if kind != "episode" && episode.is_none() {
            continue;
        }

        entries.push(StreamEntry {
            id,
            play_url,
            download_url: json_field(rest, "url").unwrap_or_default(),
            mode: json_field(rest, "mode").unwrap_or_default(),
            provider: json_field(rest, "provider").unwrap_or_default(),
            label: json_field(rest, "label")
                .or_else(|| json_field(rest, "provider"))
                .unwrap_or_else(|| "Server".into()),
            quality: json_field(rest, "quality").unwrap_or_default(),
            episode,
            kind,
        });
    }

    entries
}

fn json_field(source: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = source.find(&needle)? + needle.len();
    let rest = &source[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    if value.is_empty() || value == "$undefined" {
        None
    } else {
        Some(decode_json_escapes(value))
    }
}

/// Mengubah escape JSON (`\u0026`, `\u003c`, `\n`, ...) menjadi karakter asli.
///
/// RSC payload Nimegami menyimpan `&` sebagai `\u0026`, sehingga URL embed
/// eksternal rusak jika escape dibiarkan.
fn decode_json_escapes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('u') => {
                chars.next();
                let mut hex = String::with_capacity(4);
                for _ in 0..4 {
                    match chars.next() {
                        Some(h) => hex.push(h),
                        None => break,
                    }
                }
                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                    if let Some(decoded) = char::from_u32(code) {
                        out.push(decoded);
                    }
                }
            }
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some('r') => {
                chars.next();
                out.push('\r');
            }
            Some('"') => {
                chars.next();
                out.push('"');
            }
            Some('\\') => {
                chars.next();
                out.push('\\');
            }
            Some('/') => {
                chars.next();
                out.push('/');
            }
            _ => out.push(ch),
        }
    }
    out
}

fn json_number(source: &str, key: &str) -> Option<u32> {
    let needle = format!("\"{key}\":");
    let start = source.find(&needle)? + needle.len();
    let rest = &source[start..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn is_nimegami_stream(play_url: &str) -> bool {
    if play_url.starts_with('/') {
        return play_url.contains("/streaming/");
    }
    let Ok(url) = reqwest::Url::parse(play_url) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    (host == "nimegami.id" || host.ends_with(".nimegami.id"))
        && url.path().contains("/streaming/")
}

fn is_resolvable_player(play_url: &str) -> bool {
    if is_nimegami_stream(play_url) {
        return true;
    }
    reqwest::Url::parse(play_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "workers-anime.berkasdrive.com")
}

async fn fetch_stream_page(
    client: &reqwest::Client,
    base: &str,
    path: &str,
) -> Option<(String, ResolvedStream)> {
    let url = if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{base}{path}")
    };
    let response = client
        .get(&url)
        .header(reqwest::header::REFERER, format!("{base}/"))
        .header(reqwest::header::ORIGIN, base)
        .timeout(StdDuration::from_secs(8))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    Some((body.clone(), parse_stream_page(&body)))
}

fn parse_stream_page(html: &str) -> ResolvedStream {
    let document = Html::parse_document(html);
    let player = document.select(&sel("main#player")).next();
    let video = document.select(&sel("video[src]")).next();
    let source = document.select(&sel("video source[src]")).next();
    let src = player
        .map(|element| attr(element, "data-src"))
        .filter(|value| !value.is_empty())
        .or_else(|| video.map(|element| attr(element, "src")))
        .or_else(|| source.map(|element| attr(element, "src")))
        .unwrap_or_default();
    ResolvedStream {
        src,
        quality: player
            .map(|element| attr(element, "data-quality"))
            .or_else(|| video.map(|element| attr(element, "data-quality")))
            .unwrap_or_default(),
        title: player
            .map(|element| attr(element, "data-title"))
            .or_else(|| video.map(|element| attr(element, "data-title")))
            .unwrap_or_default(),
    }
}

fn resolve_stream_url(src: &str, player_url: &str, base_url: &str) -> String {
    if let Ok(player_url) = reqwest::Url::parse(player_url) {
        if let Ok(url) = player_url.join(src) {
            return url.into();
        }
    }
    absolute_url(src, base_url)
}

async fn probe_url(client: &reqwest::Client, url: &str, base: &str) -> bool {
    // HEAD lebih murah; sebagian host MP4 menolak HEAD, jadi coba Range GET.
    // Samakan Referer, Origin, dan Accept dengan proxy pemutar agar server yang
    // menolak pemeriksaan tanpa header browser tidak salah dianggap mati.
    let origin = reqwest::Url::parse(base)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| base.trim_end_matches('/').to_string());
    let referer = format!("{}/", base.trim_end_matches('/'));
    let head = client
        .head(url)
        .header(reqwest::header::REFERER, &referer)
        .header(reqwest::header::ORIGIN, &origin)
        .header(reqwest::header::ACCEPT, "video/*,*/*;q=0.8")
        .header(reqwest::header::RANGE, "bytes=0-0")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await;

    if let Ok(response) = head {
        let status = response.status().as_u16();
        if (200..400).contains(&status) {
            return true;
        }
    }

    match client
        .get(url)
        .header(reqwest::header::REFERER, &referer)
        .header(reqwest::header::ORIGIN, &origin)
        .header(reqwest::header::ACCEPT, "video/*,*/*;q=0.8")
        .header(reqwest::header::RANGE, "bytes=0-1023")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status().as_u16();
            (200..400).contains(&status)
        }
        Err(_) => false,
    }
}

fn build_qualities(streams: &[StreamSource]) -> Vec<QualityOption> {
    let mut grouped: BTreeMap<u32, Vec<&StreamSource>> = BTreeMap::new();
    for stream in streams.iter().filter(|stream| stream.direct) {
        grouped.entry(stream.rank).or_default().push(stream);
    }

    let mut qualities = Vec::new();
    for (rank, group) in grouped.into_iter().rev() {
        let quality = group
            .first()
            .map(|stream| stream.quality.clone())
            .unwrap_or_else(|| format!("{rank}p"));

        let available = group.iter().copied().find(|stream| stream.available);
        let chosen = available.or_else(|| group.first().copied());

        if let Some(chosen) = chosen {
            qualities.push(QualityOption {
                quality,
                rank,
                available: available.is_some(),
                url: chosen.url.clone(),
                server: chosen.label.clone(),
                count: group.len(),
            });
        }
    }

    // Pastikan opsi tertinggi lebih dulu, "original" paling akhir.
    qualities.sort_by(|a, b| b.rank.cmp(&a.rank));
    qualities
}

fn build_downloads(entries: &[StreamEntry], base: &str) -> Vec<DownloadLink> {
    let mut seen = HashSet::new();
    let mut downloads = Vec::new();

    for entry in entries {
        let url = if entry.download_url.is_empty() {
            continue;
        } else {
            absolute_url(&entry.download_url, base)
        };
        let key = format!("{}|{}", entry.quality, url);
        if !seen.insert(key) {
            continue;
        }
        downloads.push(DownloadLink {
            server: entry.label.clone(),
            quality: entry.quality.clone(),
            url,
        });
        if downloads.len() >= 16 {
            break;
        }
    }

    downloads
}

fn quality_rank(quality: &str) -> u32 {
    let trimmed = quality.trim().trim_end_matches('p').trim_end_matches('P');
    trimmed.parse::<u32>().unwrap_or(0)
}

/* -------------------------------------------------------------------------
   Parsing beranda & katalog
   ------------------------------------------------------------------------- */

fn parse_home_html(html: &str, base: &str) -> (Vec<Card>, Vec<Card>, Vec<HeroSlide>) {
    let document = Html::parse_document(html);
    let root = document.root_element();
    let website_hero = parse_hero(root, base);
    let mut latest = Vec::new();
    let mut popular = Vec::new();

    for section in document.select(&sel("section.content-section")) {
        let heading = first_text_el(section, "h2").to_lowercase();
        let cards = parse_cards(section, base, 30);
        if cards.is_empty() {
            continue;
        }
        if heading.contains("ramai") || heading.contains("populer") {
            if popular.is_empty() {
                popular = cards;
            }
        } else if latest.is_empty() {
            latest = cards;
        } else if popular.is_empty() {
            popular = cards;
        }
    }

    if latest.is_empty() {
        latest = parse_cards(root, base, 30);
    }
    if popular.is_empty() {
        popular = latest.iter().take(10).cloned().collect();
    }

    (latest, popular, website_hero)
}

fn parse_hero(scope: ElementRef<'_>, base: &str) -> Vec<HeroSlide> {
    let mut slides = Vec::new();
    let mut seen = HashSet::new();

    for slide in scope.select(&sel(".hero-slide")) {
        let raw_title = first_text_el(slide, ".hero-title");
        let title = raw_title.trim().trim_end_matches('.').trim().to_string();
        if title.is_empty() {
            continue;
        }

        let image = slide
            .select(&sel("img"))
            .next()
            .map(|img| extract_asset_url(&attr(img, "srcset"), &attr(img, "src"), base))
            .unwrap_or_default();

        let href = slide
            .select(&sel("a[href]"))
            .next()
            .map(|anchor| attr(anchor, "href"))
            .unwrap_or_default();
        let slug = href
            .trim_start_matches('/')
            .split(['/', '#'])
            .next()
            .unwrap_or_default()
            .to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
            continue;
        }

        let meta: Vec<String> = slide
            .select(&sel(".hero-meta span"))
            .map(|item| text_of(item))
            .filter(|value| !value.is_empty())
            .collect();

        let kind_label = meta.first().cloned().unwrap_or_default();

        slides.push(HeroSlide {
            slug: slug.clone(),
            title,
            href: format!("/anime/{}", encode(&slug)),
            image,
            synopsis: first_text_el(slide, ".hero-synopsis"),
            rating: first_text_el(slide, ".hero-rating strong"),
            kind_label,
            meta,
        });

        if slides.len() >= 5 {
            break;
        }
    }

    slides
}

fn mal_entry_titles(entry: &Value) -> Vec<&str> {
    let mut titles = Vec::new();
    for key in [
        "title",
        "title_english",
        "titleEnglish",
        "title_japanese",
        "titleJapanese",
    ] {
        if let Some(title) = entry.get(key).and_then(Value::as_str)
            && !title.trim().is_empty()
        {
            titles.push(title);
        }
    }
    for key in ["title_synonyms", "titleSynonyms"] {
        if let Some(synonyms) = entry.get(key).and_then(Value::as_array) {
            titles.extend(
                synonyms
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|title| !title.trim().is_empty()),
            );
        }
    }
    if let Some(alternates) = entry.get("titles").and_then(Value::as_array) {
        for item in alternates {
            if let Some(title) = item.get("title").and_then(Value::as_str) {
                titles.push(title);
            } else if let Some(title) = item.as_str() {
                titles.push(title);
            }
        }
    }
    titles
}

fn mal_entry_is_adult(entry: &Value) -> bool {
    let has_adult_genre = ["genres", "explicit_genres"]
        .iter()
        .filter_map(|key| entry.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(|genre| genre.get("name").and_then(Value::as_str))
        .any(is_adult_genre);
    let rating = entry
        .get("rating")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    has_adult_genre || rating.contains("hentai") || rating.contains("erotica") || rating.starts_with("rx")
}

/// Poster/trailer dari MyAnimeList untuk dipakai sebagai gambar cadangan.
fn mal_entry_image(entry: &Value) -> Option<String> {
    for path in [
        "/trailer/images/maximum_image_url",
        "/trailer/images/large_image_url",
        "/trailer/images/medium_image_url",
        "/images/jpg/large_image_url",
        "/images/webp/large_image_url",
        "/images/jpg/image_url",
    ] {
        if let Some(url) = entry
            .pointer(path)
            .and_then(Value::as_str)
            .filter(|url| !url.trim().is_empty())
        {
            return Some(url.to_string());
        }
    }
    entry
        .get("imageUrl")
        .or_else(|| entry.get("image_url"))
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .map(str::to_string)
}

/// Tahun rilis dari skema Jikan v4 maupun jikan-edge (`startDate`).
fn mal_entry_year(entry: &Value) -> Option<String> {
    if let Some(year) = entry
        .pointer("/aired/prop/from/year")
        .and_then(Value::as_i64)
    {
        return Some(year.to_string());
    }
    for path in ["/aired/from", "/startDate", "/start_date"] {
        if let Some(raw) = entry.pointer(path).and_then(Value::as_str) {
            let digits: String = raw
                .chars()
                .filter(|character| character.is_ascii_digit())
                .take(4)
                .collect();
            if digits.len() == 4 {
                return Some(digits);
            }
        }
    }
    None
}

/// Label jumlah penonton, dipakai jikan-edge yang tidak mengirim `popularity`.
fn mal_entry_members_label(entry: &Value) -> Option<String> {
    let members = entry.get("members").and_then(Value::as_u64)?;
    if members == 0 {
        return None;
    }
    Some(if members >= 1_000_000 {
        format!("{:.1} jt anggota", members as f64 / 1_000_000.0).replace('.', ",")
    } else if members >= 1_000 {
        format!("{:.0} rb anggota", members as f64 / 1_000.0)
    } else {
        format!("{members} anggota")
    })
}

/// Menyusun hero beranda dari daftar popularitas MAL yang sudah dicocokkan
/// dengan katalog Nimegami.
fn build_popularity_hero(
    entries: &[Value],
    latest: &[Card],
    popular: &[Card],
    website_hero: &[HeroSlide],
) -> Vec<HeroSlide> {
    let mut catalog: Vec<Card> = Vec::new();
    let mut seen_slugs = HashSet::new();
    for card in popular.iter().chain(latest.iter()) {
        if !card.slug.is_empty() && seen_slugs.insert(card.slug.clone()) && !is_adult_card(card) {
            catalog.push(card.clone());
        }
    }
    for slide in website_hero {
        if !slide.slug.is_empty()
            && !is_adult_genre(&slide.title)
            && !is_adult_genre(&slide.slug)
            && seen_slugs.insert(slide.slug.clone())
        {
            catalog.push(Card {
                slug: slide.slug.clone(),
                title: slide.title.clone(),
                image: slide.image.clone(),
                href: slide.href.clone(),
                kind_label: slide.kind_label.clone(),
                score: slide.rating.clone(),
                ..Default::default()
            });
        }
    }

    let mut slides = Vec::new();
    let mut matched_slugs = HashSet::new();
    for entry in entries {
        if mal_entry_is_adult(entry) {
            continue;
        }
        let titles = mal_entry_titles(entry);
        let matched = catalog
            .iter()
            .find(|card| {
                let site_title = normalize_title_for_dedup(&card.title);
                !site_title.is_empty()
                    && titles
                        .iter()
                        .any(|title| normalize_title_for_dedup(title) == site_title)
            })
            .or_else(|| {
                // Cadangan untuk judul yang sedikit berbeda (mis. tambahan
                // "Season 3"): cocokkan berdasarkan kemiripan token.
                catalog.iter().find(|card| {
                    titles.iter().any(|title| {
                        title_match_score(&title_tokens(title), &card.title, &card.slug) >= 0.92
                    })
                })
            });
        let Some(card) = matched else { continue };
        if !matched_slugs.insert(card.slug.clone()) {
            continue;
        }

        let source_slide = website_hero.iter().find(|slide| slide.slug == card.slug);
        // Utamakan gambar Nimegami sendiri: filter `backdrop` mengambil versi
        // 1080px, sedangkan `imageUrl` MAL hanya cover kecil.
        let image = source_slide
            .map(|slide| slide.image.clone())
            .filter(|image| !image.is_empty())
            .or_else(|| (!card.image.is_empty()).then(|| card.image.clone()))
            .or_else(|| mal_entry_image(entry))
            .unwrap_or_default();
        let synopsis = entry
            .get("synopsis")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .split("[Written by MAL Rewrite]")
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        let synopsis = if synopsis.is_empty() {
            source_slide
                .map(|slide| slide.synopsis.clone())
                .unwrap_or_default()
        } else {
            synopsis
        };
        let rating = entry
            .get("score")
            .and_then(Value::as_f64)
            .map(|score| format!("{score:.1}"))
            .unwrap_or_else(|| card.score.clone());
        let kind_label = if !card.kind_label.is_empty() {
            card.kind_label.clone()
        } else {
            entry
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let year = mal_entry_year(entry)
            .filter(|year| !year.is_empty())
            .unwrap_or_else(|| card.year.clone());
        let mut meta = Vec::new();
        if !year.is_empty() {
            meta.push(year);
        }
        if let Some(popularity) = entry.get("popularity").and_then(Value::as_u64) {
            meta.push(format!("MAL Populer #{popularity}"));
        } else if let Some(members) = mal_entry_members_label(entry) {
            meta.push(members);
        }

        slides.push(HeroSlide {
            slug: card.slug.clone(),
            title: card.title.clone(),
            href: card.href.clone(),
            image,
            synopsis,
            rating,
            kind_label,
            meta,
        });
        if slides.len() == 10 {
            break;
        }
    }

    slides
}

fn parse_cards(scope: ElementRef<'_>, base: &str, limit: usize) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut seen = HashSet::new();

    for element in scope.select(&sel("article.anime-card")) {
        let anchor = element
            .select(&sel("a.poster-link"))
            .next()
            .or_else(|| element.select(&sel("a.card-title")).next())
            .or_else(|| element.select(&sel("a")).next());

        let Some(anchor) = anchor else { continue };
        let href = attr(anchor, "href");
        let slug = href.trim_matches('/').split('/').next().unwrap_or("").to_string();
        if slug.is_empty() || slug == "#" || !seen.insert(slug.clone()) {
            continue;
        }

        let mut title = attr(anchor, "aria-label")
            .trim_start_matches("Lihat ")
            .trim()
            .to_string();
        if title.is_empty() {
            title = first_text_el(element, ".card-title");
        }
        if title.is_empty() {
            title = first_text_el(element, "a");
        }
        if title.is_empty() {
            continue;
        }

        let image = element
            .select(&sel("img"))
            .next()
            .map(|img| extract_asset_url(&attr(img, "srcset"), &attr(img, "src"), base))
            .unwrap_or_default();

        let format = first_text_el(element, ".format-badge");
        let score = first_text_el(element, ".score-badge");
        let last_episode = first_text_el(element, ".card-last-episode strong");
        let bottom = first_text_el(element, ".poster-bottom");
        let updated = first_text_el(element, ".card-updated time");
        let updated = if updated.is_empty() {
            first_text_el(element, ".card-updated")
        } else {
            updated
        };
        let updated_at = element
            .select(&sel(".card-updated time"))
            .next()
            .map(|time| attr(time, "datetime"))
            .unwrap_or_default();

        let meta_text = first_text_el(element, ".card-meta");
        let mut tokens = meta_text.split_whitespace();
        let year = tokens
            .next()
            .filter(|token| token.len() == 4 && token.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or("")
            .to_string();
        let genres: Vec<String> = meta_text
            .split_whitespace()
            .skip(if year.is_empty() { 0 } else { 1 })
            .filter(|token| !token.is_empty() && token.chars().any(|c| c.is_alphabetic()))
            .map(|token| token.to_string())
            .collect();

        let numeric_ep = last_episode
            .trim()
            .trim_start_matches("Episode")
            .trim();
        let episode = if !numeric_ep.is_empty()
            && numeric_ep != "—"
            && numeric_ep != "-"
            && numeric_ep.chars().all(|c| c.is_ascii_digit())
        {
            format!("Eps {numeric_ep}")
        } else {
            String::new()
        };

        let kind_label = match format.to_lowercase().as_str() {
            "movie" => "Movie".to_string(),
            "series" | "tv" => "Series".to_string(),
            "ova" => "OVA".to_string(),
            "ona" => "ONA".to_string(),
            "special" => "Special".to_string(),
            _ => format.clone(),
        };

        let card = Card {
            slug: slug.clone(),
            title,
            image,
            href: format!("/anime/{}", encode(&slug)),
            kind: format.to_lowercase(),
            status: String::new(),
            kind_label,
            episode,
            time: updated,
            total: bottom,
            rank: 0,
            genres,
            provider: "Nimegami".into(),
            score,
            year,
            updated_at,
        };

        if is_adult_card(&card) {
            continue;
        }

        cards.push(card);
        if cards.len() >= limit {
            break;
        }
    }

    cards
}

fn parse_genres(document: &Html) -> Vec<Genre> {
    let mut genres = Vec::new();
    let mut seen = HashSet::new();

    for item in document.select(&sel("input[name='genre']")) {
        let value = attr(item, "value");
        if value.is_empty() || is_adult_genre(&value) {
            continue;
        }
        let slug = value.to_lowercase().replace(' ', "-");
        if !seen.insert(slug.clone()) {
            continue;
        }
        genres.push(Genre {
            name: value.clone(),
            href: format!("/anime?genre={}", encode(&value)),
            slug,
        });
    }

    if genres.is_empty() {
        return default_genres();
    }

    genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    genres
}

fn default_genres() -> Vec<Genre> {
    [
        "Action",
        "Adventure",
        "Comedy",
        "Drama",
        "Fantasy",
        "Isekai",
        "Romance",
        "Sci-Fi",
        "Slice of Life",
        "Sports",
        "Supernatural",
        "Thriller",
    ]
    .into_iter()
    .map(|name| Genre {
        name: name.to_string(),
        slug: name.to_lowercase().replace(' ', "-"),
        href: format!("/anime?genre={}", encode(name)),
    })
    .collect()
}

fn has_next_page(document: &Html, current: u32) -> bool {
    for anchor in document.select(&sel("a[href]")) {
        let href = attr(anchor, "href");
        if let Some(index) = href.find("page=") {
            let digits: String = href[index + 5..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(page) = digits.parse::<u32>() {
                if page > current {
                    return true;
                }
            }
        }
    }
    false
}

/* -------------------------------------------------------------------------
   Detail anime
   ------------------------------------------------------------------------- */

fn build_detail(html: &str, slug: &str, base: &str) -> Result<AnimeDetail> {
    let document = Html::parse_document(html);
    let root = document.root_element();

    let (series, json_episodes) = parse_json_ld(html);

    let mut title = first_text_el(root, ".post-title-block h1");
    if title.is_empty() {
        title = series
            .as_ref()
            .and_then(|series| series.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    if title.is_empty() {
        tracing::warn!(
            len = html.len(),
            has_block = html.contains("post-title-block"),
            has_notfound = html.contains("Anime tidak ditemukan"),
            "detail nimegami tanpa judul"
        );
        return Err(anyhow!("Anime tidak ditemukan di Nimegami"));
    }

    let mut raw_genres: Vec<String> = root
        .select(&sel(".post-genres a"))
        .map(|item| text_of(item))
        .filter(|value| !value.is_empty())
        .collect();

    if raw_genres.is_empty() {
        raw_genres = series
            .as_ref()
            .and_then(|series| series.get("genre"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
    }

    if raw_genres.iter().any(|genre| is_adult_genre(genre))
        || is_adult_card(&Card {
            title: title.clone(),
            slug: slug.to_string(),
            genres: raw_genres.clone(),
            ..Default::default()
        })
    {
        return Err(anyhow!("Anime diblokir karena mengandung genre 18+ / dewasa"));
    }

    let genres: Vec<Genre> = raw_genres
        .iter()
        .map(|name| Genre {
            name: name.clone(),
            slug: name.to_lowercase().replace(' ', "-"),
            href: format!("/anime?genre={}", encode(name)),
        })
        .collect();

    let image = root
        .select(&sel(".detail-poster img"))
        .next()
        .map(|img| extract_asset_url(&attr(img, "srcset"), &attr(img, "src"), base))
        .or_else(|| {
            series
                .as_ref()
                .and_then(|series| series.get("image"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();

    let banner_image = root
        .select(&sel(".post-backdrop img"))
        .next()
        .map(|img| extract_asset_url(&attr(img, "srcset"), &attr(img, "src"), base))
        .unwrap_or_default();

    let score = first_attr_text(root, "[data-rating-score='poster']");
    let votes = first_text_el(root, ".anime-rating-count");
    let synopsis = first_text_el(root, ".synopsis-block .rich-content");
    let status_raw = first_text_el(root, ".status-badge");
    let status = if status_raw.contains("Tamat") || status_raw.contains("Completed") {
        "Completed".to_string()
    } else if status_raw.contains("Ongoing") {
        "Ongoing".to_string()
    } else if status_raw.contains("Upcoming") {
        "Upcoming".to_string()
    } else {
        status_raw
    };
    let kind = first_text_el(root, ".post-kicker span");
    let facts = parse_facts(root);

    let released = fact_value(&facts, "Mulai tayang")
        .or_else(|| {
            series
                .as_ref()
                .and_then(|series| series.get("startDate"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let studio = fact_value(&facts, "Studio").unwrap_or_default();
    let season = fact_value(&facts, "Musim").unwrap_or_default();
    let duration = fact_value(&facts, "Durasi").unwrap_or_default();
    let country = fact_value(&facts, "Negara").unwrap_or_default();
    let alt_title = fact_value(&facts, "Judul alternatif")
        .or_else(|| fact_value(&facts, "Judul asli"))
        .unwrap_or_default();

    let entries = parse_stream_entries(html);
    let mut numbers: Vec<u32> = entries.iter().filter_map(|entry| entry.episode).collect();
    numbers.sort_unstable();
    numbers.dedup();

    if numbers.is_empty() {
        for anchor in root.select(&sel(".episode-grid a")) {
            if let Ok(number) = text_of(anchor).parse::<u32>() {
                numbers.push(number);
            }
        }
        numbers.sort_unstable();
        numbers.dedup();
    }

    if numbers.is_empty() {
        let total = series
            .as_ref()
            .and_then(|series| series.get("numberOfEpisodes"))
            .and_then(Value::as_u64)
            .unwrap_or(json_episodes.len() as u64);
        for number in 1..=total.min(2000) {
            numbers.push(number as u32);
        }
    }

    let episodes: Vec<EpisodeLink> = numbers
        .iter()
        .rev()
        .map(|number| {
            let episode_slug = format!("{slug}-ep-{number}");
            EpisodeLink {
                number: number.to_string(),
                title: format!("Episode {number}"),
                slug: episode_slug.clone(),
                date: String::new(),
                href: format!("/watch/{}", encode(&episode_slug)),
            }
        })
        .collect();

    let mut info_facts = Vec::new();
    if !studio.is_empty() {
        info_facts.push(InfoItem {
            label: "Studio".into(),
            value: studio.clone(),
        });
    }
    if !season.is_empty() {
        info_facts.push(InfoItem {
            label: "Musim".into(),
            value: season.clone(),
        });
    }
    if !released.is_empty() {
        info_facts.push(InfoItem {
            label: "Rilis".into(),
            value: released.clone(),
        });
    }
    if !country.is_empty() {
        info_facts.push(InfoItem {
            label: "Negara".into(),
            value: country.clone(),
        });
    }
    info_facts.push(InfoItem {
        label: "Sumber".into(),
        value: "Nimegami".into(),
    });

    Ok(AnimeDetail {
        slug: slug.to_string(),
        title: title.clone(),
        alt_title,
        image,
        banner_image,
        trailer: String::new(),
        rating: score.clone(),
        score,
        votes,
        synopsis,
        status,
        studio,
        season,
        released,
        duration,
        kind,
        country,
        released_on: String::new(),
        updated_on: String::new(),
        facts: info_facts,
        genres,
        episodes,
        episode_count: numbers.len(),
    })
}

fn parse_facts(scope: ElementRef<'_>) -> Vec<InfoItem> {
    let mut facts = Vec::new();
    for row in scope.select(&sel(".anime-info-summary div, .anime-info-more div")) {
        let label = first_text_el(row, "dt");
        let value = first_text_el(row, "dd");
        if !label.is_empty() && !value.is_empty() && value != "Belum tersedia" {
            facts.push(InfoItem { label, value });
        }
    }
    facts
}

fn fact_value(facts: &[InfoItem], label: &str) -> Option<String> {
    facts
        .iter()
        .find(|fact| fact.label.eq_ignore_ascii_case(label))
        .map(|fact| fact.value.clone())
}

fn parse_json_ld(html: &str) -> (Option<Value>, Vec<Value>) {
    let mut series_opt = None;
    let mut episodes = Vec::new();

    let document = Html::parse_document(html);
    for element in document.select(&sel("script[type='application/ld+json']")) {
        let text = element.text().collect::<String>();
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };

        let items: Vec<Value> = if let Some(array) = json.get("@graph").and_then(Value::as_array) {
            array.clone()
        } else if json.is_object() {
            vec![json]
        } else {
            Vec::new()
        };

        for item in items {
            let types: Vec<&str> = item
                .get("@type")
                .map(|value| {
                    value
                        .as_str()
                        .map(|kind| vec![kind])
                        .or_else(|| {
                            value
                                .as_array()
                                .map(|items| items.iter().filter_map(Value::as_str).collect())
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            if types.iter().any(|kind| {
                matches!(
                    kind.to_ascii_lowercase().as_str(),
                    "tvseries" | "tvseason" | "tvshow" | "movie" | "anime" | "creativework"
                )
            }) {
                if series_opt.is_none() {
                    series_opt = Some(item);
                }
            } else if types
                .iter()
                .any(|kind| kind.eq_ignore_ascii_case("TVEpisode"))
            {
                episodes.push(item);
            }
        }
    }

    (series_opt, episodes)
}

/* -------------------------------------------------------------------------
   Utilitas kecil
   ------------------------------------------------------------------------- */

fn clean_slug(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("ng-")
        .trim_start_matches("nimegami:")
        .trim_matches('/')
        .to_string()
}

/// Menerima slug episode internal (`anime-ep-7`) dan slug lama dari URL sumber
/// (`anime-episode-7-subtitle-indonesia`). Riwayat lokal dapat menyimpan format
/// lama tersebut, jadi bagian judul episode harus dipisahkan sebelum mencari
/// halaman seri di Nimegami.
fn parse_episode_reference(raw: &str) -> (String, u32) {
    let slug = clean_slug(raw);

    if let Some((series_slug, episode)) = slug.rsplit_once("-ep-") {
        if let Ok(number) = episode.parse::<u32>() {
            return (series_slug.to_string(), number);
        }
    }

    if let Some((series_slug, suffix)) = slug.rsplit_once("-episode-") {
        let digits_len = suffix.bytes().take_while(u8::is_ascii_digit).count();
        if digits_len > 0
            && (digits_len == suffix.len() || suffix.as_bytes().get(digits_len) == Some(&b'-'))
        {
            if let Ok(number) = suffix[..digits_len].parse::<u32>() {
                return (series_slug.to_string(), number);
            }
        }
    }

    (slug, 1)
}

fn episode_series_search_query(slug: &str) -> String {
    let words: Vec<&str> = slug.split('-').filter(|word| !word.is_empty()).collect();
    let mut query = Vec::with_capacity(words.len());
    let mut index = 0;

    while index < words.len() {
        if index + 1 < words.len() && words[index + 1] == "season" {
            if let Some(number) = ordinal_season_number(words[index]) {
                query.push("season");
                query.push(number);
                index += 2;
                continue;
            }
        }
        query.push(words[index]);
        index += 1;
    }

    query.join(" ")
}

fn ordinal_season_number(ordinal: &str) -> Option<&'static str> {
    match ordinal {
        "1st" | "first" => Some("1"),
        "2nd" | "second" => Some("2"),
        "3rd" | "third" => Some("3"),
        "4th" | "fourth" => Some("4"),
        "5th" | "fifth" => Some("5"),
        _ => None,
    }
}

fn title_tokens(value: &str) -> HashSet<String> {
    const IGNORED: [&str; 6] = ["no", "ga", "sub", "indo", "subtitle", "indonesia"];
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| !token.is_empty() && !IGNORED.contains(&token.as_str()))
        .collect()
}

fn title_match_score(target: &HashSet<String>, title: &str, slug: &str) -> f32 {
    if target.is_empty() {
        return 0.0;
    }
    let candidate = title_tokens(&format!("{title} {slug}"));
    if candidate.is_empty() {
        return 0.0;
    }

    let shared = target.intersection(&candidate).count();
    let coverage = shared as f32 / target.len() as f32;
    let union = target.union(&candidate).count();
    let jaccard = shared as f32 / union as f32;
    coverage * 0.85 + jaccard * 0.15
}

fn absolute_url(href: &str, base: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if href.starts_with('/') {
        format!("{base}{href}")
    } else {
        format!("{base}/{href}")
    }
}

fn is_allowed_video_url(raw_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw_url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && (host.eq_ignore_ascii_case("berkasdrive.com")
            || host.to_ascii_lowercase().ends_with(".berkasdrive.com"))
}

/// Ambil URL aset terbaik dari srcset (resolusi terbesar) atau src.
fn extract_asset_url(srcset: &str, src: &str, base: &str) -> String {
    let mut candidates = Vec::new();
    for candidate in srcset.split(',') {
        let url = candidate.split_whitespace().next().unwrap_or("").trim();
        if !url.is_empty() {
            candidates.push(url);
        }
    }
    if !src.is_empty() {
        candidates.push(src.trim());
    }

    for candidate in candidates.iter().rev() {
        if let Some(index) = candidate.find("/assets/anime-data/") {
            let path = &candidate[index..];
            let path = path.split(['&', ' ', '"', '\'']).next().unwrap_or(path);
            return format!("{base}{path}");
        }
    }

    for candidate in candidates.iter().rev() {
        if !candidate.is_empty() {
            return absolute_url(candidate, base);
        }
    }

    String::new()
}

fn parse_mal_schedule_card(item: &Value, fallback_weekday: Option<usize>) -> Option<(usize, Card)> {
    let broadcast = item.get("broadcast");
    let weekday = broadcast
        .and_then(|value| value.get("day"))
        .and_then(Value::as_str)
        .and_then(mal_weekday_index)
        .or(fallback_weekday)?;
    let is_adult_genre = ["genres", "explicit_genres", "explicitGenres"]
        .iter()
        .filter_map(|key| item.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(|genre| genre.get("name").and_then(Value::as_str))
        .any(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("hentai") || name.contains("erotica")
        });
    let rating = item
        .get("rating")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if is_adult_genre || rating.contains("hentai") || rating.starts_with("rx") || rating == "r18" {
        return None;
    }

    let title = item
        .get("title_english")
        .or_else(|| item.get("titleEnglish"))
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .or_else(|| item.get("title").and_then(Value::as_str))?
        .trim()
        .to_string();
    let images = item.get("images");
    let image = item
        .get("imageUrl")
        .and_then(Value::as_str)
        .or_else(|| {
            images
                .and_then(|value| value.get("webp").or_else(|| value.get("jpg")))
                .and_then(|value| value.get("large_image_url").or_else(|| value.get("image_url")))
                .and_then(Value::as_str)
        })
        .or_else(|| images.and_then(|value| value.get("large").and_then(Value::as_str)))
        .unwrap_or_default()
        .to_string();
    let mal_id = item
        .get("mal_id")
        .or_else(|| item.get("malId"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let airing_time = broadcast
        .and_then(|value| value.get("time"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let score = item
        .get("score")
        .and_then(Value::as_f64)
        .filter(|score| *score > 0.0)
        .map(|score| format!("{score:.1}"))
        .unwrap_or_default();
    let year = item
        .get("year")
        .and_then(Value::as_i64)
        .or_else(|| item.get("year").and_then(Value::as_u64).map(|year| year as i64))
        .map(|year| year.to_string())
        .unwrap_or_default();

    Some((
        weekday,
        Card {
            slug: mal_id.to_string(),
            title: title.clone(),
            image,
            href: format!("/jadwal/anime?title={}", encode(&title)),
            kind: item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            kind_label: item
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            time: airing_time,
            provider: "MyAnimeList".into(),
            score,
            year,
            ..Default::default()
        },
    ))
}

fn mal_weekday_index(value: &str) -> Option<usize> {
    let weekday = value.trim().to_ascii_lowercase();
    if weekday.starts_with("mon") {
        Some(0)
    } else if weekday.starts_with("tue") {
        Some(1)
    } else if weekday.starts_with("wed") {
        Some(2)
    } else if weekday.starts_with("thu") {
        Some(3)
    } else if weekday.starts_with("fri") {
        Some(4)
    } else if weekday.starts_with("sat") {
        Some(5)
    } else if weekday.starts_with("sun") {
        Some(6)
    } else {
        None
    }
}

fn indonesian_date(date: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "Januari",
        "Februari",
        "Maret",
        "April",
        "Mei",
        "Juni",
        "Juli",
        "Agustus",
        "September",
        "Oktober",
        "November",
        "Desember",
    ];
    let month = MONTHS
        .get(date.month0() as usize)
        .copied()
        .unwrap_or_default();
    format!("{} {} {}", date.day(), month, date.year())
}

fn sel(query: &str) -> Selector {
    Selector::parse(query).expect("selector valid")
}

fn attr(element: ElementRef<'_>, name: &str) -> String {
    element
        .value()
        .attr(name)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn text_of(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_text_el(scope: ElementRef<'_>, query: &str) -> String {
    scope
        .select(&sel(query))
        .next()
        .map(text_of)
        .unwrap_or_default()
}

fn first_attr_text(scope: ElementRef<'_>, query: &str) -> String {
    scope
        .select(&sel(query))
        .next()
        .map(text_of)
        .unwrap_or_default()
}

/* -------------------------------------------------------------------------
   Cache HTML
   ------------------------------------------------------------------------- */

#[derive(Default)]
struct Cache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    /// Kunci yang sedang diperbarui di latar belakang, agar satu halaman tidak
    /// di-fetch berkali-kali saat banyak permintaan datang bersamaan.
    refreshing: RwLock<HashSet<String>>,
}

struct CacheEntry {
    body: String,
    stored_at: Instant,
    ttl: StdDuration,
}

impl Cache {
    fn fresh(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().ok()?;
        let entry = entries.get(key)?;
        (entry.stored_at.elapsed() < entry.ttl).then(|| entry.body.clone())
    }

    fn stale(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().ok()?;
        entries.get(key).map(|entry| entry.body.clone())
    }

    fn put(&self, key: String, body: String, ttl: StdDuration) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        entries.insert(
            key,
            CacheEntry {
                body,
                stored_at: Instant::now(),
                ttl,
            },
        );
        if entries.len() > 240 {
            let cutoff = Instant::now()
                .checked_sub(StdDuration::from_secs(2400))
                .unwrap_or_else(Instant::now);
            entries.retain(|_, entry| entry.stored_at >= cutoff);
        }
    }

    fn begin_refresh(&self, key: &str) -> bool {
        let Ok(mut keys) = self.refreshing.write() else {
            return false;
        };
        keys.insert(key.to_string())
    }

    fn end_refresh(&self, key: &str) {
        if let Ok(mut keys) = self.refreshing.write() {
            keys.remove(key);
        }
    }
}
