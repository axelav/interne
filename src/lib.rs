pub mod auth;
pub mod cli;
pub mod config;
pub mod connection_tokens;
pub mod db;
pub mod error;
pub mod github;
pub mod models;
pub mod routes;

pub const STATIC_HASH: &str = env!("STATIC_HASH");

use std::sync::Arc;

use axum::http::{HeaderValue, Request, Uri, header};
use axum::{Router, routing::get};
use sqlx::SqlitePool;
use time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite};
use tower_sessions_sqlx_store::SqliteStore;
use tracing::Level;

use crate::config::AuthConfig;
use crate::github::GitHubProvider;

#[derive(Clone)]
pub struct AuthServices {
    pub config: AuthConfig,
    pub github: Arc<dyn GitHubProvider>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub auth: AuthServices,
}

async fn health() -> &'static str {
    "ok"
}

fn trace_path(uri: &Uri) -> &str {
    uri.path()
}

/// Build the full Axum application router.
///
/// Caller is responsible for running database migrations on `pool` beforehand.
/// This function sets up the session store (and migrates its table), then
/// assembles all route modules, middleware, and state.
pub async fn build_app(pool: SqlitePool, secure_cookies: bool, auth: AuthServices) -> Router {
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

    let state = AppState { db: pool, auth };

    Router::new()
        .route("/health", get(health))
        .merge(routes::auth::router())
        .merge(routes::entries::router())
        .merge(routes::collections::router())
        .merge(routes::export::router())
        .merge(routes::tags::router())
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
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        path = %trace_path(request.uri()),
                    )
                })
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::trace_path;

    #[test]
    fn request_log_path_excludes_recovery_query_string() {
        let uri: Uri = "/recover?token=secret".parse().unwrap();

        assert_eq!(trace_path(&uri), "/recover");
    }
}
