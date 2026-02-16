use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Collection {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub invite_code: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Collection {
    pub fn new(owner_id: String, name: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_id,
            name,
            invite_code: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CollectionMember {
    pub collection_id: String,
    pub user_id: String,
    pub joined_at: String,
}

impl CollectionMember {
    pub fn new(collection_id: String, user_id: String) -> Self {
        Self {
            collection_id,
            user_id,
            joined_at: Utc::now().to_rfc3339(),
        }
    }
}
