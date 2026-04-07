use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

pub fn verify_password(hash: &str, plain: &str) -> bool {
    if hash.starts_with("$argon2") {
        return verify_argon2(hash, plain);
    }

    if hash.starts_with("$2") {
        return verify_bcrypt(hash, plain);
    }

    false
}

pub fn hash_password(plain: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .expect("argon2 password hashing should not fail")
        .to_string()
}

fn verify_argon2(hash: &str, plain: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .and_then(|parsed| {
            Argon2::default()
                .verify_password(plain.as_bytes(), &parsed)
                .ok()
        })
        .is_some()
}

fn verify_bcrypt(hash: &str, plain: &str) -> bool {
    let normalized_hash = if hash.starts_with("$2y$") {
        hash.replacen("$2y$", "$2b$", 1)
    } else {
        hash.to_owned()
    };

    bcrypt::verify(plain, &normalized_hash).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{hash_password, verify_password};

    #[test]
    fn hashes_new_passwords_with_argon2id() {
        let hash = hash_password("secret-123");

        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "secret-123"));
        assert!(!verify_password(&hash, "wrong-password"));
    }

    #[test]
    fn verifies_laravel_compatible_bcrypt_hashes() {
        let hash = bcrypt::hash("legacy-secret", 4).expect("bcrypt hash should be generated");
        let laravel_hash = hash.replacen("$2b$", "$2y$", 1);

        assert!(verify_password(&laravel_hash, "legacy-secret"));
        assert!(!verify_password(&laravel_hash, "wrong-password"));
    }

    #[test]
    fn rejects_unknown_hash_formats() {
        assert!(!verify_password("plain-text", "plain-text"));
    }
}
