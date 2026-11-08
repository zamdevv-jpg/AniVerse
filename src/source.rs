use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Result, anyhow};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use chrono::{Datelike, Utc, Weekday};
use scraper::{ElementRef, Html, Selector};

use crate::{
    models::{
        AnimeDetail, BrowsePage, BrowseParams, Card, DownloadLink, EpisodeDetail, EpisodeLink,
        Genre, HomeData, InfoItem, ScheduleDay, VideoServer, deduplicate_cards, encode,
        is_adult_card, is_adult_genre, normalize_title_for_dedup,
    },
    doronime::DoronimeSource,
    nimegami::NimegamiSource,
};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const COOKIE: &str = "sg=1; ts_theme=darkmode";

/// Cache HTML sederhana dengan TTL + fallback data lama saat sumber error.
pub struct Source {
    base_url: String,
    client: reqwest::Client,
    cache: Cache,
    pub nimegami: NimegamiSource,
    pub doronime: DoronimeSource,
}

impl Source {
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
                    reqwest::header::COOKIE,
                    COOKIE.parse().expect("valid cookie header"),
                );
                headers.insert(
                    "sec-ch-ua",
                    "\"Chromium\";v=\"131\", \"Not_A Brand\";v=\"24\""
                        .parse()
                        .expect("valid sec-ch-ua"),
                );
                headers.insert("sec-fetch-dest", "document".parse().expect("valid header"));
                headers.insert("sec-fetch-mode", "navigate".parse().expect("valid header"));
                headers.insert("sec-fetch-site", "none".parse().expect("valid header"));
                headers.insert(
                    reqwest::header::REFERER,
                    format!("{base_url}/").parse().expect("valid referer"),
                );
                headers
            })
            .build()?;

        let nimegami_base_url = std::env::var("NIMEGAMI_BASE_URL")
            .unwrap_or_else(|_| "https://nimegami.id".to_string());
        let nimegami = NimegamiSource::new(nimegami_base_url)?;
        let doronime_base_url = std::env::var("DORONIME_BASE_URL")
            .unwrap_or_else(|_| "https://doronime.id".to_string());
        let doronime = DoronimeSource::new(doronime_base_url)?;

        Ok(Self {
            base_url,
            client,
            cache: Cache::default(),
            nimegami,
            doronime,
        })
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

        match self.fetch(path).await {
            Ok(body) => {
                self.cache.put(key, body.clone(), ttl);
                Ok(body)
            }
            Err(error) => {
                if let Some(stale) = self.cache.stale(&key) {
                    tracing::warn!(path, %error, "sumber gagal, memakai cache lama");
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
                        last_error = Some(anyhow!("Otakudesu mengembalikan HTTP {status}"));
                        if matches!(status.as_u16(), 403 | 429 | 500 | 502 | 503 | 504) {
                            continue;
                        }
                        break;
                    }

                    let body = match response.text().await {
                        Ok(body) => body,
                        Err(error) => {
                            last_error = Some(anyhow!("gagal membaca respons: {error}"));
                            continue;
                        }
                    };

                    if is_challenge(&body) {
                        last_error = Some(anyhow!("sumber sedang memverifikasi browser"));
                        continue;
                    }

                    if body.trim().is_empty() {
                        last_error = Some(anyhow!("respons sumber kosong"));
                        continue;
                    }

                    return Ok(body);
                }
                Err(error) => {
                    last_error = Some(anyhow!("tidak bisa menghubungi sumber: {error}"));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("permintaan ke sumber gagal")))
    }

    async fn mal_schedule_html(&self) -> Result<String> {
        const KEY: &str = "https://myanimelist.net/anime/season/schedule";
        const TTL: StdDuration = StdDuration::from_secs(1800);

        if let Some(body) = self.cache.fresh(KEY) {
            return Ok(body);
        }

        let mut last_error = None;
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(StdDuration::from_millis(700)).await;
            }
            match self
                .client
                .get(KEY)
                .header(reqwest::header::REFERER, "https://myanimelist.net/")
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => match response.text().await {
                    Ok(body) if !body.trim().is_empty() && !is_challenge(&body) => {
                        self.cache.put(KEY.to_string(), body.clone(), TTL);
                        return Ok(body);
                    }
                    Ok(_) => last_error = Some(anyhow!("halaman jadwal MyAnimeList kosong atau meminta verifikasi")),
                    Err(error) => last_error = Some(anyhow!("gagal membaca jadwal MyAnimeList: {error}")),
                },
                Ok(response) => {
                    let status = response.status();
                    last_error = Some(anyhow!("MyAnimeList mengembalikan HTTP {status}"));
                    if !matches!(status.as_u16(), 403 | 429 | 500 | 502 | 503 | 504) {
                        break;
                    }
                }
                Err(error) => last_error = Some(anyhow!("tidak bisa menghubungi MyAnimeList: {error}")),
            }
        }

        if let Some(stale) = self.cache.stale(KEY) {
            if let Some(error) = &last_error {
                tracing::warn!(%error, "MyAnimeList gagal; memakai jadwal cache lama");
            } else {
                tracing::warn!("MyAnimeList gagal; memakai jadwal cache lama");
            }
            return Ok(stale);
        }
        Err(last_error.unwrap_or_else(|| anyhow!("permintaan jadwal MyAnimeList gagal")))
    }

    pub async fn home(&self) -> Result<HomeData> {
        let (html_res, nimegami_res, nimegami_popular_res) = tokio::join!(
            self.html("/", StdDuration::from_secs(300)),
            self.nimegami.home(),
            self.nimegami.popular(10)
        );
        let base = self.base_url.clone();

        // Nimegami jadi sumber utama; Otakudesu hanya menambah judul yang belum ada.
        let mut latest = nimegami_res.unwrap_or_default();
        let mut popular = nimegami_popular_res.unwrap_or_default();

        if let Ok(html) = html_res {
            let document = Html::parse_document(&html);
            let latest_scope = document
                .select(&sel(".releases.latesthome"))
                .next()
                .and_then(|heading| heading.parent())
                .and_then(ElementRef::wrap)
                .and_then(|parent| parent.select(&sel(".listupd")).next())
                .or_else(|| document.select(&sel(".postbody .listupd")).next())
                .or_else(|| document.select(&sel(".listupd")).next())
                .unwrap_or_else(|| document.root_element());
            latest.extend(parse_cards(latest_scope, &base, 24));

            let mut seen = HashSet::new();
            for (index, item) in document
                .select(&sel(".serieslist.pop.wpop-weekly ul li"))
                .take(10)
                .enumerate()
            {
                let anchor = item
                    .select(&sel("h4 a"))
                    .next()
                    .or_else(|| item.select(&sel("a.series")).next());
                let Some(anchor) = anchor else { continue };
                let href = attr(anchor, "href");
                let Some(slug) = slug_from_url(&href) else {
                    continue;
                };
                if !seen.insert(slug.clone()) {
                    continue;
                }

                let title = {
                    let title = clean(&anchor.text().collect::<String>());
                    if title.is_empty() {
                        clean(&attr(anchor, "title"))
                    } else {
                        title
                    }
                };
                if title.is_empty() {
                    continue;
                }

                let image = item
                    .select(&sel(".imgseries img"))
                    .next()
                    .map(|img| {
                        resize_image(&normalize_image(&attr(img, "src"), &base), 420, 620)
                    })
                    .unwrap_or_default();

                let genres = item
                    .select(&sel(".leftseries span a"))
                    .map(|a| clean(&a.text().collect::<String>()))
                    .filter(|g| !g.is_empty())
                    .take(4)
                    .collect::<Vec<_>>();

                let card = Card {
                    slug: slug.clone(),
                    title,
                    image,
                    href: format!("/anime/{}", encode(&slug)),
                    kind: "series".into(),
                    kind_label: "Anime".into(),
                    rank: index + 1,
                    genres,
                    provider: "Otakudesu".into(),
                    ..Default::default()
                };

                if !is_adult_card(&card) {
                    popular.push(card);
                }
            }
        }

        latest.retain(|c| !is_adult_card(c));
        let latest = deduplicate_cards(latest);
        popular.retain(|c| !is_adult_card(c));
        let popular = deduplicate_cards(popular);

        Ok(HomeData { latest, popular })
    }

    pub async fn browse(&self, params: &BrowseParams) -> Result<BrowsePage> {
        if params.provider == "nimegami" {
            return self.nimegami.browse(params).await;
        }
        if params.provider == "doronime" {
            return self.doronime.browse(params).await;
        }

        let nimegami_result = if params.provider.is_empty() {
            Some(self.nimegami.browse(params).await)
        } else {
            None
        };

        let html_result = self
            .html(
                &params.source_path(),
                StdDuration::from_secs(if params.q.is_empty() { 600 } else { 300 }),
            )
            .await;
        let base = self.base_url.clone();

        let mut cards = Vec::new();
        let mut genres = Vec::new();
        let mut has_next = false;
        let mut source_succeeded = false;
        let mut last_error = None;

        if let Some(result) = nimegami_result {
            match result {
                Ok(page) => {
                    cards.extend(page.cards);
                    genres = page.genres;
                    has_next = page.has_next;
                    source_succeeded = true;
                }
                Err(error) => last_error = Some(error),
            }
        }

        match html_result {
            Ok(html) => {
                source_succeeded = true;
                let document = Html::parse_document(&html);
                let scope = if params.q.is_empty() {
                    document
                        .select(&sel(".postbody .listupd"))
                        .next()
                        .or_else(|| document.select(&sel(".postbody")).next())
                        .unwrap_or_else(|| document.root_element())
                } else {
                    document
                        .select(&sel(".postbody .listupd"))
                        .next()
                        .or_else(|| document.select(&sel(".search-page .listupd")).next())
                        .or_else(|| document.select(&sel(".postbody")).next())
                        .unwrap_or_else(|| document.root_element())
                };
                let otaku_cards = parse_cards(scope, &base, 60);
                let otaku_has_next = document
                    .select(&sel("link[rel='next']"))
                    .next()
                    .is_some()
                    || otaku_cards.len() >= 30;
                let otaku_genres = parse_genres(&document);

                if genres.is_empty() {
                    genres = otaku_genres;
                }
                has_next |= otaku_has_next;
                cards.extend(otaku_cards);
            }
            Err(error) => {
                if !source_succeeded {
                    last_error = Some(error);
                } else {
                    tracing::warn!(%error, "Otakudesu gagal; menampilkan hasil Nimegami");
                }
            }
        }

        if !source_succeeded {
            return Err(last_error.unwrap_or_else(|| anyhow!("semua sumber anime gagal")));
        }

        cards.retain(|c| !is_adult_card(c));
        let cards = deduplicate_cards(cards);

        Ok(BrowsePage {
            cards,
            genres,
            page: params.page,
            has_next,
        })
    }

    pub async fn genres(&self) -> Result<Vec<Genre>> {
        if let Ok(genres) = self.nimegami.genres().await {
            if !genres.is_empty() {
                return Ok(genres);
            }
        }

        let html = self
            .html("/series/", StdDuration::from_secs(1800))
            .await?;
        let document = Html::parse_document(&html);
        Ok(parse_genres(&document))
    }

    pub async fn detail(&self, slug: &str) -> Result<AnimeDetail> {
        if slug.starts_with("dn-") {
            return self.doronime.detail(slug).await;
        }
        if slug.starts_with("ng-") || slug.starts_with("nimegami:") {
            return self.nimegami.detail(slug).await;
        }

        let path = format!("/series/{}/", encode(slug));
        let html = match self.html(&path, StdDuration::from_secs(900)).await {
            Ok(body) => body,
            Err(_) => {
                // Coba fallback ke Nimegami jika Otakudesu gagal
                return self.nimegami.detail(slug).await;
            }
        };

        let parsed = {
            let document = Html::parse_document(&html);
            let title = first_text_doc(&document, "h1.entry-title");
            if title.is_empty() {
                None
            } else {
                let image = document
                    .select(&sel(".thumbook .thumb img"))
                    .next()
                    .map(|img| normalize_image(&attr(img, "src"), &self.base_url))
                    .filter(|url| !url.is_empty())
                    .or_else(|| {
                        document
                            .select(&sel("meta[property='og:image']"))
                            .next()
                            .map(|meta| attr(meta, "content"))
                    })
                    .unwrap_or_default();

                let alt_title = first_text_doc(&document, ".infox .alter");
                let trailer = document
                    .select(&sel("a.trailerbutton"))
                    .next()
                    .map(|a| attr(a, "href"))
                    .unwrap_or_default();
                let rating = first_text_doc(&document, ".rt .rating strong")
                    .replace("Rating", "")
                    .trim()
                    .to_string();

                let mut facts = Vec::new();
                let mut status = String::new();
                let mut studio = String::new();
                let mut season = String::new();
                let mut released = String::new();
                let mut duration = String::new();
                let mut kind = String::new();
                let mut released_on = String::new();
                let mut updated_on = String::new();

                for span in document.select(&sel(".infox .spe > span")) {
                    let text = clean(&span.text().collect::<String>());
                    let Some((label, value)) = text.split_once(':') else {
                        continue;
                    };
                    let label = clean(label);
                    let value = clean(value);
                    if label.is_empty() || value.is_empty() {
                        continue;
                    }
                    match label.to_lowercase().as_str() {
                        "status" => status = value.clone(),
                        "studio" => studio = value.clone(),
                        "season" => season = value.clone(),
                        "released" => released = value.clone(),
                        "duration" => duration = value.clone(),
                        "type" => kind = value.clone(),
                        "released on" => released_on = value.clone(),
                        "updated on" => updated_on = value.clone(),
                        _ => {}
                    }
                    facts.push(InfoItem { label, value });
                }

                let mut genres = Vec::new();
                let mut seen_genres = HashSet::new();
                let mut is_adult = false;
                for anchor in document.select(&sel(".infox .genxed a")) {
                    let name = clean(&anchor.text().collect::<String>());
                    let Some(genre_slug) = slug_from_url(&attr(anchor, "href")) else {
                        continue;
                    };
                    if is_adult_genre(&name) || is_adult_genre(&genre_slug) {
                        is_adult = true;
                        break;
                    }
                    if name.is_empty() || !seen_genres.insert(genre_slug.clone()) {
                        continue;
                    }
                    genres.push(Genre {
                        name,
                        href: format!("/anime?genre={}", encode(&genre_slug)),
                        slug: genre_slug,
                    });
                }

                let synopsis = {
                    let paragraphs = document
                        .select(&sel(".synp .entry-content p"))
                        .map(|p| clean(&p.text().collect::<String>()))
                        .filter(|p| !p.is_empty())
                        .collect::<Vec<_>>();
                    if paragraphs.is_empty() {
                        first_text_doc(&document, ".synp .entry-content")
                    } else {
                        paragraphs.join("\n\n")
                    }
                };

                let mut episodes = Vec::new();
                let mut seen_episodes = HashSet::new();
                for item in document.select(&sel(".eplister ul li")) {
                    let Some(anchor) = item.select(&sel("a")).next() else {
                        continue;
                    };
                    let Some(episode_slug) = slug_from_url(&attr(anchor, "href")) else {
                        continue;
                    };
                    if !seen_episodes.insert(episode_slug.clone()) {
                        continue;
                    }
                    episodes.push(EpisodeLink {
                        number: first_text_el(item, ".epl-num"),
                        title: first_text_el(item, ".epl-title"),
                        date: first_text_el(item, ".epl-date"),
                        href: format!("/watch/{}", encode(&episode_slug)),
                        slug: episode_slug,
                    });
                }

                let episode_count = episodes.len();

                Some((
                    title,
                    alt_title,
                    image,
                    trailer,
                    rating,
                    synopsis,
                    status,
                    studio,
                    season,
                    released,
                    duration,
                    kind,
                    released_on,
                    updated_on,
                    facts,
                    genres,
                    episodes,
                    episode_count,
                    is_adult,
                ))
            }
        };

        let Some((
            title,
            alt_title,
            image,
            trailer,
            rating,
            synopsis,
            status,
            studio,
            season,
            released,
            duration,
            kind,
            released_on,
            updated_on,
            facts,
            genres,
            episodes,
            episode_count,
            is_adult,
        )) = parsed
        else {
            return self.nimegami.detail(slug).await;
        };

        if is_adult {
            return Err(anyhow!(
                "Anime diblokir karena mengandung konten 18+ / dewasa."
            ));
        }

        // Kartu Otakudesu tetap dapat dipakai untuk judul yang tidak ada di Nimegami.
        // Jika Nimegami punya judul yang sama, jadikan detail dan episode Nimegami sebagai utama.
        let normalized_title = normalize_title_for_dedup(&title);
        for candidate in [&title, &alt_title] {
            if candidate.trim().chars().count() < 2 {
                continue;
            }
            let cards = self.nimegami.search(candidate, 10).await.unwrap_or_default();
            let matched = cards.into_iter().find(|card| {
                normalize_title_for_dedup(&card.title) == normalized_title
            });
            if let Some(card) = matched
                && let Ok(detail) = self.nimegami.detail(&card.slug).await
            {
                return Ok(detail);
            }
        }

        Ok(AnimeDetail {
            slug: slug.to_string(),
            title,
            alt_title,
            image,
            banner_image: String::new(),
            trailer,
            rating,
            synopsis,
            status,
            studio,
            season,
            released,
            duration,
            kind,
            released_on,
            updated_on,
            facts,
            genres,
            episodes,
            episode_count,
        })
    }

    pub async fn episode(&self, slug: &str) -> Result<EpisodeDetail> {
        if slug.starts_with("ng-") || slug.starts_with("nimegami:") {
            return self.nimegami.episode(slug).await;
        }

        let path = format!("/{}/", encode(slug));
        let html = self.html(&path, StdDuration::from_secs(900)).await?;
        let document = Html::parse_document(&html);

        let title = first_text_doc(&document, ".megavid .entry-title");
        let title = if title.is_empty() {
            first_text_doc(&document, "h1.entry-title")
        } else {
            title
        };
        if title.is_empty() {
            return Err(anyhow!("Episode tidak ditemukan di sumber."));
        }

        let series_anchor = document
            .select(&sel(".megavid .year a[href*='/series/']"))
            .next();
        let series_slug = series_anchor
            .and_then(|a| slug_from_url(&attr(a, "href")))
            .unwrap_or_default();
        let series_title = series_anchor
            .map(|a| clean(&a.text().collect::<String>()))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| strip_episode_suffix(&title));

        let series_image = document
            .select(&sel(".megavid .tb img"))
            .next()
            .map(|img| normalize_image(&attr(img, "src"), &self.base_url))
            .unwrap_or_default();

        let embed_url = document
            .select(&sel("#pembed iframe"))
            .next()
            .or_else(|| document.select(&sel(".player-embed iframe")).next())
            .or_else(|| document.select(&sel(".video-content iframe")).next())
            .map(|iframe| attr(iframe, "src"))
            .filter(|src| !src.is_empty())
            .unwrap_or_default();

        let mut servers: Vec<VideoServer> = Vec::new();
        for node in document.select(&sel(".gov-the-embed")) {
            let name = clean(&node.text().collect::<String>());
            let Some(onclick) = node.value().attr("onclick") else {
                continue;
            };
            let Some(payload) = onclick_payload(onclick) else {
                continue;
            };
            let Some(decoded) = decode_base64(payload) else {
                continue;
            };
            let Some(src) = extract_iframe_src(&decoded) else {
                continue;
            };
            let src = absolute(&src, &self.base_url);
            if src.is_empty() || servers.iter().any(|server| server.src == src) {
                continue;
            }
            servers.push(VideoServer {
                name: if name.is_empty() {
                    format!("Server {}", servers.len() + 1)
                } else {
                    name
                },
                src,
            });
        }

        // Tema lama memakai <select class="mirror"> berisi base64.
        for option in document.select(&sel("select.mirror option[value]")) {
            let value = attr(option, "value");
            if value.is_empty() {
                continue;
            }
            let Some(decoded) = decode_base64(&value) else {
                continue;
            };
            let Some(src) = extract_iframe_src(&decoded) else {
                continue;
            };
            let src = absolute(&src, &self.base_url);
            if src.is_empty() || servers.iter().any(|server| server.src == src) {
                continue;
            }
            let name = clean(&option.text().collect::<String>());
            servers.push(VideoServer {
                name: if name.is_empty() {
                    format!("Server {}", servers.len() + 1)
                } else {
                    name
                },
                src,
            });
        }

        if !embed_url.is_empty() && !servers.iter().any(|server| server.src == embed_url) {
            servers.insert(
                0,
                VideoServer {
                    name: "Server Utama".into(),
                    src: embed_url.clone(),
                },
            );
        }

        let mut downloads = Vec::new();
        for item in document.select(&sel(".dlbox ul li")) {
            if item.value().attr("class").is_some_and(|c| c.contains("head")) {
                continue;
            }
            let url = item
                .select(&sel(".e a"))
                .next()
                .map(|a| attr(a, "href"))
                .unwrap_or_default();
            if url.is_empty() {
                continue;
            }
            let server = first_text_el(item, ".q");
            let quality = first_text_el(item, ".w");
            downloads.push(DownloadLink {
                server: if server.is_empty() {
                    "Server".into()
                } else {
                    server
                },
                quality,
                url,
            });
        }

        let prev_slug = document
            .select(&sel(".naveps a[rel='prev']"))
            .next()
            .and_then(|a| slug_from_url(&attr(a, "href")))
            .unwrap_or_default();
        let next_slug = document
            .select(&sel(".naveps a[rel='next']"))
            .next()
            .and_then(|a| slug_from_url(&attr(a, "href")))
            .unwrap_or_default();

        let episode_number = {
            let from_meta = document
                .select(&sel(".megavid meta[itemprop='episodeNumber']"))
                .next()
                .map(|meta| attr(meta, "content"))
                .unwrap_or_default();
            if !from_meta.is_empty() {
                from_meta
            } else {
                number_from_title(&title)
            }
        };

        let mut recommended = Vec::new();
        for bixbox in document.select(&sel(".bixbox")) {
            let is_recommended = bixbox.select(&sel("h3")).any(|heading| {
                clean(&heading.text().collect::<String>())
                    .to_lowercase()
                    .contains("recommended")
            });
            if is_recommended {
                recommended = parse_cards(bixbox, &self.base_url, 12);
                break;
            }
        }

        recommended.retain(|c| !is_adult_card(c));
        let recommended = deduplicate_cards(recommended);

        Ok(EpisodeDetail {
            slug: slug.to_string(),
            title,
            series_title,
            series_slug,
            series_image,
            episode_number,
            released: first_text_doc(&document, ".megavid .year .updated"),
            kind: first_text_doc(&document, ".megavid .epx"),
            embed_url,
            source_url: self.url(&path),
            servers,
            downloads,
            prev_slug,
            next_slug,
            recommended,
        })
    }

    pub async fn schedule(&self) -> Result<Vec<ScheduleDay>> {
        let html = self.mal_schedule_html().await?;
        let document = Html::parse_document(&html);
        Ok(parse_mal_schedule(&document))
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Ok(Vec::new());
        }

        let nime_cards = self
            .nimegami
            .search(query, limit)
            .await
            .unwrap_or_default();

        let otaku_cards = {
            let path = format!("/?s={}", encode(query));
            if let Ok(html) = self.html(&path, StdDuration::from_secs(300)).await {
                let document = Html::parse_document(&html);
                let scope = document
                    .select(&sel(".postbody .listupd"))
                    .next()
                    .or_else(|| document.select(&sel(".postbody")).next())
                    .unwrap_or_else(|| document.root_element());
                parse_cards(scope, &self.base_url, limit)
            } else {
                Vec::new()
            }
        };

        let mut all_cards = nime_cards;
        all_cards.extend(otaku_cards);

        all_cards.retain(|c| !is_adult_card(c));
        let deduplicated = deduplicate_cards(all_cards);

        Ok(deduplicated.into_iter().take(limit).collect())
    }
}

#[derive(Default)]
struct Cache {
    entries: RwLock<HashMap<String, CacheEntry>>,
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

fn is_challenge(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    body.len() < 500
        || lower.contains("checking your browser")
        || lower.contains("just a moment")
        || lower.contains("cf-challenge")
        || lower.contains("enable javascript and cookies to continue")
}

fn parse_cards(scope: ElementRef<'_>, base: &str, limit: usize) -> Vec<Card> {
    let mut cards = Vec::new();
    let mut seen = HashSet::new();

    for element in scope.select(&sel(".bsx")) {
        let Some(card) = parse_card(element, base) else {
            continue;
        };
        if is_adult_card(&card) {
            continue;
        }
        if !seen.insert(card.slug.clone()) {
            continue;
        }
        cards.push(card);
        if cards.len() >= limit {
            break;
        }
    }

    cards
}

fn parse_card(element: ElementRef<'_>, base: &str) -> Option<Card> {
    let anchor = element
        .select(&sel("a.tip"))
        .next()
        .or_else(|| element.select(&sel("a[itemprop='url']")).next())
        .or_else(|| element.select(&sel("a")).next())?;

    let raw_href = attr(anchor, "href");
    if raw_href.is_empty() {
        return None;
    }
    let slug = slug_from_url(&raw_href)?;
    let is_series = raw_href.contains("/series/");

    let raw_title = {
        let title = clean(&attr(anchor, "title"));
        if !title.is_empty() {
            title
        } else {
            let headline = first_text_el(element, "h2[itemprop='headline']");
            if !headline.is_empty() {
                headline
            } else {
                first_text_el(element, ".tt")
            }
        }
    };
    if raw_title.is_empty() {
        return None;
    }

    let title = if is_series {
        raw_title.clone()
    } else {
        strip_episode_suffix(&raw_title)
    };

    let image = element
        .select(&sel("img"))
        .next()
        .map(|img| {
            let src = {
                let src = attr(img, "src");
                if src.is_empty() {
                    attr(img, "data-src")
                } else {
                    src
                }
            };
            resize_image(&normalize_image(&src, base), 420, 620)
        })
        .unwrap_or_default();

    let kind_label = first_text_el(element, ".typez");
    let epx = first_text_el(element, ".bt .epx");
    let mut status = first_text_el(element, ".status");
    let mut episode = String::new();

    let epx_lower = epx.to_lowercase();
    if epx_lower.starts_with("ep") {
        episode = epx.clone();
    } else if !epx.is_empty() && epx.parse::<f32>().is_ok() {
        episode = format!("Ep {epx}");
    } else if matches!(epx_lower.as_str(), "ongoing" | "completed" | "upcoming") {
        status = epx;
    }

    if episode.is_empty() && !is_series {
        episode = episode_from_title(&raw_title);
    }

    Some(Card {
        slug: slug.clone(),
        title: title.clone(),
        image,
        href: if is_series {
            format!("/anime/{}", encode(&slug))
        } else {
            format!("/watch/{}", encode(&slug))
        },
        kind: if is_series { "series" } else { "episode" }.into(),
        status,
        kind_label,
        episode,
        time: first_text_el(element, ".timeago"),
        provider: "Otakudesu".into(),
        ..Default::default()
    })
}

fn parse_genres(document: &Html) -> Vec<Genre> {
    let mut genres = Vec::new();
    let mut seen = HashSet::new();

    for item in document.select(&sel("form.filters .dropdown-menu li")) {
        let Some(input) = item.select(&sel("input[name='genre[]']")).next() else {
            continue;
        };
        let slug = attr(input, "value");
        if slug.is_empty() || is_adult_genre(&slug) || !seen.insert(slug.clone()) {
            continue;
        }
        let name = first_text_el(item, "label");
        let name = if name.is_empty() {
            title_case_slug(&slug)
        } else {
            name
        };
        if is_adult_genre(&name) {
            continue;
        }
        genres.push(Genre {
            name,
            slug: slug.clone(),
            href: format!("/anime?genre={}", encode(&slug)),
        });
    }

    genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    genres
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

fn first_text_doc(document: &Html, query: &str) -> String {
    document
        .select(&sel(query))
        .next()
        .map(|element| clean(&element.text().collect::<String>()))
        .unwrap_or_default()
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

fn strip_episode_suffix(title: &str) -> String {
    // Pencarian dilakukan pada versi lowercase, lalu indeksnya dipetakan kembali
    // ke batas karakter yang aman pada string asli.
    let lower = title.to_lowercase();
    let mut cut = title.len();

    for needle in [" episode ", " subtitle indonesia", " sub indo"] {
        if let Some(index) = lower.find(needle) {
            cut = cut.min(index);
        }
    }

    while cut > 0 && !title.is_char_boundary(cut) {
        cut -= 1;
    }

    clean(&title[..cut])
}

fn episode_from_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let Some(index) = lower.find("episode ") else {
        return String::new();
    };
    let rest = &title[index + "episode ".len()..];
    let token = rest
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
    if token.is_empty() {
        String::new()
    } else {
        format!("Ep {token}")
    }
}

fn number_from_title(title: &str) -> String {
    episode_from_title(title)
        .trim_start_matches("Ep ")
        .to_string()
}

fn slug_from_url(href: &str) -> Option<String> {
    let without_query = href.split(['?', '#']).next().unwrap_or(href);
    let segment = without_query
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()?;
    let decoded = urlencoding::decode(segment).ok()?.into_owned();
    let decoded = decoded.trim().to_string();
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

fn absolute(href: &str, base: &str) -> String {
    let href = href.trim();
    if href.is_empty() {
        return String::new();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with('/') {
        return format!("{base}{href}");
    }
    format!("{base}/{href}")
}

fn normalize_image(src: &str, base: &str) -> String {
    let src = src.trim();
    if src.is_empty() || src.starts_with("data:") {
        return String::new();
    }
    absolute(src, base)
}

pub fn resize_image(url: &str, _width: u32, _height: u32) -> String {
    if url.is_empty() {
        return String::new();
    }
    // Never ask an image CDN for a fixed display size: some source posters are
    // smaller than that size and the CDN will upscale them, making them look HD
    // while preserving no extra detail. Keep the provider's original URL.
    url.to_string()
}

fn title_case_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn onclick_payload(onclick: &str) -> Option<&str> {
    let start = onclick.find("putMi(")?;
    let rest = &onclick[start + "putMi(".len()..];
    let quote_index = rest.find(['\'', '"'])?;
    let quote = rest.as_bytes()[quote_index] as char;
    let after = &rest[quote_index + 1..];
    let end = after.find(quote)?;
    Some(&after[..end])
}

#[cfg(test)]
mod artwork_tests {
    use super::resize_image;

    #[test]
    fn keeps_original_artwork_url_without_upscaling_parameters() {
        let source = "https://example.test/poster.jpg?resize=180,260&quality=80";
        assert_eq!(resize_image(source, 600, 850), source);
    }
}

fn decode_base64(value: &str) -> Option<String> {
    STANDARD
        .decode(value)
        .or_else(|_| STANDARD_NO_PAD.decode(value))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn extract_iframe_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let iframe = lower.find("<iframe")?;
    let rest = &html[iframe..];
    let src_lower = rest.to_ascii_lowercase();
    let src_index = src_lower.find("src=")?;
    let after = &rest[src_index + 4..];
    let first = after.chars().next()?;

    let src = if first == '"' || first == '\'' {
        let remainder = &after[1..];
        let end = remainder.find(first)?;
        &remainder[..end]
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after.len());
        &after[..end]
    };

    let src = src.replace("&amp;", "&").trim().to_string();
    if src.is_empty() { None } else { Some(src) }
}

fn current_weekday_en() -> &'static str {
    let wib = Utc::now() + chrono::Duration::hours(7);
    match wib.weekday() {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

fn parse_mal_schedule(document: &Html) -> Vec<ScheduleDay> {
    // MAL marks the two explicit genre filters on its schedule page with these IDs:
    // 12 = Hentai and 49 = Erotica. Missing genre metadata is excluded as a safe default.
    const BLOCKED_GENRE_IDS: [&str; 2] = ["12", "49"];
    let today = current_weekday_en();
    let mut days = Vec::new();

    for section in document.select(&sel(".js-seasonal-anime-list")) {
        let day = first_text_el(section, ".anime-header");
        let (day_id, day_name) = indonesian_day(&day);
        if !matches!(day_id.as_str(), "senin" | "selasa" | "rabu" | "kamis" | "jumat" | "sabtu" | "minggu") {
            continue;
        }

        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        for entry in section.select(&sel(".js-seasonal-anime")) {
            let genre_ids = attr(entry, "data-genre");
            let ids = genre_ids.split(',').map(str::trim).collect::<Vec<_>>();
            let classes = attr(entry, "class").to_lowercase();
            if ids.is_empty()
                || genre_ids.trim().is_empty()
                || ids.iter().any(|id| BLOCKED_GENRE_IDS.contains(id))
                || classes.split_whitespace().any(|class| class == "r18")
            {
                continue;
            }

            let anchor = entry
                .select(&sel(".h2_anime_title a.link-title"))
                .next()
                .or_else(|| entry.select(&sel("a.link-title")).next());
            let Some(anchor) = anchor else { continue };
            let href = attr(anchor, "href");
            let Some(mal_id) = href
                .split("/anime/")
                .nth(1)
                .and_then(|tail| tail.split('/').next())
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };
            let title = clean(&anchor.text().collect::<String>());
            if title.is_empty() {
                continue;
            }

            let image = entry
                .select(&sel("img"))
                .next()
                .map(|image| {
                    let lazy = attr(image, "data-src");
                    if lazy.is_empty() {
                        attr(image, "src")
                    } else {
                        lazy
                    }
                })
                .unwrap_or_default();
            let total = entry
                .select(&sel(".info .item"))
                .nth(1)
                .and_then(|item| item.select(&sel("span")).next())
                .map(|span| clean(&span.text().collect::<String>()))
                .unwrap_or_default()
                .trim_end_matches(" eps")
                .to_string();

            let card = Card {
                slug: format!("mal-{mal_id}"),
                title,
                image,
                href: format!("/jadwal/anime?title={}", encode(&clean(&anchor.text().collect::<String>()))),
                kind: "series".into(),
                kind_label: "Anime".into(),
                time: "MAL".into(),
                total,
                provider: "MyAnimeList".into(),
                ..Default::default()
            };
            if !is_adult_card(&card) && seen.insert(mal_id) {
                entries.push(card);
            }
        }

        days.push(ScheduleDay {
            day: day_name,
            id: day_id,
            is_today: day.eq_ignore_ascii_case(today),
            entries,
        });
    }

    let order = ["senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "minggu"];
    days.sort_by_key(|day| order.iter().position(|id| *id == day.id).unwrap_or(99));
    days
}

fn indonesian_day(day: &str) -> (String, String) {
    let lower = day.trim().to_lowercase();
    let name = match lower.as_str() {
        "monday" => "Senin",
        "tuesday" => "Selasa",
        "wednesday" => "Rabu",
        "thursday" => "Kamis",
        "friday" => "Jumat",
        "saturday" => "Sabtu",
        "sunday" => "Minggu",
        other => {
            if other.is_empty() {
                "Hari"
            } else {
                day
            }
        }
    };
    let id = match lower.as_str() {
        "monday" => "senin",
        "tuesday" => "selasa",
        "wednesday" => "rabu",
        "thursday" => "kamis",
        "friday" => "jumat",
        "saturday" => "sabtu",
        "sunday" => "minggu",
        _ => "hari",
    };
    (id.to_string(), name.to_string())
}
