use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};
use url::Url;
use uuid::Uuid;

use crate::github::GitHubProfile;
use crate::models::User;

const TOKEN_BYTES: usize = 32;
const TOKEN_LENGTH: usize = 43;
const TOKEN_LIFETIME_HOURS: i64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionPurpose {
    Invite,
    Recovery,
}

impl ConnectionPurpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Invite => "invite",
            Self::Recovery => "recovery",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "invite" => Some(Self::Invite),
            "recovery" => Some(Self::Recovery),
            _ => None,
        }
    }
}

pub struct IssuedConnection {
    pub user_id: String,
    pub plaintext_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionClaim {
    pub token_id: String,
    pub user_id: String,
    pub purpose: ConnectionPurpose,
}

pub enum ConnectionTokenError {
    InvalidToken,
    GitHubIdentityInUse,
    UserNotFound,
    Database(sqlx::Error),
}

impl fmt::Debug for ConnectionTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken => formatter.write_str("InvalidToken"),
            Self::GitHubIdentityInUse => formatter.write_str("GitHubIdentityInUse"),
            Self::UserNotFound => formatter.write_str("UserNotFound"),
            Self::Database(_) => formatter.write_str("Database"),
        }
    }
}

impl fmt::Display for ConnectionTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken => formatter.write_str("connection link is invalid or expired"),
            Self::GitHubIdentityInUse => {
                formatter.write_str("GitHub identity is already connected")
            }
            Self::UserNotFound => formatter.write_str("user not found"),
            Self::Database(_) => formatter.write_str("connection token database operation failed"),
        }
    }
}

impl std::error::Error for ConnectionTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::GitHubIdentityInUse | Self::InvalidToken | Self::UserNotFound => None,
        }
    }
}

impl From<sqlx::Error> for ConnectionTokenError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub async fn issue_invitation(
    pool: &SqlitePool,
    name: &str,
    now: DateTime<Utc>,
) -> Result<IssuedConnection, ConnectionTokenError> {
    let user_id = Uuid::new_v4().to_string();
    let now_text = database_timestamp(now);
    let mut transaction = pool.begin().await?;

    sqlx::query(
        "INSERT INTO users (id, name, email, invite_code, github_user_id, github_login, created_at, updated_at) \
         VALUES (?, ?, NULL, NULL, NULL, NULL, ?, ?)",
    )
    .bind(&user_id)
    .bind(name)
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await?;

    let issued = issue_token(&mut transaction, &user_id, ConnectionPurpose::Invite, now).await?;
    transaction.commit().await?;
    Ok(issued)
}

pub async fn reset_auth(
    pool: &SqlitePool,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<IssuedConnection, ConnectionTokenError> {
    let now_text = database_timestamp(now);
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE users SET github_user_id = NULL, github_login = NULL, invite_code = NULL, \
         auth_version = auth_version + 1, updated_at = ? WHERE id = ?",
    )
    .bind(&now_text)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ConnectionTokenError::UserNotFound);
    }

    let issued = issue_token(&mut transaction, user_id, ConnectionPurpose::Recovery, now).await?;
    transaction.commit().await?;
    Ok(issued)
}

pub async fn validate_token(
    pool: &SqlitePool,
    plaintext: &str,
    now: DateTime<Utc>,
) -> Result<ConnectionClaim, ConnectionTokenError> {
    if plaintext.len() != TOKEN_LENGTH {
        return Err(ConnectionTokenError::InvalidToken);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(plaintext)
        .map_err(|_| ConnectionTokenError::InvalidToken)?;
    if decoded.len() != TOKEN_BYTES || URL_SAFE_NO_PAD.encode(&decoded) != plaintext {
        return Err(ConnectionTokenError::InvalidToken);
    }

    let token_hash = hash_token(plaintext);
    let record: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, user_id, purpose FROM auth_connection_tokens \
         WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
    )
    .bind(token_hash)
    .bind(database_timestamp(now))
    .fetch_optional(pool)
    .await?;

    let (token_id, user_id, purpose) = record.ok_or(ConnectionTokenError::InvalidToken)?;
    let purpose =
        ConnectionPurpose::from_str(&purpose).ok_or(ConnectionTokenError::InvalidToken)?;
    Ok(ConnectionClaim {
        token_id,
        user_id,
        purpose,
    })
}

pub async fn validate_claim(
    pool: &SqlitePool,
    claim: &ConnectionClaim,
    now: DateTime<Utc>,
) -> Result<(), ConnectionTokenError> {
    let is_active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(\
             SELECT 1 FROM auth_connection_tokens \
             WHERE id = ? AND user_id = ? AND purpose = ? \
               AND consumed_at IS NULL AND expires_at > ?\
         )",
    )
    .bind(&claim.token_id)
    .bind(&claim.user_id)
    .bind(claim.purpose.as_str())
    .bind(database_timestamp(now))
    .fetch_one(pool)
    .await?;

    if is_active == 1 {
        Ok(())
    } else {
        Err(ConnectionTokenError::InvalidToken)
    }
}

pub async fn consume_and_link(
    pool: &SqlitePool,
    claim: &ConnectionClaim,
    profile: &GitHubProfile,
    now: DateTime<Utc>,
) -> Result<User, ConnectionTokenError> {
    let now_text = database_timestamp(now);
    let mut transaction = pool.begin().await?;

    let target_token = sqlx::query(
        "UPDATE auth_connection_tokens SET consumed_at = ? \
         WHERE id = ? AND user_id = ? AND purpose = ? \
           AND consumed_at IS NULL AND expires_at > ?",
    )
    .bind(&now_text)
    .bind(&claim.token_id)
    .bind(&claim.user_id)
    .bind(claim.purpose.as_str())
    .bind(&now_text)
    .execute(&mut *transaction)
    .await?;
    if target_token.rows_affected() != 1 {
        return Err(ConnectionTokenError::InvalidToken);
    }

    let owner: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE github_user_id = ?")
        .bind(&profile.user_id)
        .fetch_optional(&mut *transaction)
        .await?;
    if owner.is_some_and(|owner_id| owner_id != claim.user_id) {
        return Err(ConnectionTokenError::GitHubIdentityInUse);
    }

    let user_update = sqlx::query(
        "UPDATE users \
         SET github_user_id = ?, github_login = ?, invite_code = NULL, \
             auth_version = auth_version + 1, updated_at = ? \
         WHERE id = ? AND github_user_id IS NULL",
    )
    .bind(&profile.user_id)
    .bind(&profile.login)
    .bind(&now_text)
    .bind(&claim.user_id)
    .execute(&mut *transaction)
    .await;
    let user_update = match user_update {
        Ok(update) => update,
        Err(error) if is_unique_violation(&error) => {
            return Err(ConnectionTokenError::GitHubIdentityInUse);
        }
        Err(error) => return Err(error.into()),
    };
    if user_update.rows_affected() != 1 {
        return Err(ConnectionTokenError::InvalidToken);
    }

    sqlx::query(
        "UPDATE auth_connection_tokens SET consumed_at = ? \
         WHERE user_id = ? AND consumed_at IS NULL AND expires_at > ?",
    )
    .bind(&now_text)
    .bind(&claim.user_id)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await?;

    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&claim.user_id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(user)
}

pub fn connection_url(base: &Url, plaintext: &str) -> Url {
    let mut url = base.clone();
    url.set_path("/recover");
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut().append_pair("token", plaintext);
    url
}

async fn issue_token(
    transaction: &mut Transaction<'_, Sqlite>,
    user_id: &str,
    purpose: ConnectionPurpose,
    now: DateTime<Utc>,
) -> Result<IssuedConnection, ConnectionTokenError> {
    let mut random = [0_u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut random);
    let plaintext_token = URL_SAFE_NO_PAD.encode(random);
    let token_hash = hash_token(&plaintext_token);
    let expires_at = now + Duration::hours(TOKEN_LIFETIME_HOURS);
    let now_text = database_timestamp(now);

    sqlx::query(
        "UPDATE auth_connection_tokens SET consumed_at = ? \
         WHERE user_id = ? AND consumed_at IS NULL AND expires_at > ?",
    )
    .bind(&now_text)
    .bind(user_id)
    .bind(&now_text)
    .execute(&mut **transaction)
    .await?;

    sqlx::query(
        "INSERT INTO auth_connection_tokens \
         (id, user_id, token_hash, purpose, created_at, expires_at, consumed_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(token_hash)
    .bind(purpose.as_str())
    .bind(&now_text)
    .bind(database_timestamp(expires_at))
    .execute(&mut **transaction)
    .await?;

    Ok(IssuedConnection {
        user_id: user_id.to_owned(),
        plaintext_token,
        expires_at,
    })
}

fn hash_token(plaintext: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(plaintext.as_bytes()))
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
}

pub(crate) fn database_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true)
}
