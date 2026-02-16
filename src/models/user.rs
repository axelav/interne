use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub invite_code: String,
    pub created_at: String,
    pub updated_at: String,
}
