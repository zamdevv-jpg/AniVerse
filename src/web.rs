use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::Query;
use chrono::Datelike;
use minijinja::Environment;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    artwork::ArtworkResolver,
    models::{BrowseParams, BrowseQuery, encode, normalize_title_for_dedup},
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
}

fn base_ctx(state: &AppState, active: &str, title: &str, description: &str) -> Value {
    json!({
        "site_name": state.site.name,
        "site_tagline": state.site.tagline,
        "meta_title": title,
        "meta_description": description,
        "active": active,
        "source_host": state.source.host(),
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
                     <body style=\"font-family:system-ui;background:#08080b;color:#eee;padding:40px\">\
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
    let browse_ongoing = BrowseParams {
        status: "Ongoing".into(),
        order: "update".into(),
        ..Default::default()
    };
    let browse_completed = BrowseParams {
        status: "Completed".into(),
        order: "update".into(),
        ..Default::default()
    };

    let (home_result, ongoing_result, completed_result, schedule_result) = tokio::join!(
        state.source.home(),
        state.source.browse(&browse_ongoing),
        state.source.browse(&browse_completed),
        state.source.schedule()
    );

    let (latest, popular) = match home_result {
        Ok(home) => (home.latest, home.popular),
        Err(error) => {
            tracing::error!(%error, "gagal memuat beranda");
            (Vec::new(), Vec::new())
        }
    };

    let ongoing = ongoing_result.map(|page| page.cards).unwrap_or_else(|error| {
        tracing::warn!(%error, "gagal memuat ongoing");
        Vec::new()
    });
    let completed = completed_result
        .map(|page| page.cards)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "gagal memuat completed");
            Vec::new()
        });
    let schedule = schedule_result.unwrap_or_else(|error| {
        tracing::warn!(%error, "gagal memuat jadwal");
        Vec::new()
    });

    let source_error = latest.is_empty() && popular.is_empty();
    let hero: Vec<_> = latest.iter().take(6).cloned().collect();
    let today = schedule.iter().find(|day| day.is_today).cloned();

    let mut ctx = with(
        &state,
        "home",
        &format!(
            "{} — Nonton Anime Sub Indo Gratis",
            state.site.name
        ),
        &state.site.description,
    );
    insert(&mut ctx, "hero", &hero);
    insert(&mut ctx, "latest", latest.iter().take(18).collect::<Vec<_>>());
    insert(&mut ctx, "popular", popular);
    insert(
        &mut ctx,
        "ongoing",
        ongoing.iter().take(12).collect::<Vec<_>>(),
    );
    insert(
        &mut ctx,
        "completed",
        completed.iter().take(12).collect::<Vec<_>>(),
    );
    insert(&mut ctx, "today", &today);
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
                "Katalog anime belum dapat dimuat dari Otakudesu. Coba lagi beberapa saat lagi.",
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
        "Jelajahi katalog anime subtitle Indonesia: ongoing, completed, movie, dan genre pilihan.".to_string()
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
        if page.page > 1 {
            params.web_url(page.page - 1)
        } else {
            String::new()
        },
    );
    insert(
        &mut ctx,
        "next_url",
        if page.has_next {
            params.web_url(page.page + 1)
        } else {
            String::new()
        },
    );
    insert(&mut ctx, "is_search", is_search);

    render(&state, "browse.html", &ctx, StatusCode::OK)
}

pub async fn anime_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Response {
    let mut detail = match state.source.detail(&slug).await {
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
                    "Judul ini tidak tersedia di sumber, atau halaman sumber sedang bermasalah."
                },
            );
        }
    };

    // Smart Anime Artwork Resolver (HD cover and banner)
    let artwork = state.artwork.resolve(&detail.title, &detail.image).await;
    if !artwork.poster.is_empty() && artwork.score >= 45 {
        detail.image = artwork.poster;
    }
    if !artwork.banner.is_empty() {
        detail.banner_image = artwork.banner;
    }

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
                    "Episode ini tidak tersedia di sumber, atau halaman sumber sedang bermasalah."
                },
            );
        }
    };

    let series = if episode.series_slug.is_empty() {
        None
    } else {
        match state.source.detail(&episode.series_slug).await {
            Ok(detail) => Some(detail),
            Err(error) => {
                let message = error.to_string().to_lowercase();
                if message.contains("18+")
                    || message.contains("dewasa")
                    || message.contains("hentai")
                    || message.contains("erotica")
                {
                    return error_page(
                        &state,
                        StatusCode::FORBIDDEN,
                        "Konten 18+ Diblokir",
                        "Episode ini diblokir dan tidak dapat diakses karena mengandung genre dewasa.",
                    );
                }
                None
            }
        }
    };

    let title = format!(
        "Nonton {} — {}",
        episode.title,
        state.site.name
    );
    let description = format!(
        "Streaming {} subtitle Indonesia. Pilih server video yang tersedia.",
        episode.title
    );

    let mut ctx = with(&state, "browse", &title, &description);
    insert(&mut ctx, "episode", &episode);
    insert(&mut ctx, "series", &series);
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

pub async fn schedule(State(state): State<AppState>) -> Response {
    let days = state.source.schedule().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "gagal memuat jadwal");
        Vec::new()
    });

    let mut ctx = with(
        &state,
        "schedule",
        &format!("Jadwal Rilis Anime — {}", state.site.name),
        "Jadwal rilis anime mingguan subtitle Indonesia berdasarkan hari.",
    );
    insert(&mut ctx, "days", &days);
    insert(&mut ctx, "empty", days.is_empty());

    render(&state, "schedule.html", &ctx, StatusCode::OK)
}

#[derive(Deserialize)]
pub struct ScheduleAnimeQuery {
    title: String,
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

    let nimegami = state
        .source
        .nimegami
        .search(title, 10)
        .await
        .unwrap_or_default();
    if let Some(card) = nimegami
        .iter()
        .find(|card| normalize_title_for_dedup(&card.title) == wanted)
    {
        return Redirect::to(&card.href).into_response();
    }

    let doronime = state
        .source
        .doronime
        .search(title, 10)
        .await
        .unwrap_or_default();
    if let Some(card) = doronime
        .iter()
        .find(|card| normalize_title_for_dedup(&card.title) == wanted)
    {
        return Redirect::to(&card.href).into_response();
    }

    let fallback = state.source.search(title, 10).await.unwrap_or_default();
    if let Some(card) = fallback
        .iter()
        .find(|card| normalize_title_for_dedup(&card.title) == wanted)
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
                .search(&q, 30)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "gagal mencari");
                    Vec::new()
                }),
            Vec::new(),
        )
    } else {
        let latest = state
            .source
            .home()
            .await
            .map(|home| home.latest)
            .unwrap_or_default();
        (Vec::new(), latest.into_iter().take(12).collect())
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
    let latest = state
        .source
        .home()
        .await
        .map(|home| home.latest)
        .unwrap_or_default();

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

fn local_feature_page(state: &AppState, active: &str, template: &str, title: &str, description: &str) -> Response {
    let ctx = with(state, active, &format!("{title} — {}", state.site.name), description);
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

pub async fn history_page(State(state): State<AppState>) -> Response {
    local_feature_page(&state, "history", "history.html", "Histori tontonan", "Lanjutkan anime yang terakhir kamu tonton.")
}

pub async fn downloads_page(State(state): State<AppState>) -> Response {
    local_feature_page(&state, "downloads", "downloads.html", "Unduhan", "Tautan unduhan episode yang disimpan di perangkat ini.")
}

pub async fn offline_page(State(state): State<AppState>) -> Response {
    local_feature_page(&state, "offline", "offline.html", "Tersimpan offline", "Halaman detail anime yang disimpan untuk dibuka tanpa internet.")
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
