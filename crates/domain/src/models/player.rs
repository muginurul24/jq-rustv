use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Player {
    pub id: i64,
    pub toko_id: i64,
    pub username: String,
    /// Internal upstream username (ULID). NEVER expose to toko in API responses.
    #[serde(skip_serializing)]
    pub ext_username: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}
