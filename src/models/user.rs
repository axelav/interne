use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub invite_code: String,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    pub fn new(name: String, email: Option<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email,
            invite_code: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
