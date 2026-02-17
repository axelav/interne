pub mod auth;
pub mod cli;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

pub const STATIC_HASH: &str = env!("STATIC_HASH");

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use time::Duration;
use axum::http::{header, HeaderValue};
use tower::ServiceBuilder;
use tower_http::{
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

async fn health() -> &'static str {
    "ok"
}

/// Build the full Axum application router.
///
/// Caller is responsible for running database migrations on `pool` beforehand.
/// This function sets up the session store (and migrates its table), then
/// assembles all route modules, middleware, and state.
pub async fn build_app(pool: SqlitePool, secure_cookies: bool) -> Router {
    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)))
        .with_secure(secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);

    let state = AppState { db: pool };

    Router::new()
        .route("/health", get(health))
        .merge(routes::auth::router())
        .merge(routes::entries::router())
        .merge(routes::collections::router())
        .merge(routes::export::router())
        .nest_service(
            "/static",
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=86400"),
                ))
                .service(ServeDir::new("static")),
        )
        .layer(session_layer)
        .layer(
            TraceLayer::new_for_http()
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}
