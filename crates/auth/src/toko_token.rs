use justqiu_domain::models::Toko;
use justqiu_errors::AppError;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};

const TOKO_TOKENABLE_TYPE: &str = "App\\Models\\Toko";

#[derive(Debug, FromRow)]
struct PersonalAccessTokenRow {
    id: i64,
    tokenable_id: i64,
}

pub async fn verify_toko_token(pool: &PgPool, bearer: &str) -> Result<Toko, AppError> {
    let (token_id, plaintext) = parse_sanctum_bearer(bearer)?;
    let token_hash = hash_sanctum_token(&plaintext);

    let token = sqlx::query_as::<_, PersonalAccessTokenRow>(
        r#"
        SELECT pat.id, pat.tokenable_id
        FROM personal_access_tokens pat
        WHERE pat.id = $1
          AND pat.token = $2
          AND pat.tokenable_type = $3
        LIMIT 1
        "#,
    )
    .bind(token_id)
    .bind(token_hash)
    .bind(TOKO_TOKENABLE_TYPE)
    .fetch_optional(pool)
    .await?;

    let token = token.ok_or(AppError::Unauthorized)?;

    let toko = sqlx::query_as::<_, Toko>(
        r#"
        SELECT id, user_id, name, callback_url, token, is_active, created_at, updated_at, deleted_at
        FROM tokos
        WHERE id = $1
          AND is_active = TRUE
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(token.tokenable_id)
    .fetch_optional(pool)
    .await?;

    let toko = toko.ok_or(AppError::Unauthorized)?;

    if let Err(error) = sqlx::query(
        r#"
        UPDATE personal_access_tokens
        SET last_used_at = NOW(), updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(token.id)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %error, token_id = token.id, "Failed to update toko token last_used_at");
    }

    Ok(toko)
}

fn parse_sanctum_bearer(bearer: &str) -> Result<(i64, String), AppError> {
    let bearer = bearer.trim();
    let (scheme, credentials) = bearer.split_once(' ').ok_or(AppError::Unauthorized)?;

    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AppError::Unauthorized);
    }

    let (token_id, plaintext) = credentials.split_once('|').ok_or(AppError::Unauthorized)?;

    if plaintext.is_empty() {
        return Err(AppError::Unauthorized);
    }

    let token_id = token_id
        .parse::<i64>()
        .map_err(|_| AppError::Unauthorized)?;

    if token_id <= 0 {
        return Err(AppError::Unauthorized);
    }

    Ok((token_id, plaintext.to_string()))
}

fn hash_sanctum_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{hash_sanctum_token, parse_sanctum_bearer};
    use justqiu_errors::AppError;

    #[test]
    fn parses_sanctum_bearer_tokens() {
        let (token_id, plaintext) =
            parse_sanctum_bearer("Bearer 42|plain-text-token").expect("token should parse");

        assert_eq!(token_id, 42);
        assert_eq!(plaintext, "plain-text-token");
    }

    #[test]
    fn accepts_case_insensitive_bearer_scheme() {
        let (token_id, plaintext) =
            parse_sanctum_bearer("bearer 7|abc123").expect("token should parse");

        assert_eq!(token_id, 7);
        assert_eq!(plaintext, "abc123");
    }

    #[test]
    fn rejects_invalid_bearer_formats() {
        for candidate in [
            "",
            "Bearer",
            "Token 1|abc",
            "Bearer abc|def",
            "Bearer -1|def",
            "Bearer 1|",
            "Bearer 1",
        ] {
            assert!(matches!(
                parse_sanctum_bearer(candidate),
                Err(AppError::Unauthorized)
            ));
        }
    }

    #[test]
    fn hashes_plaintext_like_sanctum() {
        assert_eq!(
            hash_sanctum_token("plain-text-token"),
            "6ead7167f9c191b0c1416c860bea7deb6a73b50f9f30da9a81c920e4fc17b3ee"
        );
    }
}
