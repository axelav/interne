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
use crate::connection_tokens::{ConnectionClaim, ConnectionPurpose};
use crate::github::GitHubProfile;
use crate::models::User;

const USER_ID_KEY: &str = "user_id";
const AUTH_VERSION_KEY: &str = "auth_version";
const OAUTH_ATTEMPT_KEY: &str = "oauth_attempt";
const MIGRATION_SESSION_KEY: &str = "migration_session";
const PENDING_CONNECTION_KEY: &str = "pending_connection";
const CONNECTION_CLAIM_KEY: &str = "connection_claim";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthPurpose {
    Login,
    Link {
        user_id: String,
    },
    ConnectionToken {
        token_id: String,
        user_id: String,
        purpose: ConnectionPurpose,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthAttempt {
    pub state: String,
    pub pkce_verifier: String,
    pub purpose: OAuthPurpose,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationSession {
    pub user_id: String,
    pub expires_at: i64,
}

impl MigrationSession {
    pub fn new(user_id: String, now: DateTime<Utc>) -> Self {
        Self {
            user_id,
            expires_at: now.timestamp() + 1_800,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConnectionProof {
    LegacyInvite,
    Token {
        token_id: String,
        purpose: ConnectionPurpose,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingConnection {
    pub user_id: String,
    pub github_profile: GitHubProfile,
    pub proof: ConnectionProof,
    pub expires_at: i64,
}

impl PendingConnection {
    pub fn new(
        user_id: String,
        github_profile: GitHubProfile,
        proof: ConnectionProof,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            github_profile,
            proof,
            expires_at: now.timestamp() + 600,
        }
    }
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

pub async fn store_migration_session(
    session: &Session,
    migration: &MigrationSession,
) -> Result<(), tower_sessions::session::Error> {
    session.remove::<String>(USER_ID_KEY).await?;
    session.remove::<i64>(AUTH_VERSION_KEY).await?;
    session
        .remove::<PendingConnection>(PENDING_CONNECTION_KEY)
        .await?;
    session.insert(MIGRATION_SESSION_KEY, migration).await
}

pub async fn get_migration_session(
    session: &Session,
) -> Result<Option<MigrationSession>, tower_sessions::session::Error> {
    session.get(MIGRATION_SESSION_KEY).await
}

pub async fn take_migration_session(
    session: &Session,
) -> Result<Option<MigrationSession>, tower_sessions::session::Error> {
    session.remove(MIGRATION_SESSION_KEY).await
}

pub async fn store_pending_connection(
    session: &Session,
    pending: &PendingConnection,
) -> Result<(), tower_sessions::session::Error> {
    session.remove::<String>(USER_ID_KEY).await?;
    session.remove::<i64>(AUTH_VERSION_KEY).await?;
    session.insert(PENDING_CONNECTION_KEY, pending).await
}

pub async fn store_connection_claim(
    session: &Session,
    claim: &ConnectionClaim,
) -> Result<(), tower_sessions::session::Error> {
    session.remove::<String>(USER_ID_KEY).await?;
    session.remove::<i64>(AUTH_VERSION_KEY).await?;
    session
        .remove::<MigrationSession>(MIGRATION_SESSION_KEY)
        .await?;
    session
        .remove::<PendingConnection>(PENDING_CONNECTION_KEY)
        .await?;
    session.remove::<OAuthAttempt>(OAUTH_ATTEMPT_KEY).await?;
    session.insert(CONNECTION_CLAIM_KEY, claim).await
}

pub async fn take_connection_claim(
    session: &Session,
) -> Result<Option<ConnectionClaim>, tower_sessions::session::Error> {
    session.remove(CONNECTION_CLAIM_KEY).await
}

pub async fn get_pending_connection(
    session: &Session,
) -> Result<Option<PendingConnection>, tower_sessions::session::Error> {
    session.get(PENDING_CONNECTION_KEY).await
}

pub async fn take_pending_connection(
    session: &Session,
) -> Result<Option<PendingConnection>, tower_sessions::session::Error> {
    session.remove(PENDING_CONNECTION_KEY).await
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

        let migration: Option<MigrationSession> = session
            .get(MIGRATION_SESSION_KEY)
            .await
            .map_err(|_| AuthRedirect)?;
        let pending: Option<PendingConnection> = session
            .get(PENDING_CONNECTION_KEY)
            .await
            .map_err(|_| AuthRedirect)?;
        if migration.is_some() || pending.is_some() {
            return Err(AuthRedirect);
        }

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
    session
        .remove::<MigrationSession>(MIGRATION_SESSION_KEY)
        .await?;
    session
        .remove::<PendingConnection>(PENDING_CONNECTION_KEY)
        .await?;
    session
        .remove::<ConnectionClaim>(CONNECTION_CLAIM_KEY)
        .await?;
    session.insert(USER_ID_KEY, &user.id).await?;
    session.insert(AUTH_VERSION_KEY, user.auth_version).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::{ConnectionProof, MigrationSession, PendingConnection, session_identity};
    use crate::github::GitHubProfile;

    #[test]
    fn missing_auth_version_rejects_a_known_user_id() {
        let identity = session_identity(Some("known-user".into()), None);

        assert_eq!(identity, None);
    }

    #[test]
    fn migration_sessions_expire_after_thirty_minutes() {
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();

        let migration = MigrationSession::new("legacy-user".into(), now);

        assert_eq!(migration.expires_at, 1_001_800);
    }

    #[test]
    fn pending_connections_expire_after_ten_minutes() {
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let profile = GitHubProfile {
            user_id: "123".into(),
            login: "octocat".into(),
            name: None,
        };

        let pending = PendingConnection::new(
            "legacy-user".into(),
            profile,
            ConnectionProof::LegacyInvite,
            now,
        );

        assert_eq!(pending.expires_at, 1_000_600);
    }
}
