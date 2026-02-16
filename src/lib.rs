pub mod auth;
pub mod cli;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use time::Duration;
use tower_http::services::ServeDir;
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
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .with_state(state)
}
