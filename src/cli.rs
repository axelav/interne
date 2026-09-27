use serde::{Deserialize, Deserializer};
use sqlx::SqlitePool;
use std::fs;
use url::Url;
use uuid::Uuid;

use crate::config::{AuthConfig, ConfigError, SignupMode};
use crate::connection_tokens::{connection_url, issue_invitation, reset_auth};
use crate::models::Interval;

// Custom deserializer to handle duration as either string or integer
fn deserialize_duration<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(i64),
    }

    match StringOrInt::deserialize(deserializer)? {
        StringOrInt::String(s) => Ok(s),
        StringOrInt::Int(i) => Ok(i.to_string()),
    }
}

#[derive(Deserialize)]
struct LegacyEntry {
    url: String,
    title: String,
    description: Option<String>,
    #[serde(deserialize_with = "deserialize_duration")]
    duration: String,
    interval: String,
    visited: Option<i64>,
    #[serde(rename = "id")]
    _id: String,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(rename = "dismissedAt")]
    dismissed_at: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

pub async fn import_data(
    pool: &SqlitePool,
    file_path: &str,
    user_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Verify user exists before importing
    let user_exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    if user_exists.0 == 0 {
        return Err(format!("User with ID '{}' not found", user_id).into());
    }

    let content = fs::read_to_string(file_path)?;
    let entries: Vec<LegacyEntry> = serde_json::from_str(&content)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut imported = 0;
    let mut tx = pool.begin().await?;

    for entry in entries {
        let id = Uuid::new_v4().to_string();
        let duration: i64 = entry.duration.parse().unwrap_or(1);
        let created_at = entry.created_at.unwrap_or_else(|| now.clone());
        let updated_at = entry.updated_at.unwrap_or_else(|| now.clone());

        let interval = match entry.interval.as_str() {
            "hours" => Interval::Hours,
            "days" => Interval::Days,
            "weeks" => Interval::Weeks,
            "months" => Interval::Months,
            "years" => Interval::Years,
            other => {
                eprintln!("Unknown interval: {other}, defaulting to days");
                Interval::Days
            }
        };

        sqlx::query(
            r#"
            INSERT INTO entries (id, user_id, url, title, description, duration, interval, dismissed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(user_id)
        .bind(&entry.url)
        .bind(&entry.title)
        .bind(&entry.description)
        .bind(duration)
        .bind(interval)
        .bind(&entry.dismissed_at)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&mut *tx)
        .await?;

        // Handle tags
        for tag_name in &entry.tags {
            let tag_name = tag_name.trim().to_lowercase();
            if tag_name.is_empty() {
                continue;
            }

            let tag_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(&tag_name)
                .fetch_optional(&mut *tx)
                .await?;

            let tag_id = match tag_id {
                Some((id,)) => id,
                None => {
                    let new_id = Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                        .bind(&new_id)
                        .bind(&tag_name)
                        .bind(&now)
                        .execute(&mut *tx)
                        .await?;
                    new_id
                }
            };

            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
                .bind(&id)
                .bind(&tag_id)
                .execute(&mut *tx)
                .await?;
        }

        // Create visit records for visited count
        if let Some(visited) = entry.visited {
            for _ in 0..visited {
                let visit_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO visits (id, entry_id, user_id, visited_at) VALUES (?, ?, ?, ?)",
                )
                .bind(&visit_id)
                .bind(&id)
                .bind(user_id)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
            }
        }

        imported += 1;
    }

    tx.commit().await?;
    println!("Imported {} entries", imported);
    Ok(())
}

pub async fn invite_user(
    pool: &SqlitePool,
    name: &str,
    public_base_url: &Url,
) -> Result<(String, Url), Box<dyn std::error::Error>> {
    let issued = issue_invitation(pool, name, chrono::Utc::now()).await?;
    let url = connection_url(public_base_url, &issued.plaintext_token);
    Ok((issued.user_id, url))
}

pub async fn reset_user_auth(
    pool: &SqlitePool,
    user_id: &str,
    public_base_url: &Url,
) -> Result<Url, Box<dyn std::error::Error>> {
    let issued = reset_auth(pool, user_id, chrono::Utc::now()).await?;
    Ok(connection_url(public_base_url, &issued.plaintext_token))
}

pub fn connection_base_url_from_lookup<F>(mut lookup: F) -> Result<Url, ConfigError>
where
    F: FnMut(&str) -> Option<String>,
{
    let public_base_url =
        lookup("PUBLIC_BASE_URL").ok_or(ConfigError::MissingVariable("PUBLIC_BASE_URL"))?;
    Ok(AuthConfig::new(&public_base_url, SignupMode::Closed)?.public_base_url)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::SqlitePool;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use url::Url;

    use super::{connection_base_url_from_lookup, invite_user, reset_user_auth};

    async fn migrated_pool() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn invite_user_returns_the_created_id_and_connection_url() {
        let pool = migrated_pool().await;
        let base = Url::parse("https://interne.test/").unwrap();

        let (user_id, url) = invite_user(&pool, "Guest", &base).await.unwrap();

        assert_eq!(url.path(), "/recover");
        assert!(url.query_pairs().any(|(key, _)| key == "token"));
        let user: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT name, email, invite_code, github_user_id FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(user, ("Guest".into(), None, None, None));
    }

    #[tokio::test]
    async fn reset_user_auth_returns_a_recovery_url() {
        let pool = migrated_pool().await;
        let base = Url::parse("https://interne.test/").unwrap();
        let (user_id, _) = invite_user(&pool, "Guest", &base).await.unwrap();

        let url = reset_user_auth(&pool, &user_id, &base).await.unwrap();

        assert_eq!(url.path(), "/recover");
        assert!(url.query_pairs().any(|(key, _)| key == "token"));
    }

    #[test]
    fn connection_commands_require_only_public_base_url() {
        let url = connection_base_url_from_lookup(|key| match key {
            "PUBLIC_BASE_URL" => Some("https://interne.test".into()),
            unexpected => panic!("must not read {unexpected}"),
        })
        .unwrap();

        assert_eq!(url.as_str(), "https://interne.test/");
    }
}
