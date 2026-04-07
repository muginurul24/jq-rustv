use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Ledger row. Amount is i64 (integer rupiah). No floats.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: i64,
    pub toko_id: i64,
    pub player: Option<String>,
    /// Internal upstream player identity. NEVER expose in public responses.
    #[serde(skip_serializing)]
    pub external_player: Option<String>,
    pub category: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub tx_type: String,
    pub status: String,
    pub amount: i64,
    pub code: Option<String>,
    pub note: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}
