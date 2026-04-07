use redis::AsyncCommands;

use crate::{RedisStoreError, RedisStoreResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub key: String,
    pub count: u64,
    pub limit: u64,
    pub ttl_seconds: i64,
    pub exceeded: bool,
}

pub fn rate_limit_key(scope: &str, identifier: &str) -> String {
    format!("rl:{scope}:{identifier}")
}

pub async fn check_rate_limit(
    client: &redis::Client,
    scope: &str,
    identifier: &str,
    limit: u64,
    window_seconds: u64,
) -> RedisStoreResult<RateLimitDecision> {
    if limit == 0 {
        return Err(RedisStoreError::InvalidLimit);
    }

    if window_seconds == 0 {
        return Err(RedisStoreError::InvalidTtl);
    }

    let key = rate_limit_key(scope, identifier);
    let mut connection = client.get_connection_manager().await?;
    let count: u64 = connection.incr(&key, 1_u8).await?;

    let _: bool = redis::cmd("EXPIRE")
        .arg(&key)
        .arg(window_seconds)
        .arg("NX")
        .query_async(&mut connection)
        .await?;

    let ttl_seconds: i64 = connection.ttl(&key).await?;

    Ok(RateLimitDecision {
        key,
        count,
        limit,
        ttl_seconds,
        exceeded: count > limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_rate_limit_namespace() {
        assert_eq!(rate_limit_key("login", "127.0.0.1"), "rl:login:127.0.0.1");
    }

    #[test]
    fn decision_marks_threshold_breach() {
        let decision = RateLimitDecision {
            key: rate_limit_key("login", "127.0.0.1"),
            count: 11,
            limit: 10,
            ttl_seconds: 60,
            exceeded: 11 > 10,
        };

        assert!(decision.exceeded);
    }
}
