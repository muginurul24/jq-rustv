use redis::AsyncCommands;

use crate::{RedisJsonValue, RedisStoreError, RedisStoreResult};

pub fn session_key(sid: &str) -> String {
    format!("session:{sid}")
}

pub async fn put_session<T>(
    client: &redis::Client,
    sid: &str,
    data: &T,
    ttl_seconds: u64,
) -> RedisStoreResult<()>
where
    T: RedisJsonValue,
{
    if ttl_seconds == 0 {
        return Err(RedisStoreError::InvalidTtl);
    }

    let payload = serde_json::to_string(data)?;
    let mut connection = client.get_connection_manager().await?;
    let key = session_key(sid);

    let _: () = connection.set_ex(key, payload, ttl_seconds).await?;

    Ok(())
}

pub async fn get_session<T>(client: &redis::Client, sid: &str) -> RedisStoreResult<Option<T>>
where
    T: RedisJsonValue,
{
    let mut connection = client.get_connection_manager().await?;
    let key = session_key(sid);
    let payload: Option<String> = connection.get(key).await?;

    payload
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(Into::into)
}

pub async fn delete_session(client: &redis::Client, sid: &str) -> RedisStoreResult<()> {
    let mut connection = client.get_connection_manager().await?;
    let key = session_key(sid);

    let _: usize = connection.del(key).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct DummySession {
        user_id: i64,
        role: String,
    }

    #[test]
    fn prefixes_sid_with_session_namespace() {
        assert_eq!(
            session_key("09fced7e-aee5-4778-a912-6b99831d38e9"),
            "session:09fced7e-aee5-4778-a912-6b99831d38e9"
        );
    }

    #[test]
    fn dummy_session_implements_json_value_contract() {
        fn assert_json_value<T: RedisJsonValue>() {}

        assert_json_value::<DummySession>();
    }
}
