use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Platform fee configuration and accumulated income.
/// All money fields are i64 (integer rupiah). No floats.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Income {
    pub id: i64,
    /// GGR conversion ratio for NexusGGR topup.
    pub ggr: i64,
    /// Percentage fee for QRIS deposit (integer, e.g. 3 = 3%).
    pub fee_transaction: i64,
    /// Percentage fee for withdrawal (integer, e.g. 2 = 2%).
    pub fee_withdrawal: i64,
    /// Accumulated platform income (integer rupiah).
    pub amount: i64,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}
