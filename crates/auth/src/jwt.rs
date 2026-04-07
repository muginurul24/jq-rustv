use chrono::{DateTime, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use justqiu_errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub sid: String,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    pub fn new(
        user_id: i64,
        role: impl Into<String>,
        sid: Uuid,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            sub: user_id.to_string(),
            role: role.into(),
            sid: sid.to_string(),
            iat: issued_at.timestamp(),
            exp: expires_at.timestamp(),
        }
    }

    pub fn user_id(&self) -> Result<i64, AppError> {
        self.sub.parse().map_err(|_| AppError::Unauthorized)
    }

    pub fn session_id(&self) -> Result<Uuid, AppError> {
        self.sid.parse().map_err(|_| AppError::Unauthorized)
    }
}

pub fn sign_jwt(claims: &Claims, secret: &str) -> Result<String, AppError> {
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|err| AppError::Internal(err.into()))
}

pub fn decode_jwt(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.required_spec_claims.insert("exp".to_string());

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn signs_and_decodes_hs256_jwt() {
        let issued_at = Utc::now() - Duration::minutes(5);
        let expires_at = issued_at + Duration::hours(8);
        let sid = Uuid::parse_str("09fced7e-aee5-4778-a912-6b99831d38e9").unwrap();
        let claims = Claims::new(42, "admin", sid, issued_at, expires_at);

        let token = sign_jwt(&claims, "test-secret").expect("jwt should sign");
        let decoded = decode_jwt(&token, "test-secret").expect("jwt should decode");

        assert_eq!(decoded, claims);
        assert_eq!(decoded.user_id().unwrap(), 42);
        assert_eq!(decoded.session_id().unwrap(), sid);
    }

    #[test]
    fn rejects_invalid_jwt() {
        let result = decode_jwt("definitely-not-a-jwt", "test-secret");

        assert!(matches!(result, Err(AppError::Unauthorized)));
    }
}
