use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration as StdDuration, Instant},
};

use anyhow::{Result, anyhow};
use scraper::{ElementRef, Html, Selector};
use tokio::task::JoinSet;

use crate::models::{
    AnimeDetail, BrowsePage, BrowseParams, Card, EpisodeLink, Genre, InfoItem, encode,
    is_adult_card, is_adult_genre,
};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct DoronimeSource {
    base_url: String,
    client: reqwest::Client,
    cache: Arc<Cache>,
}

impl DoronimeSource {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(StdDuration::from_secs(20))
            .connect_timeout(StdDuration::from_secs(8))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;

        Ok(Self {
            base_url,
            client,
            cache: Arc::new(Cache::default()),
        })
    }

    async fn html(&self, path: &str, ttl: StdDuration) -> Result<String> {
        if let Some(body) = self.cache.fresh(path) {
            return Ok(body);
        }

        match self.fetch(path).await {
            Ok(body) => {
                self.cache.put(path.to_string(), body.clone(), ttl);
                Ok(body)
            }
            Err(error) => {
                if let Some(stale) = self.cache.stale(path) {
                    tracing::warn!(path, %error, "Doronime gagal; memakai cache lama");
                    Ok(stale)
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn fetch(&self, path: &str) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);
        let mut last_error = None;

        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(StdDuration::from_millis(650)).await;
            }

            match self
                .client
                .get(&url)
                .header(reqwest::header::REFERER, format!("{}/", self.base_url))
                .header(
                    reqwest::header::ACCEPT,
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    let body = response.text().await?;
                    if body.trim().is_empty() || is_challenge(&body) {
                        last_error = Some(anyhow!("respons Doronime kosong atau meminta verifikasi"));
                        continue;
                    }
                    return Ok(body);
                }
                Ok(response) => {
                    let status = response.status();
                    last_error = Some(anyhow!("Doronime mengembalikan HTTP {status}"));
                    if !matches!(status.as_u16(), 403 | 429 | 500 | 502 | 503 | 504) {
                        break;
                    }
                }
                Err(error) => last_error = Some(anyhow!("tidak bisa menghubungi Doronime: {error}")),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("permintaan ke Doronime gagal")))
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Card>> {
        let query = query.trim();
        if query.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let path = format!("/search?s={}", encode(query));
        self.cards_from_path(&path, limit.min(20), 300).await
    }

    pub async fn browse(&self, params: &BrowseParams) -> Result<BrowsePage> {
        let path = if !params.q.is_empty() {
            format!("/search?s={}", encode(&params.q))
        } else if let Some(genre) = params.genres.first() {
            format!("/genre/{}", encode(genre))
        } else if params.page > 1 {
            format!("/anime?page={}", params.page)
        } else {
            "/anime".to_string()
        };

        let html = self
            .html(
                &path,
                if params.q.is_empty() {
                    StdDuration::from_secs(600)
                } else {
                    StdDuration::from_secs(300)
                },
            )
            .await?;
        let (cards, genres, page_has_next) = {
            let document = Html::parse_document(&html);
            let cards = self.parse_cards(&document, 30);
            let genres = self.parse_genres(&document);
            let page_has_next = document
                .select(&sel("a[rel='next'], a[aria-label*='Next']"))
                .next()
                .is_some();
            (cards, genres, page_has_next)
        };
        let cards = self.verify_safe_cards(cards).await;
        let has_next = page_has_next || cards.len() >= 20;

        Ok(BrowsePage {
            cards,
            genres,
            page: params.page,
            has_next,
        })
    }

    async fn cards_from_path(&self, path: &str, limit: usize, ttl_secs: u64) -> Result<Vec<Card>> {
        let html = self.html(path, StdDuration::from_secs(ttl_secs)).await?;
        let cards = {
            let document = Html::parse_document(&html);
            self.parse_cards(&document, limit)
        };
        Ok(self.verify_safe_cards(cards).await)
    }

    fn parse_cards(&self, document: &Html, limit: usize) -> Vec<Card> {
        let mut cards = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for anchor in document.select(&sel("a.Card.Card--column")) {
            let href = attr(anchor, "href");
            let Some(slug) = doronime_slug(&href, &self.base_url) else {
                continue;
            };
            if !seen.insert(slug.clone()) {
                continue;
            }

            let title = {
                let title = attr(anchor, "title");
                if title.is_empty() {
                    first_text_el(anchor, ".Card__caption small")
                } else {
                    title
                }
            };
            if title.is_empty() {
                continue;
            }

            let image = anchor
                .select(&sel("img[itemprop='image']"))
                .next()
                .map(|image| {
                    let src = attr(image, "src");
                    if src.is_empty() {
                        attr(image, "data-src")
                    } else {
                        src
                    }
                })
                .unwrap_or_default();
            let status = first_text_el(anchor, ".Card__badge .Badge--success");
            let kind_label = first_text_el(anchor, ".Card__badge--bottom");

            let card = Card {
                slug: format!("dn-{slug}"),
                title,
                image,
                href: format!("/anime/dn-{}", encode(&slug)),
                kind: "series".into(),
                status,
                kind_label,
                provider: "Doronime".into(),
                ..Default::default()
            };
            if !is_adult_card(&card) {
                cards.push(card);
            }
            if cards.len() >= limit {
                break;
            }
        }

        cards
    }

    pub async fn detail(&self, raw_slug: &str) -> Result<AnimeDetail> {
        let slug = raw_slug
            .strip_prefix("dn-")
            .unwrap_or(raw_slug)
            .trim_matches('/');
        let Some(slug) = doronime_slug(slug, &self.base_url).or_else(|| {
            (!slug.is_empty() && !slug.contains('/')).then(|| slug.to_string())
        }) else {
            return Err(anyhow!("slug Doronime tidak valid"));
        };
        let path = format!("/anime/{slug}");
        let html = self.html(&path, StdDuration::from_secs(900)).await?;
        let document = Html::parse_document(&html);

        let mut title = first_text_doc(&document, ".Content__header .Content__title");
        if title.is_empty() {
            title = first_text_doc(&document, "h1.Content__title");
        }
        if title.is_empty() {
            title = first_text_doc(&document, ".Content__header h1");
        }
        if title.is_empty() {
            return Err(anyhow!("anime tidak ditemukan di Doronime"));
        }

        let mut facts = Vec::new();
        let mut genres = Vec::new();
        let mut status = String::new();
        let mut studio = String::new();
        let mut season = String::new();
        let mut released = String::new();
        let mut duration = String::new();
        let mut rating = String::new();
        let mut alt_title = String::new();

        for item in document.select(&sel(".Content__header-caption-item")) {
            let label = first_text_el(item, "b")
                .trim()
                .trim_end_matches(':')
                .to_lowercase();
            if label.is_empty() {
                continue;
            }
            let value = first_text_el(item, "span");
            match label.as_str() {
                "romaji" | "english" if alt_title.is_empty() => alt_title = value.clone(),
                "genre" => {
                    for anchor in item.select(&sel("a[href*='/genre/']")) {
                        let name = clean(&anchor.text().collect::<String>());
                        let href = attr(anchor, "href");
                        let genre_slug = href
                            .split("/genre/")
                            .nth(1)
                            .unwrap_or_default()
                            .trim_matches('/')
                            .to_string();
                        if name.is_empty() || genre_slug.is_empty() {
                            continue;
                        }
                        if is_adult_genre(&name) || is_adult_genre(&genre_slug) {
                            return Err(anyhow!(
                                "Anime diblokir karena genre dewasa / 18+ (Doronime)."
                            ));
                        }
                        genres.push(Genre {
                            href: format!("/anime?genre={}", encode(&genre_slug)),
                            slug: genre_slug,
                            name,
                        });
                    }
                }
                "status" => status = value.clone(),
                "studio" => studio = value.clone(),
                "season" => season = value.clone(),
                "tanggal tayang" => released = value.clone(),
                "durasi" => duration = value.clone(),
                "skor mal" => rating = value.clone(),
                _ => {}
            }
            if label != "genre" && !value.is_empty() {
                facts.push(InfoItem {
                    label: label.clone(),
                    value,
                });
            }
        }

        // Bila detail tidak memberi metadata genre, kartu tidak aman untuk ditampilkan.
        if genres.is_empty() {
            return Err(anyhow!("genre Doronime tidak tersedia untuk diverifikasi"));
        }
        let card = Card {
            slug: format!("dn-{slug}"),
            title: title.clone(),
            genres: genres.iter().map(|genre| genre.name.clone()).collect(),
            ..Default::default()
        };
        if is_adult_card(&card) {
            return Err(anyhow!("Anime diblokir karena konten dewasa / 18+ (Doronime)."));
        }

        let image = document
            .select(&sel(".Content__header img[itemprop='image']"))
            .next()
            .map(|image| attr(image, "src"))
            .filter(|image| !image.is_empty())
            .or_else(|| {
                document
                    .select(&sel("meta[property='og:image']"))
                    .next()
                    .map(|meta| attr(meta, "content"))
            })
            .unwrap_or_default();
        let synopsis = document
            .select(&sel("meta[name='description']"))
            .next()
            .map(|meta| attr(meta, "content"))
            .unwrap_or_default();

        let episode_anchor_selector = sel("a[href*='/anime/']");
        let episodes = document
            .select(&sel(".Content__table-body"))
            .filter_map(|row| {
                let mut anchors = row.select(&episode_anchor_selector);
                let anchor = anchors.next()?;
                let url = attr(anchor, "href");
                let number = clean(&anchor.text().collect::<String>());
                let episode_title = anchors
                    .next()
                    .map(|anchor| clean(&anchor.text().collect::<String>()))
                    .unwrap_or_else(|| number.clone());
                if url.contains("/batch")
                    || (!url.contains("/episode") && !number.to_lowercase().contains("episode"))
                {
                    return None;
                }
                let episode_slug = url
                    .split("/anime/")
                    .nth(1)
                    .unwrap_or_default()
                    .trim_matches('/')
                    .replace('/', "-");
                Some(EpisodeLink {
                    number,
                    title: episode_title,
                    slug: format!("dn-{episode_slug}"),
                    date: row
                        .select(&sel(".d-none.d-md-block.col"))
                        .next()
                        .map(|date| clean(&date.text().collect::<String>()))
                        .unwrap_or_default(),
                    href: url,
                })
            })
            .collect::<Vec<_>>();

        Ok(AnimeDetail {
            slug: format!("dn-{slug}"),
            title,
            alt_title,
            image,
            synopsis,
            status,
            studio,
            season,
            released,
            duration,
            rating,
            facts,
            genres,
            episode_count: episodes.len(),
            episodes,
            ..Default::default()
        })
    }

    fn parse_genres(&self, document: &Html) -> Vec<Genre> {
        let mut genres = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for anchor in document.select(&sel(".Navigation__footer a.item[href*='/genre/']")) {
            let name = clean(&anchor.text().collect::<String>());
            let href = attr(anchor, "href");
            let slug = href
                .split("/genre/")
                .nth(1)
                .unwrap_or_default()
                .trim_matches('/')
                .to_string();
            if name.is_empty()
                || slug.is_empty()
                || is_adult_genre(&name)
                || is_adult_genre(&slug)
                || !seen.insert(slug.clone())
            {
                continue;
            }
            genres.push(Genre {
                name,
                slug: slug.clone(),
                href: format!("/anime?provider=doronime&genre={}", encode(&slug)),
            });
        }
        genres
    }

    async fn verify_safe_cards(&self, cards: Vec<Card>) -> Vec<Card> {
        const CONCURRENCY: usize = 6;
        let mut safe = vec![false; cards.len()];

        for (batch_index, batch) in cards.chunks(CONCURRENCY).enumerate() {
            let start = batch_index * CONCURRENCY;
            let mut checks = JoinSet::new();
            for (offset, card) in batch.iter().enumerate() {
                let source = self.clone();
                let slug = card.slug.clone();
                checks.spawn(async move { (start + offset, source.detail(&slug).await.is_ok()) });
            }
            while let Some(Ok((index, allowed))) = checks.join_next().await {
                safe[index] = allowed;
            }
        }

        cards
            .into_iter()
            .enumerate()
            .filter_map(|(index, card)| safe[index].then_some(card))
            .collect()
    }
}

fn doronime_slug(href: &str, base_url: &str) -> Option<String> {
    let path = href
        .strip_prefix(base_url)
        .or_else(|| href.split_once("/anime/").map(|(_, tail)| tail))?;
    let slug = path
        .trim_start_matches("/anime/")
        .trim_matches('/')
        .split(['?', '#', '/'])
        .next()?
        .trim();
    (!slug.is_empty() && slug != "anime").then(|| slug.to_string())
}

fn first_text_doc(document: &Html, selector: &str) -> String {
    document
        .select(&sel(selector))
        .next()
        .map(|element| clean(&element.text().collect::<String>()))
        .unwrap_or_default()
}

fn first_text_el(element: ElementRef<'_>, selector: &str) -> String {
    element
        .select(&sel(selector))
        .next()
        .map(|child| clean(&child.text().collect::<String>()))
        .unwrap_or_default()
}

fn attr(element: ElementRef<'_>, name: &str) -> String {
    element
        .value()
        .attr(name)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn clean(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sel(value: &str) -> Selector {
    Selector::parse(value).expect("valid Doronime selector")
}

fn is_challenge(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("just a moment") || lower.contains("cf-challenge") || lower.contains("checking your browser")
}

#[derive(Default)]
struct Cache {
    entries: RwLock<HashMap<String, CacheEntry>>,
}

#[derive(Clone)]
struct CacheEntry {
    body: String,
    expires: Instant,
}

impl Cache {
    fn fresh(&self, key: &str) -> Option<String> {
        let entry = self.entries.read().ok()?.get(key)?.clone();
        (entry.expires > Instant::now()).then_some(entry.body)
    }

    fn stale(&self, key: &str) -> Option<String> {
        self.entries.read().ok()?.get(key).map(|entry| entry.body.clone())
    }

    fn put(&self, key: String, body: String, ttl: StdDuration) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        if entries.len() > 500 {
            entries.retain(|_, entry| entry.expires > Instant::now());
        }
        if entries.len() > 600 {
            entries.clear();
        }
        entries.insert(
            key,
            CacheEntry {
                body,
                expires: Instant::now() + ttl,
            },
        );
    }
}
