use std::path::Path;
use std::sync::Arc;

use axum::Router;
use dashmap::DashMap;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use shinobi::api::routes::{self, AppState};
use shinobi::storage::db::DbStore;
use shinobi::storage::manager::StorageManager;

/// Default CORS policy: localhost and RFC1918 LAN origins, no credentials.
const DEFAULT_CORS_REGEX: &str = r"^https?://(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]|192\.168\.[0-9]{1,3}\.[0-9]{1,3}|10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}|172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3})(:[0-9]{1,5})?$";

fn cors_layer() -> Option<CorsLayer> {
    let pattern = std::env::var("XWA_CORS_ORIGINS").unwrap_or_else(|_| DEFAULT_CORS_REGEX.into());
    match regex::Regex::new(&pattern) {
        Ok(regex) => Some(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::predicate(
                    move |origin, _| {
                        origin
                            .to_str()
                            .map(|value| regex.is_match(value))
                            .unwrap_or(false)
                    },
                ))
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_credentials(false),
        ),
        Err(e) => {
            tracing::warn!("Invalid XWA_CORS_ORIGINS regex ({}); CORS disabled", e);
            None
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("shinobi=info,tower_http=info")),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8060);

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "downloads".into());

    let db_path = std::env::var("SHINOBI_DB_PATH").unwrap_or_else(|_| "shinobi.db".into());

    let storage = Arc::new(StorageManager::new(&data_dir));

    let db = match DbStore::new(&db_path) {
        Ok(d) => {
            let store = Arc::new(d);
            tracing::info!("Database persistence enabled at {}", db_path);
            Some(store)
        }
        Err(e) => {
            tracing::warn!("Database persistence disabled: {}", e);
            None
        }
    };

    let jobs: Arc<DashMap<String, routes::JobInfo>> = Arc::new(DashMap::new());

    if let Some(ref db) = db {
        match db.load_jobs().await {
            Ok(saved) => {
                for job in &saved {
                    jobs.insert(job.id.clone(), job.clone());
                }
                tracing::info!("Loaded {} jobs from database", saved.len());
            }
            Err(e) => tracing::error!("Failed to load jobs from database: {}", e),
        }
    }

    let state = AppState::new(storage, jobs, Arc::new(DashMap::new()), db);

    let scheduler_state = state.clone();
    let api_routes = routes::create_router(state);

    let static_dir = if Path::new("static/browser").exists() {
        "static/browser"
    } else {
        "static"
    };
    tracing::info!(
        "Serving static files from: {} (UI may be absent)",
        static_dir
    );

    // Axum 0.8 no longer allows nesting at the root; the static/UI files are
    // the fallback for every non-/api path.
    let mut app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(static_dir));
    if let Some(cors) = cors_layer() {
        app = app.layer(cors);
    }

    tokio::spawn(async move {
        routes::scheduler_worker(scheduler_state).await;
    });

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Shinobi running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app).await.expect("Server failed");
}
