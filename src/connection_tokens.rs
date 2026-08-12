use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqlitePool, Transaction};
use url::Url;
use uuid::Uuid;

const TOKEN_BYTES: usize = 32;
const TOKEN_LENGTH: usize = 43;
const TOKEN_LIFETIME_HOURS: i64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub struct ConnectionClaim {
    pub token_id: String,
    pub user_id: String,
    pub purpose: ConnectionPurpose,
}

pub enum ConnectionTokenError {
    InvalidToken,
    UserNotFound,
    Database(sqlx::Error),
}

impl fmt::Debug for ConnectionTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken => formatter.write_str("InvalidToken"),
            Self::UserNotFound => formatter.write_str("UserNotFound"),
            Self::Database(_) => formatter.write_str("Database"),
        }
    }
}

impl fmt::Display for ConnectionTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken => formatter.write_str("connection link is invalid or expired"),
            Self::UserNotFound => formatter.write_str("user not found"),
            Self::Database(_) => formatter.write_str("connection token database operation failed"),
        }
    }
}

impl std::error::Error for ConnectionTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidToken | Self::UserNotFound => None,
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
    let now_text = now.to_rfc3339();
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
    let now_text = now.to_rfc3339();
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
    .bind(now.to_rfc3339())
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
    let now_text = now.to_rfc3339();

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
    .bind(expires_at.to_rfc3339())
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
