use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

use crate::AppState;
use crate::models::User;

const USER_ID_KEY: &str = "user_id";
const AUTH_VERSION_KEY: &str = "auth_version";
const OAUTH_ATTEMPT_KEY: &str = "oauth_attempt";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthPurpose {
    Login,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthAttempt {
    pub state: String,
    pub pkce_verifier: String,
    pub purpose: OAuthPurpose,
    pub expires_at: i64,
}

impl OAuthAttempt {
    pub fn new(purpose: OAuthPurpose, now: DateTime<Utc>) -> Self {
        let mut state = [0_u8; 32];
        let mut pkce_verifier = [0_u8; 32];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut state);
        rng.fill_bytes(&mut pkce_verifier);

        Self {
            state: URL_SAFE_NO_PAD.encode(state),
            pkce_verifier: URL_SAFE_NO_PAD.encode(pkce_verifier),
            purpose,
            expires_at: now.timestamp() + 600,
        }
    }

    pub fn pkce_challenge(&self) -> String {
        crate::github::pkce_challenge(&self.pkce_verifier)
    }
}

pub async fn store_oauth_attempt(
    session: &Session,
    attempt: &OAuthAttempt,
) -> Result<(), tower_sessions::session::Error> {
    session.insert(OAUTH_ATTEMPT_KEY, attempt).await
}

pub async fn take_oauth_attempt(
    session: &Session,
) -> Result<Option<OAuthAttempt>, tower_sessions::session::Error> {
    session.remove(OAUTH_ATTEMPT_KEY).await
}

fn session_identity(user_id: Option<String>, auth_version: Option<i64>) -> Option<(String, i64)> {
    Some((user_id?, auth_version?))
}

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRedirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user_id: Option<String> = session.get(USER_ID_KEY).await.ok().flatten();
        let auth_version: Option<i64> = session.get(AUTH_VERSION_KEY).await.ok().flatten();

        let Some((user_id, auth_version)) = session_identity(user_id, auth_version) else {
            return Err(AuthRedirect);
        };

        let user: Option<User> =
            sqlx::query_as("SELECT * FROM users WHERE id = ? AND auth_version = ?")
                .bind(&user_id)
                .bind(auth_version)
                .fetch_optional(&state.db)
                .await
                .map_err(|_| AuthRedirect)?;

        user.map(AuthUser).ok_or(AuthRedirect)
    }
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/login").into_response()
    }
}

pub async fn login_user(
    session: &Session,
    user: &User,
) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, &user.id).await?;
    session.insert(AUTH_VERSION_KEY, user.auth_version).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}

#[cfg(test)]
mod tests {
    use super::session_identity;

    #[test]
    fn missing_auth_version_rejects_a_known_user_id() {
        let identity = session_identity(Some("known-user".into()), None);

        assert_eq!(identity, None);
    }
}
