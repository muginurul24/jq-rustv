use redis::AsyncCommands;

use crate::{RedisJsonValue, RedisStoreError, RedisStoreResult};

pub fn captcha_key(captcha_id: &str) -> String {
    format!("captcha:{captcha_id}")
}

pub async fn put_captcha<T>(
    client: &redis::Client,
    captcha_id: &str,
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

    let _: () = connection
        .set_ex(captcha_key(captcha_id), payload, ttl_seconds)
        .await?;

    Ok(())
}

pub async fn get_del_captcha<T>(
    client: &redis::Client,
    captcha_id: &str,
) -> RedisStoreResult<Option<T>>
where
    T: RedisJsonValue,
{
    let mut connection = client.get_connection_manager().await?;
    let payload: Option<String> = redis::cmd("GETDEL")
        .arg(captcha_key(captcha_id))
        .query_async(&mut connection)
        .await?;

    payload
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_id_with_captcha_namespace() {
        assert_eq!(
            captcha_key("09fced7e-aee5-4778-a912-6b99831d38e9"),
            "captcha:09fced7e-aee5-4778-a912-6b99831d38e9"
        );
    }
}
