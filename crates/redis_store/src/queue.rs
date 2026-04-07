use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};

use crate::{RedisStoreError, RedisStoreResult};

pub fn job_queue_key(queue_name: &str) -> RedisStoreResult<String> {
    let queue_name = queue_name.trim();
    if queue_name.is_empty() {
        return Err(RedisStoreError::InvalidQueueName);
    }

    Ok(format!("queue:jobs:{queue_name}"))
}

pub async fn enqueue_json<T: Serialize>(
    client: &redis::Client,
    queue_name: &str,
    payload: &T,
) -> RedisStoreResult<()> {
    let key = job_queue_key(queue_name)?;
    let body = serde_json::to_string(payload)?;
    let mut connection = client.get_multiplexed_async_connection().await?;
    let _: usize = connection.rpush(key, body).await?;
    Ok(())
}

pub async fn dequeue_json<T: DeserializeOwned>(
    client: &redis::Client,
    queue_name: &str,
) -> RedisStoreResult<Option<T>> {
    let key = job_queue_key(queue_name)?;
    let mut connection = client.get_multiplexed_async_connection().await?;
    let body: Option<String> = connection.lpop(key, None).await?;

    match body {
        Some(body) => Ok(Some(serde_json::from_str(&body)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use crate::{job_queue_key, RedisStoreError};

    #[test]
    fn job_queue_key_formats_expected_pattern() {
        let key = job_queue_key("process_qris").unwrap();

        assert_eq!(key, "queue:jobs:process_qris");
    }

    #[test]
    fn job_queue_key_rejects_empty_queue_name() {
        let error = job_queue_key("  ").unwrap_err();

        assert!(matches!(error, RedisStoreError::InvalidQueueName));
    }
}
