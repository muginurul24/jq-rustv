pub mod captcha;
pub mod idempotency;
pub mod queue;
pub mod rate_limit;
pub mod session;

use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum RedisStoreError {
    #[error("Redis error")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error")]
    Serialization(#[from] serde_json::Error),

    #[error("TTL must be greater than 0")]
    InvalidTtl,

    #[error("Limit must be greater than 0")]
    InvalidLimit,

    #[error("Key part must not be empty")]
    InvalidKeyPart,

    #[error("Queue name must not be empty")]
    InvalidQueueName,
}

pub type RedisStoreResult<T> = Result<T, RedisStoreError>;

pub trait RedisJsonValue: Serialize + DeserializeOwned + Send + Sync {}

impl<T> RedisJsonValue for T where T: Serialize + DeserializeOwned + Send + Sync {}

pub use captcha::{captcha_key, get_del_captcha, put_captcha};
pub use idempotency::{
    acquire_idempotency, idempotency_exists, idempotency_key, release_idempotency,
};
pub use queue::{dequeue_json, enqueue_json, job_queue_key};
pub use rate_limit::{check_rate_limit, rate_limit_key, RateLimitDecision};
pub use session::{delete_session, get_session, put_session, session_key};
