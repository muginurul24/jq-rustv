use anyhow::anyhow;
use chrono::{Duration, Utc};
use justqiu_redis::{
    delete_session as delete_session_record, get_session as get_session_record,
    put_session as put_session_record,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use justqiu_errors::AppError;

use crate::csrf::generate_csrf_secret;
use crate::jwt::{sign_jwt, Claims};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionData {
    pub user_id: i64,
    pub role: String,
    pub csrf_secret: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub ip_hash: String,
    pub ua_hash: String,
}

impl SessionData {
    pub fn ttl_seconds(&self) -> i64 {
        self.expires_at - self.issued_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBundle {
    pub sid: Uuid,
    pub claims: Claims,
    pub data: SessionData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedSession {
    pub jwt: String,
    pub bundle: SessionBundle,
}

pub fn create_session(
    user_id: i64,
    role: impl Into<String>,
    client_ip: &str,
    user_agent: &str,
    expiry_hours: u64,
) -> Result<SessionBundle, AppError> {
    if expiry_hours == 0 {
        return Err(AppError::Internal(anyhow!(
            "JWT_EXPIRY_HOURS must be greater than 0"
        )));
    }

    let sid = Uuid::new_v4();
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::hours(expiry_hours as i64);
    let role = role.into();

    let data = SessionData {
        user_id,
        role: role.clone(),
        csrf_secret: generate_csrf_secret(),
        issued_at: issued_at.timestamp(),
        expires_at: expires_at.timestamp(),
        ip_hash: truncate_sha256(client_ip),
        ua_hash: truncate_sha256(user_agent),
    };

    let claims = Claims::new(user_id, role, sid, issued_at, expires_at);

    Ok(SessionBundle { sid, claims, data })
}

pub async fn issue_session(
    client: &redis::Client,
    jwt_secret: &str,
    user_id: i64,
    role: impl Into<String>,
    client_ip: &str,
    user_agent: &str,
    expiry_hours: u64,
) -> Result<IssuedSession, AppError> {
    let bundle = create_session(user_id, role, client_ip, user_agent, expiry_hours)?;
    let jwt = sign_jwt(&bundle.claims, jwt_secret)?;

    store_session(client, &bundle).await?;

    Ok(IssuedSession { jwt, bundle })
}

pub async fn store_session(client: &redis::Client, bundle: &SessionBundle) -> Result<(), AppError> {
    let ttl_seconds = u64::try_from(bundle.data.ttl_seconds())
        .map_err(|_| AppError::Internal(anyhow!("session TTL must be positive")))?;

    put_session_record(client, &bundle.sid.to_string(), &bundle.data, ttl_seconds)
        .await
        .map_err(|err| AppError::Internal(err.into()))
}

pub async fn get_session(
    client: &redis::Client,
    sid: Uuid,
) -> Result<Option<SessionData>, AppError> {
    get_session_record(client, &sid.to_string())
        .await
        .map_err(|err| AppError::Internal(err.into()))
}

pub async fn delete_session(client: &redis::Client, sid: Uuid) -> Result<(), AppError> {
    delete_session_record(client, &sid.to_string())
        .await
        .map_err(|err| AppError::Internal(err.into()))
}

pub fn session_key(sid: Uuid) -> String {
    format!("session:{sid}")
}

fn truncate_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_session_bundle_with_matching_claims() {
        let bundle = create_session(7, "admin", "127.0.0.1", "Firefox/1.0", 8)
            .expect("session should be created");

        assert_eq!(bundle.data.user_id, 7);
        assert_eq!(bundle.data.role, "admin");
        assert_eq!(bundle.claims.sub, "7");
        assert_eq!(bundle.claims.role, "admin");
        assert_eq!(bundle.claims.sid, bundle.sid.to_string());
        assert_eq!(bundle.claims.iat, bundle.data.issued_at);
        assert_eq!(bundle.claims.exp, bundle.data.expires_at);
        assert_eq!(bundle.data.csrf_secret.len(), 64);
        assert_eq!(bundle.data.ip_hash.len(), 16);
        assert_eq!(bundle.data.ua_hash.len(), 16);
        assert_eq!(bundle.data.ttl_seconds(), 8 * 60 * 60);
    }

    #[test]
    fn builds_redis_session_key_from_sid() {
        let sid = Uuid::parse_str("09fced7e-aee5-4778-a912-6b99831d38e9").unwrap();

        assert_eq!(
            session_key(sid),
            "session:09fced7e-aee5-4778-a912-6b99831d38e9"
        );
    }

    #[test]
    fn rejects_zero_hour_expiry() {
        let result = create_session(7, "admin", "127.0.0.1", "Firefox/1.0", 0);

        assert!(matches!(result, Err(AppError::Internal(_))));
    }
}
