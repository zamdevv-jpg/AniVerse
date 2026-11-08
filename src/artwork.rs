use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeArtwork {
    pub title: String,
    pub poster: String,
    pub banner: String,
    pub poster_source: String,
    pub banner_source: String,
    pub width: u32,
    pub height: u32,
    pub score: u32,
}

impl Default for AnimeArtwork {
    fn default() -> Self {
        Self {
            title: String::new(),
            poster: String::new(),
            banner: String::new(),
            poster_source: String::new(),
            banner_source: String::new(),
            width: 0,
            height: 0,
            score: 0,
        }
    }
}

pub struct ArtworkResolver {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, AnimeArtwork>>>,
    cache_path: Option<PathBuf>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AniListResponse {
    data: Option<AniListData>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AniListData {
    #[serde(rename = "Media")]
    media: Option<AniListMedia>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AniListMedia {
    id: Option<u64>,
    title: Option<AniListTitle>,
    #[serde(rename = "coverImage")]
    cover_image: Option<AniListCoverImage>,
    #[serde(rename = "bannerImage")]
    banner_image: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AniListTitle {
    romaji: Option<String>,
    english: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AniListCoverImage {
    #[serde(rename = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct KitsuResponse {
    data: Option<Vec<KitsuItem>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct KitsuItem {
    attributes: Option<KitsuAttributes>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct KitsuAttributes {
    #[serde(rename = "posterImage")]
    poster_image: Option<KitsuImageUrls>,
    #[serde(rename = "coverImage")]
    cover_image: Option<KitsuImageUrls>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct KitsuImageUrls {
    original: Option<String>,
    large: Option<String>,
    medium: Option<String>,
}

impl ArtworkResolver {
    pub fn new(cache_dir: Option<PathBuf>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let cache_path = cache_dir.map(|dir| dir.join("artworks.json"));
        let initial_cache = if let Some(ref path) = cache_path {
            Self::load_cache_from_disk(path).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Self {
            client,
            cache: Arc::new(RwLock::new(initial_cache)),
            cache_path,
        }
    }

    fn load_cache_from_disk(path: &Path) -> Option<HashMap<String, AnimeArtwork>> {
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_cache_to_disk(path: &Path, cache: &HashMap<String, AnimeArtwork>) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(cache) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Cleans noisy title markers like "Episode 1", "Sub Indo", "Batch", "(Movie)", etc.
    pub fn clean_title(raw: &str) -> String {
        let mut clean = raw.to_string();

        // Remove bracketed content like [Batch], (Sub Indo), (BD), (Movie)
        if let Ok(re_bracket) = Regex::new(r"\[.*?\]|\(.*?\)") {
            clean = re_bracket.replace_all(&clean, " ").to_string();
        }

        // Remove episode references: "Episode 12", "Eps 04", "Ep. 01"
        if let Ok(re_ep) = Regex::new(r"(?i)\b(episode|eps|ep)\.?\s*\d+.*") {
            clean = re_ep.replace_all(&clean, "").to_string();
        }

        // Remove common Indonesian release words
        if let Ok(re_words) = Regex::new(r"(?i)\b(sub\s*indo|subtitle\s*indonesia|batch|tamat|lengkap|bd|uncensored|end|recap)\b") {
            clean = re_words.replace_all(&clean, "").to_string();
        }

        // Remove season indicators at the end if desired or clean dashes
        clean = clean.replace('_', " ").replace('-', " ");

        // Collapse whitespace
        let words: Vec<&str> = clean.split_whitespace().collect();
        words.join(" ")
    }

    pub fn normalize_key(raw: &str) -> String {
        Self::clean_title(raw).to_lowercase()
    }

    /// Queries AniList GraphQL API for coverImage.extraLarge and bannerImage.
    async fn query_anilist(&self, search_title: &str) -> Option<(String, String)> {
        let query = r#"
            query ($search: String) {
                Media (search: $search, type: ANIME) {
                    id
                    title {
                        romaji
                        english
                    }
                    coverImage {
                        extraLarge
                        large
                    }
                    bannerImage
                }
            }
        "#;

        let payload = serde_json::json!({
            "query": query,
            "variables": {
                "search": search_title
            }
        });

        let res = self.client
            .post("https://graphql.anilist.co")
            .json(&payload)
            .send()
            .await
            .ok()?;

        if !res.status().is_success() {
            return None;
        }

        let parsed: AniListResponse = res.json().await.ok()?;
        let media = parsed.data?.media?;

        let poster = media
            .cover_image
            .as_ref()
            .and_then(|c| c.extra_large.clone().or_else(|| c.large.clone()))
            .unwrap_or_default();

        let banner = media.banner_image.unwrap_or_default();

        if poster.is_empty() && banner.is_empty() {
            None
        } else {
            Some((poster, banner))
        }
    }

    /// Queries Kitsu API for posterImage.original and coverImage.original.
    async fn query_kitsu(&self, search_title: &str) -> Option<(String, String)> {
        let url = format!(
            "https://kitsu.io/api/edge/anime?filter[text]={}&page[limit]=1",
            urlencoding::encode(search_title)
        );

        let res = self.client
            .get(&url)
            .header("Accept", "application/vnd.api+json")
            .header("Content-Type", "application/vnd.api+json")
            .send()
            .await
            .ok()?;

        if !res.status().is_success() {
            return None;
        }

        let parsed: KitsuResponse = res.json().await.ok()?;
        let items = parsed.data?;
        let item = items.first()?;
        let attrs = item.attributes.as_ref()?;

        let poster = attrs
            .poster_image
            .as_ref()
            .and_then(|p| p.original.clone().or_else(|| p.large.clone()))
            .unwrap_or_default();

        let banner = attrs
            .cover_image
            .as_ref()
            .and_then(|c| c.original.clone().or_else(|| c.large.clone()))
            .unwrap_or_default();

        if poster.is_empty() && banner.is_empty() {
            None
        } else {
            Some((poster, banner))
        }
    }

    /// Resolves high-resolution anime poster and banner artwork.
    pub async fn resolve(&self, raw_title: &str, fallback_poster: &str) -> AnimeArtwork {
        let key = Self::normalize_key(raw_title);
        if key.is_empty() {
            return AnimeArtwork {
                title: raw_title.to_string(),
                poster: fallback_poster.to_string(),
                banner: String::new(),
                poster_source: "fallback".to_string(),
                banner_source: String::new(),
                // The source dimensions have not been inspected; do not claim
                // that the fallback meets the HD target.
                width: 0,
                height: 0,
                score: 10,
            };
        }

        // 1. Check in-memory cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&key) {
                return cached.clone();
            }
        }

        let cleaned = Self::clean_title(raw_title);

        let mut poster = String::new();
        let mut banner = String::new();
        let mut poster_source = String::new();
        let mut banner_source = String::new();
        let mut score: u32 = 0;

        // 2. Query AniList
        if let Some((al_poster, al_banner)) = self.query_anilist(&cleaned).await {
            if !al_poster.is_empty() {
                poster = al_poster;
                poster_source = "AniList".to_string();
                score += 50;
            }
            if !al_banner.is_empty() {
                banner = al_banner;
                banner_source = "AniList".to_string();
                score += 50;
            }
        }

        // 3. Fallback / supplementary query to Kitsu
        if poster.is_empty() || banner.is_empty() {
            if let Some((kt_poster, kt_banner)) = self.query_kitsu(&cleaned).await {
                if poster.is_empty() && !kt_poster.is_empty() {
                    poster = kt_poster;
                    poster_source = "Kitsu".to_string();
                    score += 45;
                }
                if banner.is_empty() && !kt_banner.is_empty() {
                    banner = kt_banner;
                    banner_source = "Kitsu".to_string();
                    score += 45;
                }
            }
        }

        // 4. Fallback to existing poster if no remote artwork found
        if poster.is_empty() {
            poster = fallback_poster.to_string();
            poster_source = "SourceOriginal".to_string();
            score = 10;
        }

        let artwork = AnimeArtwork {
            title: raw_title.to_string(),
            poster,
            banner,
            poster_source,
            banner_source,
            // Metadata APIs provide URLs, not the actual image dimensions.
            // Leave these unknown until the image itself has been inspected.
            width: 0,
            height: 0,
            score,
        };

        // 5. Update cache and persist
        {
            let mut cache = self.cache.write().await;
            cache.insert(key, artwork.clone());

            if let Some(ref path) = self.cache_path {
                Self::save_cache_to_disk(path, &cache);
            }
        }

        artwork
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_title() {
        assert_eq!(
            ArtworkResolver::clean_title("Sousou no Frieren Episode 12 Subtitle Indonesia"),
            "Sousou no Frieren"
        );
        assert_eq!(
            ArtworkResolver::clean_title("[Batch] Jujutsu Kaisen Season 2 Sub Indo"),
            "Jujutsu Kaisen Season 2"
        );
        assert_eq!(
            ArtworkResolver::clean_title("One Piece BD Tamat"),
            "One Piece"
        );
        assert_eq!(
            ArtworkResolver::clean_title("Kimetsu no Yaiba: Yuukaku-hen (Movie)"),
            "Kimetsu no Yaiba: Yuukaku hen"
        );
    }
}
