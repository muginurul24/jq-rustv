use axum::{
    extract::{rejection::JsonRejection, State},
    routing::post,
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use justqiu_errors::{AppError, AppResult};

use crate::app::AppState;

const DISBURSEMENT_IDEMPOTENCY_TTL_SECONDS: usize = 24 * 60 * 60;
const PROCESS_DISBURSEMENT_QUEUE_NAME: &str = "process_disbursement";

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(handle))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DisbursementWebhookPayload {
    amount: i64,
    partner_ref_no: String,
    status: String,
    transaction_date: Option<String>,
    merchant_id: String,
}

#[derive(Debug, Serialize)]
struct DisbursementWebhookAckResponse {
    status: bool,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct ProcessDisbursementWebhookJob {
    event_type: &'static str,
    received_at: String,
    payload: DisbursementWebhookPayload,
}

async fn handle(
    State(state): State<AppState>,
    payload: Result<Json<DisbursementWebhookPayload>, JsonRejection>,
) -> AppResult<Json<DisbursementWebhookAckResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let payload = validate_payload(payload)?;

    let idempotency_key =
        justqiu_redis::idempotency_key("webhook:disbursement", &payload.partner_ref_no)
            .map_err(map_redis_error)?;
    let acquired = justqiu_redis::acquire_idempotency(
        &state.redis,
        &idempotency_key,
        DISBURSEMENT_IDEMPOTENCY_TTL_SECONDS,
    )
    .await
    .map_err(map_redis_error)?;

    if acquired {
        let job = ProcessDisbursementWebhookJob {
            event_type: "disbursement",
            received_at: Utc::now().to_rfc3339(),
            payload,
        };

        justqiu_redis::enqueue_json(&state.redis, PROCESS_DISBURSEMENT_QUEUE_NAME, &job)
            .await
            .map_err(map_redis_error)?;
    }

    Ok(Json(DisbursementWebhookAckResponse {
        status: true,
        message: "OK",
    }))
}

fn validate_payload(
    payload: DisbursementWebhookPayload,
) -> Result<DisbursementWebhookPayload, AppError> {
    if payload.amount < 0 {
        return Err(AppError::UnprocessableEntity(
            "amount must be at least 0".to_string(),
        ));
    }

    let partner_ref_no = required_string("partner_ref_no", payload.partner_ref_no)?;
    let status = required_string("status", payload.status)?;
    let merchant_id = required_string("merchant_id", payload.merchant_id)?;

    Ok(DisbursementWebhookPayload {
        amount: payload.amount,
        partner_ref_no,
        status,
        transaction_date: optional_string(payload.transaction_date),
        merchant_id,
    })
}

fn required_string(field: &'static str, value: String) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::UnprocessableEntity(format!(
            "{field} is required"
        )));
    }

    Ok(value)
}

fn optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn map_redis_error(error: justqiu_redis::RedisStoreError) -> AppError {
    match error {
        justqiu_redis::RedisStoreError::Redis(_)
        | justqiu_redis::RedisStoreError::Serialization(_)
        | justqiu_redis::RedisStoreError::InvalidTtl
        | justqiu_redis::RedisStoreError::InvalidLimit
        | justqiu_redis::RedisStoreError::InvalidKeyPart
        | justqiu_redis::RedisStoreError::InvalidQueueName => {
            AppError::InternalMessage("Failed to accept disbursement webhook".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, LazyLock};

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use redis::AsyncCommands;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use crate::{app::AppState, config::AppConfig};

    static WEBHOOK_DISBURSEMENT_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn test_state(redis_url: &str) -> AppState {
        AppState {
            db: PgPoolOptions::new()
                .connect_lazy("postgresql://postgres:postgres@127.0.0.1:5432/justqiu")
                .expect("lazy postgres pool"),
            redis: redis::Client::open(redis_url).expect("redis client"),
            config: Arc::new(AppConfig {
                database_url: "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string(),
                redis_url: redis_url.to_string(),
                bind_address: "127.0.0.1:0".to_string(),
                jwt_secret: "test-jwt-secret".to_string(),
                jwt_expiry_hours: 8,
                nexusggr_api_url: "https://api.nexusggr.test".to_string(),
                nexusggr_agent_code: "agent".to_string(),
                nexusggr_agent_token: "token".to_string(),
                qris_api_url: "https://qris.test/api".to_string(),
                qris_merchant_uuid: "merchant-uuid".to_string(),
                qris_client: "client".to_string(),
                qris_client_key: "client-key".to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn disbursement_webhook_rejects_invalid_payload_without_enqueuing() {
        let _guard = WEBHOOK_DISBURSEMENT_TEST_LOCK.lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url);
        let app = crate::app::create_router(state.clone());
        let queue_key = justqiu_redis::job_queue_key("process_disbursement").expect("queue key");

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let _: i64 = redis::cmd("DEL")
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup before test");
        drop(connection);

        let request = Request::builder()
            .method("POST")
            .uri("/api/webhook/disbursement")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "amount": 50000,
                    "partner_ref_no": "",
                    "status": "success",
                    "merchant_id": "MID-INVALID-01"
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body")
                .to_vec(),
        )
        .expect("utf8 body");

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body,
            r#"{"message":"partner_ref_no is required","success":false}"#
        );

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        assert_eq!(queue_length, 0);
        let _: i64 = redis::cmd("DEL")
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup after test");
    }

    #[tokio::test]
    async fn disbursement_webhook_duplicate_partner_ref_only_enqueues_once() {
        let _guard = WEBHOOK_DISBURSEMENT_TEST_LOCK.lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url);
        let app = crate::app::create_router(state.clone());

        let partner_ref_no = "test-disbursement-duplicate-20260408";
        let idempotency_key =
            justqiu_redis::idempotency_key("webhook:disbursement", partner_ref_no)
                .expect("idempotency key");
        let queue_key = justqiu_redis::job_queue_key("process_disbursement").expect("queue key");

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let _: i64 = redis::cmd("DEL")
            .arg(&idempotency_key)
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup before test");
        drop(connection);

        let payload = serde_json::json!({
            "amount": 50000,
            "partner_ref_no": partner_ref_no,
            "status": "success",
            "transaction_date": "2026-04-08T04:14:00+07:00",
            "merchant_id": "MID-DUP-DISB-01"
        })
        .to_string();

        for _ in 0..2 {
            let request = Request::builder()
                .method("POST")
                .uri("/api/webhook/disbursement")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "203.0.113.22")
                .body(Body::from(payload.clone()))
                .expect("request");

            let response = app.clone().oneshot(request).await.expect("router response");
            let status = response.status();
            let body = String::from_utf8(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .expect("response body")
                    .to_vec(),
            )
            .expect("utf8 body");

            assert_eq!(status, StatusCode::OK);
            assert_eq!(body, r#"{"status":true,"message":"OK"}"#);
        }

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(queue_length, 1);
        assert!(idempotency_exists);

        let _: i64 = redis::cmd("DEL")
            .arg(&idempotency_key)
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup after test");
    }

    #[tokio::test]
    async fn disbursement_webhook_enforces_rate_limit_and_only_enqueues_once_for_duplicate_partner_ref(
    ) {
        let _guard = WEBHOOK_DISBURSEMENT_TEST_LOCK.lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url);
        let app = crate::app::create_router(state.clone());

        let partner_ref_no = "test-disbursement-rate-limit-20260408";
        let idempotency_key =
            justqiu_redis::idempotency_key("webhook:disbursement", partner_ref_no)
                .expect("idempotency key");
        let rate_limit_key = justqiu_redis::rate_limit_key("webhook:203.0.113.11", "disbursement");
        let queue_key = justqiu_redis::job_queue_key("process_disbursement").expect("queue key");

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let _: i64 = redis::cmd("DEL")
            .arg(&idempotency_key)
            .arg(&rate_limit_key)
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup before test");
        drop(connection);

        let payload = serde_json::json!({
            "amount": 50000,
            "partner_ref_no": partner_ref_no,
            "status": "success",
            "transaction_date": "2026-04-08T04:14:00+07:00",
            "merchant_id": "MID-DISB-01"
        })
        .to_string();

        let mut last_status = StatusCode::OK;
        let mut last_body = String::new();

        for _ in 0..121 {
            let request = Request::builder()
                .method("POST")
                .uri("/api/webhook/disbursement")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "203.0.113.11")
                .body(Body::from(payload.clone()))
                .expect("request");

            let response = app.clone().oneshot(request).await.expect("router response");
            last_status = response.status();
            last_body = String::from_utf8(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .expect("response body")
                    .to_vec(),
            )
            .expect("utf8 body");
        }

        assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            last_body,
            r#"{"message":"Rate limit exceeded. Try again later.","success":false}"#
        );

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");
        let rate_limit_exists: bool = connection.exists(&rate_limit_key).await.expect("exists");

        assert_eq!(queue_length, 1);
        assert!(idempotency_exists);
        assert!(rate_limit_exists);

        let _: i64 = redis::cmd("DEL")
            .arg(&idempotency_key)
            .arg(&rate_limit_key)
            .arg(&queue_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup after test");
    }
}
