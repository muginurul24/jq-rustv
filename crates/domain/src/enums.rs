use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Dev,
    Superadmin,
    Admin,
    User,
}

impl Role {
    pub fn can_see_all(&self) -> bool {
        matches!(self, Role::Dev | Role::Superadmin)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Dev => write!(f, "dev"),
            Role::Superadmin => write!(f, "superadmin"),
            Role::Admin => write!(f, "admin"),
            Role::User => write!(f, "user"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" => Ok(Role::Dev),
            "superadmin" => Ok(Role::Superadmin),
            "admin" => Ok(Role::Admin),
            "user" => Ok(Role::User),
            other => Err(format!("unknown role: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TransactionCategory {
    Qris,
    Nexusggr,
}

impl fmt::Display for TransactionCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionCategory::Qris => write!(f, "qris"),
            TransactionCategory::Nexusggr => write!(f, "nexusggr"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
}

impl fmt::Display for TransactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionType::Deposit => write!(f, "deposit"),
            TransactionType::Withdrawal => write!(f, "withdrawal"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "VARCHAR", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    Pending,
    Success,
    Failed,
    Expired,
}

impl TransactionStatus {
    /// Returns true if this status can transition to `target`.
    pub fn can_transition_to(&self, target: TransactionStatus) -> bool {
        matches!(
            (self, target),
            (TransactionStatus::Pending, TransactionStatus::Success)
                | (TransactionStatus::Pending, TransactionStatus::Failed)
                | (TransactionStatus::Pending, TransactionStatus::Expired)
        )
    }
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionStatus::Pending => write!(f, "pending"),
            TransactionStatus::Success => write!(f, "success"),
            TransactionStatus::Failed => write!(f, "failed"),
            TransactionStatus::Expired => write!(f, "expired"),
        }
    }
}
