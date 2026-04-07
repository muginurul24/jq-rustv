use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// All money fields are i64 (integer rupiah). No floats allowed.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Balance {
    pub id: i64,
    pub toko_id: i64,
    pub pending: i64,
    pub settle: i64,
    pub nexusggr: i64,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
