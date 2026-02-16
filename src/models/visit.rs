use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Visit {
    pub id: String,
    pub entry_id: String,
    pub user_id: String,
    pub visited_at: String,
}

impl Visit {
    pub fn new(entry_id: String, user_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            entry_id,
            user_id,
            visited_at: Utc::now().to_rfc3339(),
        }
    }
}
