//! The API and everything it serves.

pub mod audit;
pub mod auth;
pub mod bootstrap;
pub mod client_ip;
pub mod error;
pub mod limits;
pub mod pool;
pub mod rate;
pub mod routes;
pub mod state;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};

use error::ApiError;
use state::AppState;

const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

async fn health() -> Json<Value> {
    let mut body = json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });
    let sha: String = std::env::var("GIT_SHA").unwrap_or_default().chars().take(7).collect();
    if !sha.is_empty() {
        body["gitSha"] = json!(sha);
    }
    Json(body)
}

/// A path under `/api` that does not exist. Signed-out callers learn nothing about the paths.
async fn unknown_api_path(_user: auth::AuthUser) -> ApiError {
    ApiError::new(404, "Not found")
}

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::exact(HeaderValue::from_str(&state.config.frontend_url).unwrap_or(HeaderValue::from_static("http://localhost:5173"))))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::HEAD, Method::PUT, Method::PATCH, Method::POST, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    Router::new()
        .route("/api/health", get(health))
        .nest("/api/setup", routes::setup::router())
        .nest("/api/auth", routes::auth::router(state.clone()))
        .nest("/api/users", routes::users::router())
        .nest("/api/music-bots", routes::music_bots::router())
        .nest("/api/servers", routes::servers::router())
        .nest("/api/servers/{configId}/radio-stations", routes::radio::router())
        .nest("/api/servers/{configId}/chat-commands", routes::chat_commands::router())
        .nest("/api/servers/{configId}/music-requests", routes::music_requests::router())
        .nest("/api/servers/{configId}/virtual-servers", routes::virtual_servers::router())
        .nest("/api/servers/{configId}/vs/{sid}/channels", routes::channels::router())
        .nest("/api/servers/{configId}/vs/{sid}/clients", routes::clients::router())
        .nest("/api/servers/{configId}/vs/{sid}/server-groups", routes::simple::server_groups())
        .nest("/api/servers/{configId}/vs/{sid}/channel-groups", routes::simple::channel_groups())
        .nest("/api/servers/{configId}/vs/{sid}/permissions", routes::simple::permissions())
        .nest("/api/servers/{configId}/vs/{sid}/bans", routes::simple::bans())
        .nest("/api/servers/{configId}/vs/{sid}/tokens", routes::simple::tokens())
        .nest("/api/servers/{configId}/vs/{sid}/complaints", routes::simple::complaints())
        .nest("/api/servers/{configId}/vs/{sid}/messages", routes::simple::messages())
        .nest("/api/servers/{configId}/instance", routes::simple::instance())
        .route("/api/{*rest}", axum::routing::any(unknown_api_path))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(axum::middleware::from_fn(limits::security_headers))
        .layer(cors)
        .with_state(state)
}
