use serde::{Deserialize, Deserializer};
use sqlx::SqlitePool;
use std::fs;
use uuid::Uuid;

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

pub async fn import_data(pool: &SqlitePool, file_path: &str, user_id: &str) -> Result<(), Box<dyn std::error::Error>> {
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
        .bind(&interval)
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
                    "INSERT INTO visits (id, entry_id, user_id, visited_at) VALUES (?, ?, ?, ?)"
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

pub async fn create_user(pool: &SqlitePool, name: &str, email: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4().to_string();
    let invite_code = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO users (id, name, email, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(name)
    .bind(email)
    .bind(&invite_code)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    println!("Created user:");
    println!("  ID: {}", id);
    println!("  Name: {}", name);
    println!("  Invite Code: {}", invite_code);

    Ok(())
}
