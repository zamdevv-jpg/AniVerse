pub mod artwork;
pub mod doronime;
pub mod models;
pub mod nimegami;
pub mod source;
pub mod web;

use std::{path::PathBuf, sync::Arc};
use anyhow::Context;
use axum::{
    Router,
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
    routing::get,
};
use minijinja::Environment;
use tower_http::{
    compression::CompressionLayer,
    services::ServeDir,
    trace::TraceLayer,
};
use crate::{artwork::ArtworkResolver, source::Source, web::Site};

pub const EMBEDDED_TEMPLATES: &[(&str, &str)] = &[
    ("anime.html", include_str!("../templates/anime.html")),
    ("account.html", include_str!("../templates/account.html")),
    ("base.html", include_str!("../templates/base.html")),
    ("bookmark.html", include_str!("../templates/bookmark.html")),
    ("browse.html", include_str!("../templates/browse.html")),
    ("downloads.html", include_str!("../templates/downloads.html")),
    ("error.html", include_str!("../templates/error.html")),
    ("history.html", include_str!("../templates/history.html")),
    ("index.html", include_str!("../templates/index.html")),
    ("offline.html", include_str!("../templates/offline.html")),
    ("schedule.html", include_str!("../templates/schedule.html")),
    ("search.html", include_str!("../templates/search.html")),
    ("watch.html", include_str!("../templates/watch.html")),
    ("uploads.html", include_str!("../templates/uploads.html")),
    ("partials/card.html", include_str!("../templates/partials/card.html")),
    ("partials/footer.html", include_str!("../templates/partials/footer.html")),
    ("partials/header.html", include_str!("../templates/partials/header.html")),
    ("partials/icons.html", include_str!("../templates/partials/icons.html")),
    ("partials/mobile-nav.html", include_str!("../templates/partials/mobile-nav.html")),
];

pub const EMBEDDED_STATIC: &[(&str, &str, &[u8])] = &[
    ("favicon.svg", "image/svg+xml", include_bytes!("../static/favicon.svg")),
    ("css/style.css", "text/css", include_bytes!("../static/css/style.css")),
    ("js/app.js", "application/javascript", include_bytes!("../static/js/app.js")),
    ("js/supabase.js", "application/javascript", include_bytes!("../static/js/supabase.js")),
    ("js/logo.png", "image/png", include_bytes!("../static/js/logo.png")),
];

pub fn create_template_env(base_url: &str) -> Environment<'static> {
    let mut templates = Environment::new();
    let disk_templates = PathBuf::from("templates");
    if disk_templates.exists() && disk_templates.is_dir() {
        templates.set_loader(minijinja::path_loader("templates"));
    } else {
        templates.set_loader(|name| {
            for (tmpl_name, tmpl_content) in EMBEDDED_TEMPLATES {
                if *tmpl_name == name {
                    return Ok(Some(tmpl_content.to_string()));
                }
            }
            Ok(None)
        });
    }

    templates.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    let backdrop_base = base_url.to_string();
    templates.add_filter("backdrop", move |value: String| {
        source::resize_image(&value, 1080, 1450, &backdrop_base)
    });
    let poster_base = base_url.to_string();
    templates.add_filter("poster", move |value: String| {
        source::resize_image(&value, 384, 620, &poster_base)
    });
    templates.add_filter("initial", |value: String| {
        value
            .chars()
            .find(|c| !c.is_whitespace())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    });
    templates.add_filter("urlencode", |value: String| {
        models::encode(&value)
    });

    templates
}

pub async fn build_app(
    custom_static_dir: Option<PathBuf>,
    custom_data_dir: Option<PathBuf>,
) -> anyhow::Result<Router> {
    dotenvy::dotenv().ok();

    let base_url = std::env::var("NIMEGAMI_BASE_URL")
        .unwrap_or_else(|_| "https://nimegami.id".to_string());
    let site_name = std::env::var("SITE_NAME").unwrap_or_else(|_| "AniVerse".to_string());
    let tagline = std::env::var("SITE_TAGLINE")
        .unwrap_or_else(|_| "Nonton anime sub indo, update tiap hari.".to_string());
    let description = std::env::var("SITE_DESCRIPTION").unwrap_or_else(|_| {
        "AniVerse adalah katalog streaming anime subtitle Indonesia tanpa iklan dan open source."
            .to_string()
    });

    let source = Source::new(base_url.clone()).context("gagal menyiapkan HTTP client")?;
    let templates = create_template_env(&base_url);

    let data_dir = custom_data_dir.unwrap_or_else(|| {
        std::env::var("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data"))
    });

    let artwork = ArtworkResolver::new(Some(data_dir));

    let state = web::AppState {
        source: Arc::new(source),
        templates: Arc::new(templates),
        site: Site {
            name: site_name,
            tagline,
            description,
        },
        artwork: Arc::new(artwork),
        supabase_url: std::env::var("SUPABASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| option_env!("SUPABASE_URL").map(str::to_owned))
            .unwrap_or_default(),
        supabase_anon_key: std::env::var("SUPABASE_ANON_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| option_env!("SUPABASE_ANON_KEY").map(str::to_owned))
            .unwrap_or_default(),
    };

    // Hangatkan cache halaman utama di latar belakang supaya kunjungan pertama
    // tidak menunggu semua pengambilan ke Nimegami selesai.
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let warm = state.source.clone();
        handle.spawn(async move {
            let params = crate::models::BrowseParams::default();
            let _ = tokio::join!(
                async { warm.home().await },
                async { warm.genres().await },
                async { warm.browse(&params).await },
            );
        });
    }

    let static_dir = custom_static_dir.unwrap_or_else(|| {
        PathBuf::from(std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".to_string()))
    });

    let router = Router::new()
        .route("/", get(web::home))
        .route("/anime", get(web::browse))
        .route("/anime/{slug}", get(web::anime_detail))
        .route("/watch/{slug}", get(web::watch))
        .route("/jadwal", get(web::schedule))
        .route("/cari", get(web::search_page))
        .route("/bookmark", get(web::bookmarks))
        .route("/jadwal/anime", get(web::schedule_anime))
        .route("/akun", get(web::account))
        .route("/unggahan", get(web::uploads_page))
        .route("/histori", get(web::history_page))
        .route("/unduhan", get(web::downloads_page))
        .route("/offline", get(web::offline_page))
        .route("/acak", get(web::random))
        .route("/api/search", get(web::api_search))
        .route("/api/streams/{slug}", get(web::api_streams))
        .route("/api/video", get(web::api_video))
        .route("/api/artwork", get(web::api_artwork))
        .route("/service-worker.js", get(serve_service_worker))
        .route("/healthz", get(web::health))
        .route("/robots.txt", get(web::robots));

    let router = if static_dir.exists() && static_dir.is_dir() {
        router.nest_service("/static", ServeDir::new(static_dir))
    } else {
        router.route("/static/{*path}", get(serve_embedded_static))
    };

    let app = router
        .fallback(web::not_found)
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    Ok(app)
}

async fn serve_embedded_static(axum::extract::Path(path): axum::extract::Path<String>) -> Response<Body> {
    let clean_path = path.trim_start_matches('/');
    for (asset_path, mime_type, bytes) in EMBEDDED_STATIC {
        if *asset_path == clean_path {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, HeaderValue::from_static(mime_type))
                .header(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"))
                .body(Body::from(*bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

async fn serve_service_worker() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .header("Service-Worker-Allowed", "/")
        .body(Body::from(include_str!("../static/js/service-worker.js")))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn start_server(
    desired_port: u16,
    custom_static_dir: Option<PathBuf>,
    custom_data_dir: Option<PathBuf>,
) -> anyhow::Result<(u16, tokio::sync::oneshot::Sender<()>)> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let listener = if desired_port > 0 {
        match tokio::net::TcpListener::bind(format!("{host}:{desired_port}")).await {
            Ok(l) => l,
            Err(_) => {
                tokio::net::TcpListener::bind(format!("{host}:0")).await
                    .with_context(|| format!("gagal bind ke port dinamis {host}:0"))?
            }
        }
    } else {
        tokio::net::TcpListener::bind(format!("{host}:0")).await
            .with_context(|| format!("gagal bind ke port dinamis {host}:0"))?
    };

    let local_addr = listener.local_addr()?;
    let port = local_addr.port();
    let app = build_app(custom_static_dir, custom_data_dir).await?;

    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        tokio::select! {
            res = server => {
                if let Err(e) = res {
                    tracing::error!(error = %e, "server error");
                }
            }
            _ = shutdown_rx => {
                tracing::info!("Server dimatikan dengan sinyal internal");
            }
        }
    });

    Ok((port, shutdown_tx))
}
