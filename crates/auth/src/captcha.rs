use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use justqiu_errors::AppError;
use justqiu_redis::{get_del_captcha, put_captcha};

const CAPTCHA_TTL_SECONDS: u64 = 300;
const CAPTCHA_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const CAPTCHA_LENGTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptchaChallenge {
    pub captcha_id: Uuid,
    pub answer: String,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptchaRecord {
    pub hash: String,
    pub created_at: i64,
}

pub fn generate_captcha() -> CaptchaChallenge {
    let mut rng = rand::thread_rng();
    let answer = (0..CAPTCHA_LENGTH)
        .map(|_| {
            let index = rng.gen_range(0..CAPTCHA_ALPHABET.len());
            CAPTCHA_ALPHABET[index] as char
        })
        .collect::<String>();

    CaptchaChallenge {
        captcha_id: Uuid::new_v4(),
        image: render_svg(&answer, &mut rng),
        answer,
    }
}

pub async fn store_captcha(
    client: &redis::Client,
    challenge: &CaptchaChallenge,
) -> Result<(), AppError> {
    let record = CaptchaRecord {
        hash: hash_answer(&challenge.answer),
        created_at: Utc::now().timestamp(),
    };

    put_captcha(
        client,
        &challenge.captcha_id.to_string(),
        &record,
        CAPTCHA_TTL_SECONDS,
    )
    .await
    .map_err(|err| AppError::Internal(err.into()))
}

pub async fn verify_captcha(
    client: &redis::Client,
    captcha_id: &str,
    answer: &str,
) -> Result<(), AppError> {
    let stored = get_del_captcha::<CaptchaRecord>(client, captcha_id)
        .await
        .map_err(|err| AppError::Internal(err.into()))?
        .ok_or_else(|| {
            AppError::UnprocessableEntity("Captcha is invalid or expired".to_string())
        })?;

    let expected = hex::decode(stored.hash).map_err(|err| AppError::Internal(err.into()))?;
    let actual = Sha256::digest(answer.to_ascii_lowercase().as_bytes());

    if constant_time_eq(expected.as_slice(), actual.as_slice()) {
        return Ok(());
    }

    Err(AppError::UnprocessableEntity(
        "Captcha answer is incorrect".to_string(),
    ))
}

fn hash_answer(answer: &str) -> String {
    let digest = Sha256::digest(answer.to_ascii_lowercase().as_bytes());
    hex::encode(digest)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (lhs, rhs)| diff | (lhs ^ rhs))
        == 0
}

fn render_svg(answer: &str, rng: &mut impl Rng) -> String {
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="150" height="50" viewBox="0 0 150 50" role="img" aria-label="Captcha challenge">"##,
    );
    svg.push_str(r##"<rect width="150" height="50" rx="8" fill="#f8fafc"/>"##);

    for _ in 0..4 {
        let x1 = rng.gen_range(0..150);
        let y1 = rng.gen_range(0..50);
        let x2 = rng.gen_range(0..150);
        let y2 = rng.gen_range(0..50);
        svg.push_str(&format!(
            r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#cbd5e1" stroke-width="1.5"/>"##
        ));
    }

    for _ in 0..14 {
        let cx = rng.gen_range(5..145);
        let cy = rng.gen_range(5..45);
        let radius = rng.gen_range(1.0..2.3);
        svg.push_str(&format!(
            r##"<circle cx="{cx}" cy="{cy}" r="{radius:.1}" fill="#94a3b8" opacity="0.55"/>"##
        ));
    }

    for (index, character) in answer.chars().enumerate() {
        let x = 22 + (index as i32 * 23) + rng.gen_range(-3..=3);
        let y = 32 + rng.gen_range(-2..=5);
        let rotation = rng.gen_range(-15..=15);
        let font_size = rng.gen_range(28..=32);

        svg.push_str(&format!(
            r##"<text x="{x}" y="{y}" fill="#0f172a" font-family="monospace" font-size="{font_size}" font-weight="700" transform="rotate({rotation} {x} {y})">{character}</text>"##
        ));
    }

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_svg_captcha_challenge() {
        let challenge = generate_captcha();

        assert_eq!(challenge.answer.len(), CAPTCHA_LENGTH);
        assert!(challenge
            .answer
            .chars()
            .all(|ch| CAPTCHA_ALPHABET.contains(&(ch as u8))));
        assert!(challenge.image.starts_with("<svg"));
        assert!(challenge.image.contains("</svg>"));
    }

    #[test]
    fn hashes_answer_case_insensitively() {
        assert_eq!(hash_answer("ABCD9"), hash_answer("abcd9"));
    }

    #[test]
    fn captcha_record_hash_matches_lowercased_answer() {
        let record = CaptchaRecord {
            hash: hash_answer("Q7W2A"),
            created_at: 0,
        };
        let actual = Sha256::digest("q7w2a".as_bytes());

        assert!(constant_time_eq(
            hex::decode(record.hash).unwrap().as_slice(),
            actual.as_slice(),
        ));
    }
}
