use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Result, anyhow};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use tokio::task::JoinSet;

use crate::models::{
    AnimeDetail, BrowsePage, BrowseParams, Card, DownloadLink, EpisodeDetail, EpisodeLink, Genre,
    InfoItem, VideoServer, encode, is_adult_card, is_adult_genre,
};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct NimegamiSource {
    base_url: String,
    client: reqwest::Client,
    cache: Cache,
    availability: RwLock<HashMap<String, (Instant, bool)>>,
}

impl NimegamiSource {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(StdDuration::from_secs(25))
            .connect_timeout(StdDuration::from_secs(10))
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
            client,
            cache: Cache::default(),
            availability: RwLock::new(HashMap::new()),
        })
    }

    #[allow(dead_code)]
    pub fn host(&self) -> String {
        "nimegami.id".to_string()
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

        match self.fetch(path).await {
            Ok(body) => {
                self.cache.put(key, body.clone(), ttl);
                Ok(body)
            }
            Err(error) => {
                if let Some(stale) = self.cache.stale(&key) {
                    tracing::warn!(path, %error, "sumber nimegami gagal, memakai cache lama");
                    Ok(stale)
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn fetch(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        let mut last_error = None;

        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(StdDuration::from_millis(700)).await;
            }

            match self.client.get(&url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if !status.is_success() {
                        last_error = Some(anyhow!("Nimegami mengembalikan HTTP {status}"));
                        if matches!(status.as_u16(), 403 | 429 | 500 | 502 | 503 | 504) {
                            continue;
                        }
                        break;
                    }

                    let body = match response.text().await {
                        Ok(body) => body,
                        Err(error) => {
                            last_error = Some(anyhow!("gagal membaca respons Nimegami: {error}"));
                            continue;
                        }
                    };

                    if body.trim().is_empty() {
                        last_error = Some(anyhow!("respons Nimegami kosong"));
                        continue;
                    }

                    return Ok(body);
                }
                Err(error) => {
                    last_error = Some(anyhow!("tidak bisa menghubungi Nimegami: {error}"));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("permintaan ke Nimegami gagal")))
    }

    fn cached_forbidden(&self, slug: &str, now: Instant, ttl: StdDuration) -> Option<bool> {
        let entries = self.availability.read().ok()?;
        let (checked_at, forbidden) = entries.get(slug).copied()?;
        (now.duration_since(checked_at) < ttl).then_some(forbidden)
    }

    fn cache_availability(&self, slug: String, forbidden: bool, ttl: StdDuration) {
        let Ok(mut entries) = self.availability.write() else {
            return;
        };
        entries.insert(slug, (Instant::now(), forbidden));
        if entries.len() > 900 {
            let cutoff = Instant::now().checked_sub(ttl).unwrap_or_else(Instant::now);
            entries.retain(|_, (checked_at, _)| *checked_at >= cutoff);
        }
    }

    /// Hapus kartu yang halaman detail Nimegami-nya secara eksplisit mengembalikan 403.
    /// Pemeriksaan hasil disimpan 15 menit agar daftar tidak mengulang permintaan yang sama.
    async fn hide_forbidden_cards(&self, cards: Vec<Card>) -> Vec<Card> {
        const TTL: StdDuration = StdDuration::from_secs(900);
        const CONCURRENCY: usize = 12;

        let now = Instant::now();
        let mut hidden = HashSet::new();
        let mut pending = Vec::new();

        for (index, card) in cards.iter().enumerate() {
            let Some(slug) = card.slug.strip_prefix("ng-") else {
                continue;
            };

            match self.cached_forbidden(slug, now, TTL) {
                Some(forbidden) => {
                    if forbidden {
                        hidden.insert(index);
                    }
                }
                None => pending.push((index, slug.to_string())),
            }
        }

        for batch in pending.chunks(CONCURRENCY) {
            let mut checks = JoinSet::new();
            for (index, slug) in batch.iter().cloned() {
                let client = self.client.clone();
                let url = self.url(&format!("/{slug}/"));
                checks.spawn(async move {
                    let unavailable = match client
                        .get(url)
                        .timeout(StdDuration::from_secs(3))
                        .send()
                        .await
                    {
                        Ok(response) => {
                            let status = response.status();
                            if matches!(status.as_u16(), 403 | 404 | 410) {
                                Some(true)
                            } else if status.is_success() {
                                match response.text().await {
                                    Ok(body) => {
                                        let (series, _) = parse_json_ld(&body);
                                        Some(!series
                                            .and_then(|value| {
                                                value
                                                    .get("name")
                                                    .and_then(Value::as_str)
                                                    .map(|name| !name.trim().is_empty())
                                            })
                                            .unwrap_or(false))
                                    }
                                    Err(_) => None,
                                }
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    };
                    (index, slug, unavailable)
                });
            }

            while let Some(Ok((index, slug, unavailable))) = checks.join_next().await {
                if let Some(unavailable) = unavailable {
                    if unavailable {
                        hidden.insert(index);
                    }
                    self.cache_availability(slug, unavailable, TTL);
                }
            }
        }

        cards
            .into_iter()
            .enumerate()
            .filter_map(|(index, card)| (!hidden.contains(&index)).then_some(card))
            .collect()
    }

    pub async fn home(&self) -> Result<Vec<Card>> {
        let html = self.html("/", StdDuration::from_secs(300)).await?;
        let cards = {
            let document = Html::parse_document(&html);
            parse_nimegami_cards(&document, &self.base_url, 30)
        };
        let cards = self.hide_forbidden_cards(cards).await;
        Ok(cards)
    }

    pub async fn popular(&self, limit: usize) -> Result<Vec<Card>> {
        let html = self
            .html("/anime-terbaik/", StdDuration::from_secs(300))
            .await?;
        let cards = {
            let document = Html::parse_document(&html);
            parse_nimegami_cards(&document, &self.base_url, limit)
        };
        Ok(self
            .hide_forbidden_cards(cards)
            .await)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let path = format!("/anime/?q={}", encode(query));
        let html = self.html(&path, StdDuration::from_secs(300)).await?;
        let cards = {
            let document = Html::parse_document(&html);
            parse_nimegami_cards(&document, &self.base_url, limit)
        };
        Ok(self
            .hide_forbidden_cards(cards)
            .await)
    }

    pub async fn browse(&self, params: &BrowseParams) -> Result<BrowsePage> {
        let mut q_parts = Vec::new();
        if !params.q.is_empty() {
            q_parts.push(format!("q={}", encode(&params.q)));
        }
        if params.page > 1 {
            q_parts.push(format!("page={}", params.page));
        }
        if !params.order.is_empty() {
            let sort_val = match params.order.as_str() {
                "update" => "updated",
                "popular" => "popular",
                "rating" => "rating",
                "latest" => "newest",
                "title" => "title",
                _ => &params.order,
            };
            q_parts.push(format!("sort={}", encode(sort_val)));
        }
        for g in &params.genres {
            q_parts.push(format!("genre={}", encode(g)));
        }

        let path = if q_parts.is_empty() {
            "/anime/".to_string()
        } else {
            format!("/anime/?{}", q_parts.join("&"))
        };

        let html = self.html(&path, StdDuration::from_secs(300)).await?;
        let (cards, genres) = {
            let document = Html::parse_document(&html);
            (
                parse_nimegami_cards(&document, &self.base_url, 60),
                parse_nimegami_genres(&document),
            )
        };
        let cards = self.hide_forbidden_cards(cards).await;
        let has_next = cards.len() >= 20;

        Ok(BrowsePage {
            cards,
            genres,
            page: params.page,
            has_next,
        })
    }

    #[allow(dead_code)]
    pub async fn genres(&self) -> Result<Vec<Genre>> {
        let html = self.html("/anime/", StdDuration::from_secs(1800)).await?;
        let document = Html::parse_document(&html);
        Ok(parse_nimegami_genres(&document))
    }

    pub async fn detail(&self, raw_slug: &str) -> Result<AnimeDetail> {
        let clean_slug = raw_slug
            .trim_start_matches("ng-")
            .trim_start_matches("nimegami:")
            .trim_matches('/');

        let path = format!("/{clean_slug}/");
        let html = self.html(&path, StdDuration::from_secs(900)).await?;

        // 1. Ambil data dari JSON-LD
        let (series_opt, json_episodes) = parse_json_ld(&html);
        let series = series_opt.ok_or_else(|| anyhow!("Anime tidak ditemukan di Nimegami"))?;

        let title = series
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if title.is_empty() {
            return Err(anyhow!("Judul anime kosong"));
        }

        // Cek genre 18+
        let raw_genres: Vec<String> = series
            .get("genre")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if raw_genres.iter().any(|g| is_adult_genre(g)) {
            return Err(anyhow!(
                "Anime diblokir karena mengandung genre 18+ / dewasa"
            ));
        }

        let genres: Vec<Genre> = raw_genres
            .into_iter()
            .filter(|g| !is_adult_genre(g))
            .map(|g| {
                let slug = g.to_lowercase().replace(' ', "-");
                Genre {
                    href: format!("/anime?genre={}", encode(&slug)),
                    slug,
                    name: g,
                }
            })
            .collect();

        let synopsis = series
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let image = series
            .get("image")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut studio = String::new();
        if let Some(prod) = series.get("productionCompany") {
            if let Some(name) = prod.get("name").and_then(Value::as_str) {
                studio = name.to_string();
            } else if let Some(arr) = prod.as_array() {
                if let Some(first) = arr.first().and_then(|p| p.get("name")).and_then(Value::as_str) {
                    studio = first.to_string();
                }
            }
        }

        let released = series
            .get("startDate")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let num_episodes = series
            .get("numberOfEpisodes")
            .and_then(Value::as_u64)
            .unwrap_or(json_episodes.len() as u64) as usize;

        let mut facts = Vec::new();
        if !studio.is_empty() {
            facts.push(InfoItem {
                label: "Studio".into(),
                value: studio.clone(),
            });
        }
        if !released.is_empty() {
            facts.push(InfoItem {
                label: "Rilis".into(),
                value: released.clone(),
            });
        }
        facts.push(InfoItem {
            label: "Sumber".into(),
            value: "Nimegami".into(),
        });

        // Bangun daftar episode
        let mut episodes = Vec::new();
        for (i, ep) in json_episodes.iter().enumerate() {
            let ep_num = ep
                .get("episodeNumber")
                .and_then(Value::as_u64)
                .map(|n| n.to_string())
                .unwrap_or_else(|| (i + 1).to_string());

            let ep_title = ep
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            let display_title = if ep_title.is_empty() {
                format!("Episode {ep_num}")
            } else {
                ep_title
            };

            let ep_slug = format!("ng-{clean_slug}-ep-{ep_num}");
            episodes.push(EpisodeLink {
                number: ep_num,
                title: display_title,
                slug: ep_slug.clone(),
                date: String::new(),
                href: format!("/watch/{}", encode(&ep_slug)),
            });
        }

        // Jika JSON-LD tidak memiliki episode individual, buat fallback dari jumlah episode
        if episodes.is_empty() && num_episodes > 0 {
            for ep_i in 1..=num_episodes {
                let ep_slug = format!("ng-{clean_slug}-ep-{ep_i}");
                episodes.push(EpisodeLink {
                    number: ep_i.to_string(),
                    title: format!("Episode {ep_i}"),
                    slug: ep_slug.clone(),
                    date: String::new(),
                    href: format!("/watch/{}", encode(&ep_slug)),
                });
            }
        }

        // Format urutan episode: dari yang terbaru (descending)
        episodes.reverse();

        let episode_count = episodes.len();

        Ok(AnimeDetail {
            slug: format!("ng-{clean_slug}"),
            title,
            alt_title: String::new(),
            image,
            banner_image: String::new(),
            trailer: String::new(),
            rating: String::new(),
            synopsis,
            status: "Completed".into(),
            studio,
            season: String::new(),
            released,
            duration: String::new(),
            kind: "Anime".into(),
            released_on: String::new(),
            updated_on: String::new(),
            facts,
            genres,
            episodes,
            episode_count,
        })
    }

    pub async fn episode(&self, raw_slug: &str) -> Result<EpisodeDetail> {
        let without_prefix = raw_slug
            .trim_start_matches("ng-")
            .trim_start_matches("nimegami:");

        // Format: {anime_slug}-ep-{ep_num}
        let (anime_slug, ep_number) = if let Some((slug, num)) = without_prefix.rsplit_once("-ep-")
        {
            (slug, num)
        } else {
            (without_prefix, "1")
        };

        let ep_num_int: u32 = ep_number.parse().unwrap_or(1);

        let path = format!("/{anime_slug}/");
        let html = self.html(&path, StdDuration::from_secs(900)).await?;

        let (series_opt, json_episodes) = parse_json_ld(&html);
        let series_title = series_opt
            .as_ref()
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(anime_slug)
            .to_string();

        let series_image = series_opt
            .as_ref()
            .and_then(|s| s.get("image"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // Cek genre 18+
        if let Some(genres) = series_opt.as_ref().and_then(|s| s.get("genre")).and_then(Value::as_array) {
            for g in genres {
                if let Some(name) = g.as_str() {
                    if is_adult_genre(name) {
                        return Err(anyhow!("Episode diblokir karena mengandung konten 18+"));
                    }
                }
            }
        }

        // Ekstraksi streaming servers & downloads dari chunk payload
        let (mut servers, downloads) = extract_nimegami_streams(&html, ep_num_int, anime_slug);

        // Jika tidak ada embed server yang ditemukan dari chunk, tambahkan player internal Nimegami
        if servers.is_empty() {
            let primary_url = format!(
                "https://nimegami.id/{anime_slug}/streaming/{ep_number}/720p/?player=worker"
            );
            servers.push(VideoServer {
                name: "Nimegami Player".into(),
                src: primary_url,
            });
        }

        let embed_url = servers
            .first()
            .map(|s| s.src.clone())
            .unwrap_or_default();

        let total_eps = json_episodes.len().max(1);
        let prev_slug = if ep_num_int > 1 {
            format!("ng-{anime_slug}-ep-{}", ep_num_int - 1)
        } else {
            String::new()
        };
        let next_slug = if (ep_num_int as usize) < total_eps {
            format!("ng-{anime_slug}-ep-{}", ep_num_int + 1)
        } else {
            String::new()
        };

        // Buat rekomendasi dari halaman utama Nimegami
        let recommended = self.home().await.unwrap_or_default();

        Ok(EpisodeDetail {
            slug: raw_slug.to_string(),
            title: format!("{series_title} Episode {ep_number}"),
            series_title,
            series_slug: format!("ng-{anime_slug}"),
            series_image,
            episode_number: ep_number.to_string(),
            released: String::new(),
            kind: "Anime".into(),
            embed_url,
            source_url: format!("https://nimegami.id/{anime_slug}/"),
            servers,
            downloads,
            prev_slug,
            next_slug,
            recommended,
        })
    }
}

fn parse_nimegami_cards(document: &Html, base_url: &str, limit: usize) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut seen = HashSet::new();

    for element in document.select(&sel(".anime-card")) {
        let anchor = element
            .select(&sel("a.poster-link"))
            .next()
            .or_else(|| element.select(&sel("a.card-title")).next())
            .or_else(|| element.select(&sel("a")).next());

        let Some(anchor) = anchor else { continue };
        let href = attr(anchor, "href");
        if href.is_empty() || href == "/" || href == "#" {
            continue;
        }

        let slug = href.trim_matches('/').to_string();
        if slug.is_empty() || !seen.insert(slug.clone()) {
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

        // Gambar poster
        let image = element
            .select(&sel("img"))
            .next()
            .map(|img| {
                let srcset = attr(img, "srcSet");
                let src = attr(img, "src");
                extract_best_poster_url(&srcset, &src, base_url)
            })
            .unwrap_or_default();

        let kind_label = first_text_el(element, ".format-badge");
        let score = first_text_el(element, ".score-badge");
        let last_ep = first_text_el(element, ".card-last-episode strong");
        let bottom_text = first_text_el(element, ".poster-bottom span");

        let episode = if !last_ep.is_empty() {
            format!("Ep {last_ep}")
        } else if !bottom_text.is_empty() {
            bottom_text
        } else {
            String::new()
        };

        let card = Card {
            slug: format!("ng-{slug}"),
            title,
            image,
            href: format!("/anime/ng-{}", encode(&slug)),
            kind: "series".into(),
            status: if score.is_empty() { "Anime".into() } else { format!("★ {score}") },
            kind_label: if kind_label.is_empty() { "Anime".into() } else { kind_label },
            episode,
            time: first_text_el(element, ".card-updated"),
            total: String::new(),
            rank: 0,
            genres: Vec::new(),
            provider: "Nimegami".into(),
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

fn parse_nimegami_genres(document: &Html) -> Vec<Genre> {
    let mut genres = Vec::new();
    let mut seen = HashSet::new();

    for item in document.select(&sel("input[name='genre']")) {
        let val = attr(item, "value");
        if val.is_empty() {
            continue;
        }
        let slug = val.to_lowercase().replace(' ', "-");
        if is_adult_genre(&slug) || is_adult_genre(&val) {
            continue;
        }
        if !seen.insert(slug.clone()) {
            continue;
        }
        genres.push(Genre {
            name: val,
            href: format!("/anime?genre={}", encode(&slug)),
            slug,
        });
    }

    genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    genres
}

fn extract_best_poster_url(srcset: &str, src: &str, base_url: &str) -> String {
    // Cari URL poster dari srcset jika mengandung /assets/anime-data/
    if !srcset.is_empty() {
        if let Some(pos) = srcset.find("/assets/anime-data/images/posters/") {
            let rest = &srcset[pos..];
            if let Some(end) = rest.find(['&', ' ', '"', '\'']) {
                let path = &rest[..end];
                return format!("{base_url}{path}");
            }
        }
    }

    if !src.is_empty() {
        if let Some(pos) = src.find("/assets/anime-data/images/posters/") {
            let rest = &src[pos..];
            if let Some(end) = rest.find(['&', ' ', '"', '\'']) {
                let path = &rest[..end];
                return format!("{base_url}{path}");
            }
        }
        if src.starts_with('/') {
            return format!("{base_url}{src}");
        }
        return src.to_string();
    }

    String::new()
}

fn parse_json_ld(html: &str) -> (Option<Value>, Vec<Value>) {
    let mut series_opt = None;
    let mut episodes = Vec::new();

    let sel_script = sel("script[type='application/ld+json']");
    let doc = Html::parse_document(html);

    for element in doc.select(&sel_script) {
        let text = element.text().collect::<String>();
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };

        let items: Vec<Value> = if let Some(arr) = json.get("@graph").and_then(Value::as_array) {
            arr.clone()
        } else if json.is_object() {
            vec![json]
        } else {
            Vec::new()
        };

        for item in items {
            let item_types: Vec<&str> = item
                .get("@type")
                .map(|value| {
                    value
                        .as_str()
                        .map(|kind| vec![kind])
                        .or_else(|| value.as_array().map(|items| {
                            items.iter().filter_map(Value::as_str).collect()
                        }))
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            if item_types.iter().any(|kind| {
                matches!(
                    kind.to_ascii_lowercase().as_str(),
                    "tvseries" | "tvshow" | "movie" | "anime" | "creativework"
                )
            }) {
                if series_opt.is_none() {
                    series_opt = Some(item);
                }
            } else if item_types
                .iter()
                .any(|kind| kind.eq_ignore_ascii_case("TVEpisode"))
            {
                episodes.push(item);
            }
        }
    }

    (series_opt, episodes)
}

fn extract_nimegami_streams(
    html: &str,
    target_ep: u32,
    _anime_slug: &str,
) -> (Vec<VideoServer>, Vec<DownloadLink>) {
    let mut servers = Vec::new();
    let mut downloads = Vec::new();
    let mut seen_server_src = HashSet::new();
    let mut seen_dl_url = HashSet::new();

    // Loop through script tags looking for streaming chunks
    for script_chunk in html.split("self.__next_f.push([1,\"") {
        if !script_chunk.contains("sourceEpisodeId") && !script_chunk.contains("playUrl") {
            continue;
        }

        // Ambil potongan sebelum penutup chunk `\"])`
        let unescaped = script_chunk
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\/", "/");

        let mut pos = 0;
        while let Some(match_idx) = unescaped[pos..].find("{\"id\":\"") {
            let start = pos + match_idx;
            pos = start + 7;

            // Cari batas objek JSON `{ ... }`
            let Some(end_rel) = unescaped[start..].find('}') else {
                continue;
            };
            let end = start + end_rel + 1;
            let obj_str = &unescaped[start..end];

            let Ok(val) = serde_json::from_str::<Value>(obj_str) else {
                continue;
            };

            let ep = val
                .get("episode")
                .and_then(Value::as_u64)
                .map(|e| e as u32)
                .or_else(|| {
                    val.get("sourceEpisodeId")
                        .and_then(Value::as_u64)
                        .map(|e| e as u32)
                });

            if let Some(ep_val) = ep {
                if ep_val != target_ep {
                    continue;
                }
            }

            let play_url = val
                .get("playUrl")
                .and_then(Value::as_str)
                .or_else(|| val.get("url").and_then(Value::as_str))
                .unwrap_or_default()
                .to_string();

            if play_url.is_empty() {
                continue;
            }

            let mode = val
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("embed");

            let label = val
                .get("label")
                .and_then(Value::as_str)
                .or_else(|| val.get("provider").and_then(Value::as_str))
                .unwrap_or("Server")
                .to_string();

            let quality = val
                .get("quality")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            if mode == "embed" {
                if seen_server_src.insert(play_url.clone()) {
                    let name = if quality.is_empty() {
                        format!("Nimegami - {label}")
                    } else {
                        format!("Nimegami - {label} [{quality}]")
                    };
                    servers.push(VideoServer {
                        name,
                        src: play_url,
                    });
                }
            } else if seen_dl_url.insert(play_url.clone()) {
                downloads.push(DownloadLink {
                    server: label,
                    quality,
                    url: play_url,
                });
            }
        }
    }

    (servers, downloads)
}

#[derive(Default)]
struct Cache {
    entries: RwLock<std::collections::HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    body: String,
    expires: Instant,
}

impl Cache {
    fn fresh(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().ok()?;
        let entry = entries.get(key)?;
        if entry.expires > Instant::now() {
            Some(entry.body.clone())
        } else {
            None
        }
    }

    fn stale(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().ok()?;
        entries.get(key).map(|entry| entry.body.clone())
    }

    fn put(&self, key: String, body: String, ttl: StdDuration) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        let now = Instant::now();
        if entries.len() > 600 {
            entries.retain(|_, entry| entry.expires > now);
        }
        if entries.len() > 600 {
            entries.clear();
        }
        entries.insert(
            key,
            CacheEntry {
                body,
                expires: now + ttl,
            },
        );
    }
}

fn sel(query: &str) -> Selector {
    Selector::parse(query).unwrap_or_else(|_| Selector::parse("body").expect("body selector"))
}

fn attr(element: ElementRef<'_>, name: &str) -> String {
    element
        .value()
        .attr(name)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn first_text_el(scope: ElementRef<'_>, query: &str) -> String {
    scope
        .select(&sel(query))
        .next()
        .map(|element| clean(&element.text().collect::<String>()))
        .unwrap_or_default()
}

fn clean(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
