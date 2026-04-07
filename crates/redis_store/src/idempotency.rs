use crate::{RedisStoreError, RedisStoreResult};

pub fn idempotency_key(kind: &str, reference: &str) -> RedisStoreResult<String> {
    let kind = kind.trim();
    let reference = reference.trim();

    if kind.is_empty() || reference.is_empty() {
        return Err(RedisStoreError::InvalidKeyPart);
    }

    Ok(format!("idempotency:{kind}:{reference}"))
}

pub async fn acquire_idempotency(
    client: &redis::Client,
    key: &str,
    ttl_seconds: usize,
) -> RedisStoreResult<bool> {
    if ttl_seconds == 0 {
        return Err(RedisStoreError::InvalidTtl);
    }

    let mut connection = client.get_multiplexed_async_connection().await?;
    let response: Option<String> = redis::cmd("SET")
        .arg(key)
        .arg("1")
        .arg("EX")
        .arg(ttl_seconds)
        .arg("NX")
        .query_async(&mut connection)
        .await?;

    Ok(response.as_deref() == Some("OK"))
}

pub async fn idempotency_exists(client: &redis::Client, key: &str) -> RedisStoreResult<bool> {
    let mut connection = client.get_multiplexed_async_connection().await?;
    let exists: bool = redis::cmd("EXISTS")
        .arg(key)
        .query_async(&mut connection)
        .await?;

    Ok(exists)
}

pub async fn release_idempotency(client: &redis::Client, key: &str) -> RedisStoreResult<()> {
    let mut connection = client.get_multiplexed_async_connection().await?;
    let _: i64 = redis::cmd("DEL")
        .arg(key)
        .query_async(&mut connection)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{idempotency_key, RedisStoreError};

    #[test]
    fn idempotency_key_formats_expected_pattern() {
        let key = idempotency_key("webhook:qris", "trx-123").unwrap();

        assert_eq!(key, "idempotency:webhook:qris:trx-123");
    }

    #[test]
    fn idempotency_key_rejects_empty_parts() {
        let error = idempotency_key("  ", "trx-123").unwrap_err();

        assert!(matches!(error, RedisStoreError::InvalidKeyPart));
    }
}
