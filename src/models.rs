use serde::{Deserialize, Serialize};

/// Kartu anime/episode yang dipakai di grid, hero, jadwal, dan hasil pencarian.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Card {
    pub slug: String,
    pub title: String,
    pub image: String,
    pub href: String,
    pub kind: String,
    pub status: String,
    pub kind_label: String,
    pub episode: String,
    pub time: String,
    pub total: String,
    pub rank: usize,
    pub genres: Vec<String>,
    pub provider: String,
    /// Skor komunitas dari sumber (contoh: "9.3").
    pub score: String,
    /// Tahun rilis singkat (contoh: "2017").
    pub year: String,
    /// Waktu update dalam format ISO/UTC, dipakai untuk pengelompokan harian.
    pub updated_at: String,
}

/// Satu slide hero di beranda.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HeroSlide {
    pub slug: String,
    pub title: String,
    pub href: String,
    pub image: String,
    pub synopsis: String,
    pub rating: String,
    pub kind_label: String,
    pub meta: Vec<String>,
}

/// Sumber video untuk satu episode. `direct` berarti URL MP4 bisa diputar
/// langsung oleh elemen <video>, sedangkan embed hanya bisa dibuka di tab baru
/// atau iframe pihak ketiga.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamSource {
    pub quality: String,
    pub rank: u32,
    pub label: String,
    pub url: String,
    /// URL yang dipakai elemen <video>. Untuk sumber langsung ini adalah
    /// proxy lokal agar header Referer CDN tetap benar.
    pub play_url: String,
    /// Halaman streaming Nimegami (untuk tombol "buka sumber").
    pub page: String,
    pub direct: bool,
    pub available: bool,
    pub note: String,
}

/// Ringkasan kualitas untuk tombol pemilih di halaman tonton.
#[derive(Debug, Clone, Default, Serialize)]
pub struct QualityOption {
    pub quality: String,
    pub rank: u32,
    pub available: bool,
    pub url: String,
    pub server: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Genre {
    pub name: String,
    pub slug: String,
    pub href: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct InfoItem {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EpisodeLink {
    pub number: String,
    pub title: String,
    pub slug: String,
    pub date: String,
    pub href: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VideoServer {
    pub name: String,
    pub src: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadLink {
    pub server: String,
    pub quality: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AnimeDetail {
    pub slug: String,
    pub title: String,
    pub alt_title: String,
    pub image: String,
    pub banner_image: String,
    pub trailer: String,
    pub rating: String,
    pub score: String,
    pub votes: String,
    pub synopsis: String,
    pub status: String,
    pub studio: String,
    pub season: String,
    pub released: String,
    pub duration: String,
    pub kind: String,
    pub country: String,
    pub released_on: String,
    pub updated_on: String,
    pub facts: Vec<InfoItem>,
    pub genres: Vec<Genre>,
    pub episodes: Vec<EpisodeLink>,
    pub episode_count: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EpisodeDetail {
    pub slug: String,
    pub title: String,
    pub series_title: String,
    pub series_slug: String,
    pub series_image: String,
    pub series_detail: AnimeDetail,
    pub episode_number: String,
    pub released: String,
    pub kind: String,
    pub embed_url: String,
    pub source_url: String,
    pub servers: Vec<VideoServer>,
    pub streams: Vec<StreamSource>,
    pub qualities: Vec<QualityOption>,
    pub downloads: Vec<DownloadLink>,
    pub prev_slug: String,
    pub next_slug: String,
    pub recommended: Vec<Card>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScheduleDay {
    pub day: String,
    pub weekday: String,
    pub date_number: String,
    pub month_short: String,
    pub id: String,
    pub is_today: bool,
    pub entries: Vec<Card>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HomeData {
    pub latest: Vec<Card>,
    pub popular: Vec<Card>,
    pub hero: Vec<HeroSlide>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BrowsePage {
    pub cards: Vec<Card>,
    pub genres: Vec<Genre>,
    pub page: u32,
    pub has_next: bool,
}

/// Filter katalog. `kind` dipetakan ke parameter `type` milik sumber.
#[derive(Debug, Clone, Serialize)]
pub struct BrowseParams {
    pub q: String,
    pub status: String,
    pub kind: String,
    pub order: String,
    pub genres: Vec<String>,
    pub page: u32,
    pub provider: String,
}

impl Default for BrowseParams {
    fn default() -> Self {
        Self {
            q: String::new(),
            status: String::new(),
            kind: String::new(),
            order: String::new(),
            genres: Vec::new(),
            page: 1,
            provider: String::new(),
        }
    }
}

impl BrowseParams {
    pub fn clean(mut self) -> Self {
        self.q = self.q.trim().to_string();
        self.status = self.status.trim().to_string();
        self.kind = self.kind.trim().to_string();
        self.order = self.order.trim().to_string();
        self.provider = self.provider.trim().to_lowercase();
        self.page = self.page.max(1);
        self.genres = self
            .genres
            .into_iter()
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty() && !is_adult_genre(g))
            .collect();
        self.genres.dedup();
        self
    }

    /// Path relatif ke Otakudesu, sudah ter-encode.
    pub fn source_path(&self) -> String {
        if !self.q.is_empty() {
            return format!("/?s={}", encode(&self.q));
        }

        let mut query = Vec::new();
        if !self.status.is_empty() {
            query.push(format!("status={}", encode(&self.status)));
        }
        if !self.kind.is_empty() {
            query.push(format!("type={}", encode(&self.kind)));
        }
        if !self.order.is_empty() {
            query.push(format!("order={}", encode(&self.order)));
        }
        for genre in &self.genres {
            query.push(format!("genre%5B%5D={}", encode(genre)));
        }

        let base = if self.page > 1 {
            format!("/series/page/{}/", self.page)
        } else {
            "/series/".to_string()
        };

        if query.is_empty() {
            base
        } else {
            format!("{base}?{}", query.join("&"))
        }
    }

    /// URL halaman katalog di aplikasi ini.
    pub fn web_url(&self, page: u32) -> String {
        let mut query = Vec::new();
        for (key, value) in [
            ("q", self.q.as_str()),
            ("status", self.status.as_str()),
            ("type", self.kind.as_str()),
            ("order", self.order.as_str()),
            ("provider", self.provider.as_str()),
        ] {
            if !value.is_empty() {
                query.push(format!("{}={}", key, encode(value)));
            }
        }
        for genre in &self.genres {
            query.push(format!("genre={}", encode(genre)));
        }
        if page > 1 {
            query.push(format!("page={page}"));
        }

        if query.is_empty() {
            "/anime".to_string()
        } else {
            format!("/anime?{}", query.join("&"))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BrowseQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub order: Option<String>,
    #[serde(default)]
    pub genre: Vec<String>,
    pub page: Option<u32>,
    pub provider: Option<String>,
}

impl BrowseQuery {
    pub fn into_params(self) -> BrowseParams {
        BrowseParams {
            q: self.q.unwrap_or_default(),
            status: self.status.unwrap_or_default(),
            kind: self.kind.unwrap_or_default(),
            order: self.order.unwrap_or_default(),
            genres: self.genre,
            page: self.page.unwrap_or(1),
            provider: self.provider.unwrap_or_default(),
        }
        .clean()
    }
}

pub fn encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

/// Memeriksa apakah nama atau slug genre termasuk kategori dewasa / 18+.
/// Catatan Penting:
/// - Ecchi dan Gore secara eksplisit JANGAN disembunyikan.
/// - Rating 'Mature' / 'R-17+' (seperti Attack on Titan, Berserk, Tokyo Ghoul) BUKAN Hentai.
/// - Hanya konten 18+ murni (Hentai, Erotica, Smut, Porn, 18+) yang diblokir.
pub fn is_adult_genre(genre_str: &str) -> bool {
    let lower = genre_str.trim().to_lowercase();
    let words = lower
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.is_empty() {
        return false;
    }

    // MAL memakai "Adult Cast" untuk tema karakter dewasa, bukan konten 18+.
    if words == ["adult", "cast"] {
        return false;
    }

    let compact = words.join("");
    let adult_keywords = [
        "hentai",
        "erotica",
        "erotic",
        "smut",
        "porn",
        "porno",
        "nsfw",
        "rx",
        "r18",
        "18",
        "18禁",
        "netorare",
        "persenggamaan",
    ];

    words.iter().any(|word| adult_keywords.contains(word))
        || compact == "adult"
        || (words.contains(&"adult") && !words.contains(&"cast"))
}

/// Memeriksa apakah kartu mengandung genre atau indikasi konten 18+
pub fn is_adult_card(card: &Card) -> bool {
    if card.genres.iter().any(|g| is_adult_genre(g)) {
        return true;
    }
    is_adult_genre(&card.title) || is_adult_genre(&card.slug)
}

/// Normalisasi judul anime untuk deduplikasi antar provider.
/// Menghilangkan perbedaan spasi, huruf besar/kecil, tanda baca,
/// serta imbuhan seperti 'sub indo', 'subtitle indonesia', 'season', dll.
pub fn normalize_title_for_dedup(title: &str) -> String {
    let lower = title.to_lowercase();
    let cleaned = lower
        .replace("subtitle indonesia", "")
        .replace("sub indo", "")
        .replace("(tv)", "")
        .replace("[batch]", "")
        .replace("batch", "");

    let filtered = cleaned
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>();

    let tokens = filtered.split_whitespace().collect::<Vec<_>>();
    let mut normalized = Vec::with_capacity(tokens.len());
    let mut index = 0;

    while index < tokens.len() {
        if tokens[index] == "season"
            && let Some(number) = tokens.get(index + 1).and_then(|token| ordinal_number(token))
        {
            normalized.push("season".to_string());
            normalized.push(number.to_string());
            index += 2;
            continue;
        }

        if let Some(number) = ordinal_number(tokens[index])
            && tokens.get(index + 1) == Some(&"season")
        {
            normalized.push("season".to_string());
            normalized.push(number.to_string());
            index += 2;
            continue;
        }

        normalized.push(tokens[index].to_string());
        index += 1;
    }

    normalized.join(" ")
}

fn ordinal_number(token: &str) -> Option<u32> {
    match token {
        "first" => return Some(1),
        "second" => return Some(2),
        "third" => return Some(3),
        "fourth" => return Some(4),
        "fifth" => return Some(5),
        _ => {}
    }

    if let Ok(number) = token.parse() {
        return Some(number);
    }

    let digits = token.chars().take_while(char::is_ascii_digit).collect::<String>();
    if digits.is_empty() {
        return None;
    }

    match &token[digits.len()..] {
        "st" | "nd" | "rd" | "th" => digits.parse().ok(),
        _ => None,
    }
}

/// Menghapus anime duplikat dari kumpulan Card berdasarkan kemiripan judul.
/// Jika anime sudah ada dari sumber sebelumnya, duplikat akan diabaikan.
pub fn deduplicate_cards(cards: Vec<Card>) -> Vec<Card> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(cards.len());

    for card in cards {
        let key = normalize_title_for_dedup(&card.title);
        if key.is_empty() {
            result.push(card);
            continue;
        }
        if seen.insert(key) {
            result.push(card);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adult_genre_blocking() {
        // Harus diblokir (18+)
        assert!(is_adult_genre("hentai"));
        assert!(is_adult_genre("Hentai"));
        assert!(is_adult_genre("erotica"));
        assert!(is_adult_genre("Erotica"));
        assert!(is_adult_genre("erotic"));
        assert!(is_adult_genre("smut"));
        assert!(is_adult_genre("18+"));
        assert!(is_adult_genre("r18"));
        assert!(is_adult_genre("r-18"));
        assert!(is_adult_genre("porn"));

        // Harus diizinkan (tidak diblokir)
        assert!(!is_adult_genre("ecchi"));
        assert!(!is_adult_genre("Ecchi"));
        assert!(!is_adult_genre("Gore"));
        assert!(!is_adult_genre("gore"));
        assert!(!is_adult_genre("Mature"));
        assert!(!is_adult_genre("R-17+"));
        assert!(!is_adult_genre("Action"));
        assert!(!is_adult_genre("Comedy"));
        assert!(!is_adult_genre("Romance"));
        assert!(!is_adult_genre("Adult Cast"));
        assert!(!is_adult_genre("adult-cast"));
    }

    #[test]
    fn test_title_deduplication() {
        let key1 = normalize_title_for_dedup("Jujutsu Kaisen Sub Indo");
        let key2 = normalize_title_for_dedup("Jujutsu Kaisen");
        assert_eq!(key1, key2);

        let key3 = normalize_title_for_dedup("Tokyo Revengers Season 4 Sub Indo");
        let key4 = normalize_title_for_dedup("Tokyo Revengers: Season 4");
        assert_eq!(key3, key4);
    }

    #[test]
    fn test_deduplicate_cards() {
        let cards = vec![
            Card {
                title: "Jujutsu Kaisen Sub Indo".into(),
                slug: "jujutsu-kaisen-sub-indo".into(),
                provider: "Otakudesu".into(),
                ..Default::default()
            },
            Card {
                title: "Jujutsu Kaisen".into(),
                slug: "ng-jujutsu-kaisen-sub-indo".into(),
                provider: "Nimegami".into(),
                ..Default::default()
            },
            Card {
                title: "Frieren: Beyond Journey's End".into(),
                slug: "ng-frieren".into(),
                provider: "Nimegami".into(),
                ..Default::default()
            },
        ];

        let deduped = deduplicate_cards(cards);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].provider, "Otakudesu");
        assert_eq!(deduped[1].title, "Frieren: Beyond Journey's End");
    }
}
