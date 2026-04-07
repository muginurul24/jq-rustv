use anyhow::anyhow;
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use uuid::Uuid;

use justqiu_errors::AppError;

type HmacSha256 = Hmac<Sha256>;

pub fn generate_csrf_secret() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn derive_csrf_token(csrf_secret: &str, sid: Uuid) -> Result<String, AppError> {
    let secret_bytes = decode_secret(csrf_secret)?;
    let mut mac =
        HmacSha256::new_from_slice(&secret_bytes).map_err(|err| AppError::Internal(err.into()))?;

    mac.update(sid.to_string().as_bytes());

    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn verify_csrf_token(
    csrf_secret: &str,
    sid: Uuid,
    provided_token: &str,
) -> Result<bool, AppError> {
    let provided_bytes = match hex::decode(provided_token) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };

    if provided_bytes.len() != 32 {
        return Ok(false);
    }

    let secret_bytes = decode_secret(csrf_secret)?;
    let mut mac =
        HmacSha256::new_from_slice(&secret_bytes).map_err(|err| AppError::Internal(err.into()))?;
    mac.update(sid.to_string().as_bytes());

    Ok(mac.verify_slice(&provided_bytes).is_ok())
}

fn decode_secret(csrf_secret: &str) -> Result<Vec<u8>, AppError> {
    hex::decode(csrf_secret)
        .map_err(|_| AppError::Internal(anyhow!("csrf secret must be valid hex")))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn generates_hex_encoded_secret() {
        let secret = generate_csrf_secret();

        assert_eq!(secret.len(), 64);
        assert!(secret.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn derives_and_verifies_csrf_token() {
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let sid = Uuid::parse_str("09fced7e-aee5-4778-a912-6b99831d38e9").unwrap();

        let token = derive_csrf_token(secret, sid).expect("csrf token should derive");

        assert_eq!(token.len(), 64);
        assert!(verify_csrf_token(secret, sid, &token).unwrap());
    }

    #[test]
    fn rejects_invalid_or_mismatched_tokens() {
        let secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let sid = Uuid::parse_str("09fced7e-aee5-4778-a912-6b99831d38e9").unwrap();
        let other_sid = Uuid::parse_str("e5dc01b0-6300-4634-8b28-f022fa723638").unwrap();
        let token = derive_csrf_token(secret, sid).unwrap();

        assert!(!verify_csrf_token(secret, sid, "not-hex").unwrap());
        assert!(!verify_csrf_token(secret, other_sid, &token).unwrap());
    }

    #[test]
    fn rejects_invalid_secret_hex() {
        let sid = Uuid::parse_str("09fced7e-aee5-4778-a912-6b99831d38e9").unwrap();

        assert!(matches!(
            derive_csrf_token("invalid-secret", sid),
            Err(AppError::Internal(_))
        ));
    }
}
