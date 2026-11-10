use std::sync::Arc;

use axum::{
    body::Body,
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::Query;
use chrono::{Datelike, Duration as ChronoDuration, FixedOffset, NaiveDate, Utc};
use minijinja::Environment;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    artwork::ArtworkResolver,
    models::{BrowseQuery, encode, normalize_title_for_dedup},
    source::Source,
};

#[derive(Clone)]
pub struct Site {
    pub name: String,
    pub tagline: String,
    pub description: String,
}

#[derive(Clone)]
pub struct AppState {
    pub source: Arc<Source>,
    pub templates: Arc<Environment<'static>>,
    pub site: Site,
    pub artwork: Arc<ArtworkResolver>,
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

fn escape_html_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
}

fn base_ctx(state: &AppState, active: &str, title: &str, description: &str) -> Value {
    json!({
        "site_name": state.site.name,
        "site_tagline": state.site.tagline,
        "meta_title": title,
        "meta_description": description,
        "active": active,
        "source_host": state.source.host(),
        "supabase_url": escape_html_attribute(&state.supabase_url),
        "supabase_anon_key": escape_html_attribute(&state.supabase_anon_key),
        "year": chrono::Utc::now().year(),
        "query": "",
    })
}

fn with(state: &AppState, active: &str, title: &str, description: &str) -> Value {
    base_ctx(state, active, title, description)
}

fn insert(ctx: &mut Value, key: &str, value: impl serde::Serialize) {
    if let Some(object) = ctx.as_object_mut() {
        object.insert(key.to_string(), serde_json::to_value(value).unwrap_or(Value::Null));
    }
}

fn render(state: &AppState, template: &str, ctx: &Value, status: StatusCode) -> Response {
    let result = state
        .templates
        .get_template(template)
        .and_then(|tmpl| tmpl.render(ctx));

    match result {
        Ok(html) => (status, Html(html)).into_response(),
        Err(error) => {
            tracing::error!(template, %error, "gagal merender template");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!(
                    "<!doctype html><meta charset=\"utf-8\"><title>Kesalahan render</title>\
                     <body style=\"font-family:system-ui;background:#0b0b0d;color:#eee;padding:40px\">\
                     <h1>Kesalahan render</h1><pre style=\"white-space:pre-wrap;color:#f99\">{error}</pre></body>"
                )),
            )
                .into_response()
        }
    }
}

fn error_page(state: &AppState, status: StatusCode, title: &str, message: &str) -> Response {
    let mut ctx = with(
        state,
        "",
        &format!("{title} — {}", state.site.name),
        message,
    );
    insert(&mut ctx, "code", status.as_u16());
    insert(&mut ctx, "title", title);
    insert(&mut ctx, "message", message);
    render(state, "error.html", &ctx, status)
}

pub async fn home(State(state): State<AppState>) -> Response {
    // Genre diambil dari halaman katalog, jadi fetch-nya dijalankan bersamaan
    // dengan beranda agar satu kunjungan tidak menunggu dua kali jaringan.
    let (home_result, genres_result) = tokio::join!(state.source.home(), state.source.genres());
    let home = home_result.unwrap_or_else(|error| {
        tracing::error!(%error, "gagal memuat beranda Nimegami");
        crate::models::HomeData::default()
    });

    let source_error = home.latest.is_empty() && home.hero.is_empty();
    let genres = genres_result.unwrap_or_default();

    let mut ctx = with(
        &state,
        "home",
        &format!("{} — Nonton Anime Sub Indo Gratis", state.site.name),
        &state.site.description,
    );
    insert(&mut ctx, "hero", &home.hero);
    insert(&mut ctx, "genres", &genres);
    insert(&mut ctx, "latest", home.latest.iter().take(18).collect::<Vec<_>>());
    insert(&mut ctx, "popular", home.popular.iter().take(10).collect::<Vec<_>>());
    insert(&mut ctx, "source_error", source_error);

    render(&state, "index.html", &ctx, StatusCode::OK)
}

pub async fn browse(
    State(state): State<AppState>,
    Query(query): Query<BrowseQuery>,
) -> Response {
    let params = query.into_params();
    let result = state.source.browse(&params).await;

    let page = match result {
        Ok(page) => page,
        Err(error) => {
            tracing::error!(%error, "gagal memuat katalog");
            return error_page(
                &state,
                StatusCode::BAD_GATEWAY,
                "Sumber sedang tidak bisa diakses",
                "Katalog anime belum dapat dimuat. Coba lagi beberapa saat lagi.",
            );
        }
    };

    let is_search = !params.q.is_empty();
    let title = if is_search {
        format!("Hasil pencarian “{}” — {}", params.q, state.site.name)
    } else {
        format!("Katalog Anime — {}", state.site.name)
    };
    let description = if is_search {
        format!("Hasil pencarian anime “{}” subtitle Indonesia.", params.q)
    } else {
        "Jelajahi katalog anime subtitle Indonesia".to_string()
    };

    let mut ctx = with(&state, "browse", &title, &description);
    insert(&mut ctx, "query", &params.q);
    insert(&mut ctx, "params", &params);
    insert(&mut ctx, "cards", &page.cards);
    insert(&mut ctx, "genres", &page.genres);
    insert(&mut ctx, "selected_genres", &params.genres);
    insert(&mut ctx, "page", page.page);
    insert(&mut ctx, "has_next", page.has_next);
    insert(
        &mut ctx,
        "prev_url",
        params.web_url(page.page.saturating_sub(1).max(1)),
    );
    insert(&mut ctx, "next_url", params.web_url(page.page + 1));
    insert(&mut ctx, "is_search", is_search);

    render(&state, "browse.html", &ctx, StatusCode::OK)
}

pub async fn anime_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    // Rekomendasi diambil dari snapshot cache supaya halaman anime tidak
    // menunggu permintaan ke beranda Nimegami.
    let recommended = state.source.latest_cached(12);
    let detail_result = state.source.detail(&slug).await;

    let detail = match detail_result {
        Ok(detail) => detail,
        Err(error) => {
            tracing::warn!(%error, slug, "gagal memuat detail anime");
            let is_adult = error.to_string().contains("18+");
            return error_page(
                &state,
                if is_adult { StatusCode::FORBIDDEN } else { StatusCode::NOT_FOUND },
                if is_adult { "Konten 18+ Diblokir" } else { "Anime tidak ditemukan" },
                if is_adult {
                    "Anime ini diblokir dan tidak dapat diakses karena mengandung konten 18+ / dewasa."
                } else {
                    "Judul ini tidak tersedia, atau halaman sumber sedang bermasalah."
                },
            );
        }
    };

    let latest_episode = detail.episodes.first().cloned();
    let first_episode = detail.episodes.last().cloned();
    let title = format!("{} Sub Indo — {}", detail.title, state.site.name);
    let description = if detail.synopsis.is_empty() {
        format!("Nonton {} subtitle Indonesia gratis.", detail.title)
    } else {
        detail.synopsis.chars().take(180).collect::<String>()
    };

    let mut ctx = with(&state, "browse", &title, &description);
    insert(&mut ctx, "detail", &detail);
    insert(&mut ctx, "latest_episode", &latest_episode);
    insert(&mut ctx, "first_episode", &first_episode);
    insert(&mut ctx, "recommended", &recommended);

    render(&state, "anime.html", &ctx, StatusCode::OK)
}

pub async fn watch(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    let episode = match state.source.episode(&slug).await {
        Ok(episode) => episode,
        Err(error) => {
            tracing::warn!(%error, slug, "gagal memuat episode");
            let is_adult = error.to_string().contains("18+");
            return error_page(
                &state,
                if is_adult { StatusCode::FORBIDDEN } else { StatusCode::NOT_FOUND },
                if is_adult { "Konten 18+ Diblokir" } else { "Episode tidak ditemukan" },
                if is_adult {
                    "Episode ini diblokir dan tidak dapat diakses karena mengandung konten 18+ / dewasa."
                } else {
                    "Episode ini tidak tersedia, atau halaman sumber sedang bermasalah."
                },
            );
        }
    };

    let title = format!("Nonton {} — {}", episode.title, state.site.name);
    let description = format!(
        "Streaming {} subtitle Indonesia. Kualitas otomatis dan sumber yang tidak tersedia dinonaktifkan.",
        episode.title
    );

    let streams_json = serde_json::to_string(&episode.streams).unwrap_or_else(|_| "[]".into());
    let available_qualities = episode.qualities.iter().filter(|q| q.available).count();

    let mut ctx = with(&state, "browse", &title, &description);
    insert(&mut ctx, "episode", &episode);
    insert(&mut ctx, "series", &episode.series_detail);
    insert(&mut ctx, "streams_json", streams_json);
    insert(&mut ctx, "available_qualities", available_qualities);
    insert(
        &mut ctx,
        "prev_url",
        if episode.prev_slug.is_empty() {
            String::new()
        } else {
            format!("/watch/{}", encode(&episode.prev_slug))
        },
    );
    insert(
        &mut ctx,
        "next_url",
        if episode.next_slug.is_empty() {
            String::new()
        } else {
            format!("/watch/{}", encode(&episode.next_slug))
        },
    );

    render(&state, "watch.html", &ctx, StatusCode::OK)
}

#[derive(Deserialize, Default)]
pub struct ScheduleQuery {
    pub week: Option<String>,
}

pub async fn schedule(
    State(state): State<AppState>,
    Query(query): Query<ScheduleQuery>,
) -> Response {
    let jakarta = FixedOffset::east_opt(7 * 60 * 60).expect("offset WIB valid");
    let today = Utc::now().with_timezone(&jakarta).date_naive();
    let requested = query
        .week
        .as_deref()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .unwrap_or(today);
    let week_start = requested
        - ChronoDuration::days(requested.weekday().num_days_from_monday() as i64);
    let week_end = week_start + ChronoDuration::days(6);

    let days = state.source.schedule(week_start).await.unwrap_or_else(|error| {
        tracing::warn!(%error, "gagal memuat jadwal MyAnimeList");
        Vec::new()
    });

    let mut ctx = with(
        &state,
        "schedule",
        &format!("Jadwal Anime Mingguan — {}", state.site.name),
        "Jadwal anime mingguan dari MyAnimeList, dikelompokkan menurut tanggal.",
    );
    insert(&mut ctx, "days", &days);
    insert(&mut ctx, "empty", days.is_empty());
    insert(&mut ctx, "week_label", format!("{} – {}", indonesian_date(week_start), indonesian_date(week_end)));
    insert(&mut ctx, "week_start", week_start.format("%Y-%m-%d").to_string());
    insert(&mut ctx, "previous_week", (week_start - ChronoDuration::days(7)).format("%Y-%m-%d").to_string());
    insert(&mut ctx, "next_week", (week_start + ChronoDuration::days(7)).format("%Y-%m-%d").to_string());

    render(&state, "schedule.html", &ctx, StatusCode::OK)
}

fn indonesian_date(date: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus",
        "September", "Oktober", "November", "Desember",
    ];
    format!("{} {} {}", date.day(), MONTHS[date.month0() as usize], date.year())
}

#[derive(Deserialize)]
pub struct ScheduleAnimeQuery {
    pub title: String,
}

pub async fn schedule_anime(
    State(state): State<AppState>,
    Query(query): Query<ScheduleAnimeQuery>,
) -> Response {
    let title = query.title.trim();
    if title.chars().count() < 2 {
        return Redirect::to("/anime").into_response();
    }
    let wanted = normalize_title_for_dedup(title);
    let results = state.source.search(title, 10).await.unwrap_or_default();
    if let Some(card) = results
        .iter()
        .find(|card| normalize_title_for_dedup(&card.title) == wanted)
        .or_else(|| results.first())
    {
        return Redirect::to(&card.href).into_response();
    }

    Redirect::to(&format!("/cari?q={}", encode(title))).into_response()
}

pub async fn search_page(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let q = query.q.unwrap_or_default().trim().to_string();
    let genres = state.source.genres().await.unwrap_or_default();

    let (results, recommended) = if q.chars().count() >= 2 {
        (
            state
                .source
                .search(&q, 40)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "gagal mencari");
                    Vec::new()
                }),
            Vec::new(),
        )
    } else {
        let latest = state.source.latest(12).await;
        (Vec::new(), latest)
    };

    let title = if q.is_empty() {
        format!("Cari Anime — {}", state.site.name)
    } else {
        format!("Cari “{q}” — {}", state.site.name)
    };

    let mut ctx = with(
        &state,
        "search",
        &title,
        "Cari anime subtitle Indonesia dengan cepat.",
    );
    insert(&mut ctx, "query", &q);
    insert(&mut ctx, "results", &results);
    insert(&mut ctx, "recommended", &recommended);
    insert(&mut ctx, "genres", &genres);
    insert(&mut ctx, "has_query", q.chars().count() >= 2);
    insert(&mut ctx, "result_count", results.len());

    render(&state, "search.html", &ctx, StatusCode::OK)
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

pub async fn api_search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Response {
    let q = query.q.unwrap_or_default().trim().to_string();
    let results = if q.chars().count() >= 2 {
        state.source.search(&q, 8).await.unwrap_or_else(|error| {
            tracing::warn!(%error, "gagal mencari lewat API");
            Vec::new()
        })
    } else {
        Vec::new()
    };

    (
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(json!({
            "query": q,
            "count": results.len(),
            "results": results,
        })),
    )
        .into_response()
}

/// Endpoint ringan untuk memeriksa ulang sumber video tanpa memuat halaman.
pub async fn api_streams(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    match state.source.episode(&slug).await {
        Ok(episode) => (
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({
                "slug": episode.slug,
                "streams": episode.streams,
                "qualities": episode.qualities,
            })),
        )
            .into_response(),
        Err(error) => {
            tracing::warn!(%error, slug, "gagal memuat sumber video");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "Sumber video belum bisa dimuat" })),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct VideoProxyQuery {
    pub url: String,
    pub download: Option<bool>,
    pub filename: Option<String>,
}

fn safe_video_download_name(value: Option<&str>) -> String {
    let mut name = value
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || " ._-".contains(*character))
        .take(110)
        .collect::<String>();
    name = name.trim().trim_matches('.').to_string();
    if name.is_empty() {
        name = "aniverse-video".to_string();
    }
    if !name.to_ascii_lowercase().ends_with(".mp4") {
        name.push_str(".mp4");
    }
    name
}

pub async fn api_video(
    State(state): State<AppState>,
    Query(query): Query<VideoProxyQuery>,
    request_headers: HeaderMap,
) -> Response {
    let allowed = reqwest::Url::parse(&query.url)
        .ok()
        .filter(|url| {
            let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
            url.scheme() == "https"
                && url.port_or_known_default() == Some(443)
                && (host == "berkasdrive.com" || host.ends_with(".berkasdrive.com"))
        })
        .is_some();
    if !allowed {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let range = request_headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let upstream = match state.source.nimegami().proxy_video(&query.url, range).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "gagal memproksi video dari Berkasdrive");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let status = upstream.status();
    let mut response = Response::builder().status(status);
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            response = response.header(name, value);
        }
    }
    if query.download.unwrap_or(false) {
        let filename = safe_video_download_name(query.filename.as_deref());
        response = response.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        );
    }
    response
        .body(Body::from_stream(upstream.bytes_stream()))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

#[derive(Debug, Deserialize)]
pub struct ArtworkQuery {
    pub title: Option<String>,
    pub fallback: Option<String>,
}

pub async fn api_artwork(
    State(state): State<AppState>,
    Query(query): Query<ArtworkQuery>,
) -> Response {
    let title = query.title.unwrap_or_default().trim().to_string();
    let fallback = query.fallback.unwrap_or_default();
    if title.is_empty() {
        return (StatusCode::BAD_REQUEST, "Parameter 'title' wajib diisi").into_response();
    }

    let artwork = state.artwork.resolve(&title, &fallback).await;
    (
        [(header::CACHE_CONTROL, "public, max-age=86400")],
        Json(artwork),
    )
        .into_response()
}

pub async fn random(State(state): State<AppState>) -> Response {
    let latest = state.source.latest(60).await;

    if latest.is_empty() {
        return Redirect::to("/anime").into_response();
    }

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0);
    let index = nanos % latest.len();
    Redirect::to(&latest[index].href).into_response()
}

pub async fn bookmarks(State(state): State<AppState>) -> Response {
    let ctx = with(
        &state,
        "bookmark",
        &format!("Bookmark Saya — {}", state.site.name),
        "Daftar anime yang kamu simpan di perangkat ini.",
    );
    render(&state, "bookmark.html", &ctx, StatusCode::OK)
}

fn local_feature_page(
    state: &AppState,
    active: &str,
    template: &str,
    title: &str,
    description: &str,
) -> Response {
    let ctx = with(
        state,
        active,
        &format!("{title} — {}", state.site.name),
        description,
    );
    render(state, template, &ctx, StatusCode::OK)
}

pub async fn account(State(state): State<AppState>) -> Response {
    local_feature_page(
        &state,
        "account",
        "account.html",
        "Akun",
        "Profil, histori, unduhan, dan koleksi offline AniVerse di perangkat ini.",
    )
}

pub async fn uploads_page(State(state): State<AppState>) -> Response {
    let ctx = with(
        &state,
        "uploads",
        &format!("Unggah Anime — {}", state.site.name),
        "Unggah poster dan episode ke koleksi AniVerse menggunakan Supabase Storage.",
    );
    render(&state, "uploads.html", &ctx, StatusCode::OK)
}

pub async fn history_page(State(state): State<AppState>) -> Response {
    local_feature_page(
        &state,
        "history",
        "history.html",
        "Histori tontonan",
        "Lanjutkan anime yang terakhir kamu tonton.",
    )
}

pub async fn downloads_page(State(state): State<AppState>) -> Response {
    local_feature_page(
        &state,
        "downloads",
        "downloads.html",
        "Unduhan",
        "Tautan unduhan episode yang disimpan di perangkat ini.",
    )
}

pub async fn offline_page(State(state): State<AppState>) -> Response {
    local_feature_page(
        &state,
        "offline",
        "offline.html",
        "Tersimpan offline",
        "Halaman detail anime yang disimpan untuk dibuka tanpa internet.",
    )
}

pub async fn not_found(State(state): State<AppState>) -> Response {
    error_page(
        &state,
        StatusCode::NOT_FOUND,
        "Halaman tidak ditemukan",
        "Alamat yang kamu buka tidak ada. Mungkin sudah dipindahkan atau salah ketik.",
    )
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn robots() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "User-agent: *\nAllow: /\nDisallow: /api/\n",
    )
}
