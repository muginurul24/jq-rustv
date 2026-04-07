use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SEND_TOKO_CALLBACK_QUEUE_NAME: &str = "send_toko_callback";

const CALLBACK_IDEMPOTENCY_TTL_SECONDS: usize = 24 * 60 * 60;
const CALLBACK_MAX_ATTEMPTS: u8 = 4;
const INLINE_RETRY_DELAYS_MS: [u64; 2] = [250, 750];
const JOB_RETRY_BACKOFF_SECONDS: [i64; 3] = [10, 30, 60];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTokoCallbackJob {
    pub event_type: String,
    pub reference: String,
    pub callback_url: String,
    pub payload: Value,
    pub attempt: u8,
    pub not_before: Option<String>,
}

impl SendTokoCallbackJob {
    pub fn new(
        event_type: impl Into<String>,
        reference: impl Into<String>,
        callback_url: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            reference: reference.into(),
            callback_url: callback_url.into(),
            payload,
            attempt: 1,
            not_before: None,
        }
    }
}

pub async fn enqueue(
    redis: &redis::Client,
    job: &SendTokoCallbackJob,
) -> Result<(), justqiu_redis::RedisStoreError> {
    justqiu_redis::enqueue_json(redis, SEND_TOKO_CALLBACK_QUEUE_NAME, job).await
}

pub async fn run_once(redis: &redis::Client) -> bool {
    let job = match justqiu_redis::dequeue_json::<SendTokoCallbackJob>(
        redis,
        SEND_TOKO_CALLBACK_QUEUE_NAME,
    )
    .await
    {
        Ok(job) => job,
        Err(error) => {
            tracing::error!(error = %error, "failed to dequeue send_toko_callback job");
            return false;
        }
    };

    let Some(job) = job else {
        return false;
    };

    if !is_job_due(job.not_before.as_deref()) {
        if let Err(error) = enqueue(redis, &job).await {
            tracing::error!(
                error = %error,
                event_type = %job.event_type,
                reference = %job.reference,
                "failed to requeue deferred send_toko_callback job"
            );
        }
        return true;
    }

    let idempotency_key = match justqiu_redis::idempotency_key(
        &format!("callback:{}", job.event_type),
        &job.reference,
    ) {
        Ok(key) => key,
        Err(error) => {
            tracing::error!(
                error = %error,
                event_type = %job.event_type,
                reference = %job.reference,
                "failed to build callback idempotency key"
            );
            return true;
        }
    };

    let acquired = match justqiu_redis::acquire_idempotency(
        redis,
        &idempotency_key,
        CALLBACK_IDEMPOTENCY_TTL_SECONDS,
    )
    .await
    {
        Ok(acquired) => acquired,
        Err(error) => {
            tracing::error!(
                error = %error,
                event_type = %job.event_type,
                reference = %job.reference,
                "failed to acquire callback idempotency key"
            );
            if let Err(requeue_error) = enqueue(redis, &job).await {
                tracing::error!(
                    error = %requeue_error,
                    event_type = %job.event_type,
                    reference = %job.reference,
                    "failed to requeue send_toko_callback job after idempotency error"
                );
            }
            return true;
        }
    };

    if !acquired {
        let already_sent = match justqiu_redis::idempotency_exists(redis, &idempotency_key).await {
            Ok(exists) => exists,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    event_type = %job.event_type,
                    reference = %job.reference,
                    "failed to confirm callback idempotency state; skipping duplicate callback job"
                );
                true
            }
        };

        if already_sent {
            tracing::info!(
                event_type = %job.event_type,
                reference = %job.reference,
                "callback already delivered or currently locked; skipping job"
            );
        }
        return true;
    }

    let client = match justqiu_callback::CallbackClient::new() {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(
                error = %error,
                event_type = %job.event_type,
                reference = %job.reference,
                "failed to initialize callback client"
            );
            release_callback_lock(redis, &idempotency_key, &job).await;
            return true;
        }
    };

    let request = justqiu_callback::CallbackRequest {
        callback_url: job.callback_url.clone(),
        event_type: job.event_type.clone(),
        reference: job.reference.clone(),
        payload: job.payload.clone(),
    };

    match send_with_inline_retries(&client, &request).await {
        Ok(()) => {
            tracing::info!(
                event_type = %job.event_type,
                reference = %job.reference,
                callback_url = %job.callback_url,
                attempt = job.attempt,
                "delivered toko callback"
            );
        }
        Err(error) => {
            release_callback_lock(redis, &idempotency_key, &job).await;

            if is_permanent_callback_error(&error) || job.attempt >= CALLBACK_MAX_ATTEMPTS {
                tracing::error!(
                    error = %error,
                    event_type = %job.event_type,
                    reference = %job.reference,
                    callback_url = %job.callback_url,
                    attempt = job.attempt,
                    "dropping toko callback job after permanent error or max attempts"
                );
                return true;
            }

            let retry_job = schedule_retry(job);
            if let Err(requeue_error) = enqueue(redis, &retry_job).await {
                tracing::error!(
                    error = %requeue_error,
                    event_type = %retry_job.event_type,
                    reference = %retry_job.reference,
                    attempt = retry_job.attempt,
                    "failed to requeue toko callback job after delivery failure"
                );
                return true;
            }

            tracing::warn!(
                error = %error,
                event_type = %retry_job.event_type,
                reference = %retry_job.reference,
                callback_url = %retry_job.callback_url,
                attempt = retry_job.attempt,
                not_before = ?retry_job.not_before,
                "scheduled retry for toko callback job"
            );
        }
    }

    true
}

async fn send_with_inline_retries(
    client: &justqiu_callback::CallbackClient,
    request: &justqiu_callback::CallbackRequest,
) -> Result<(), justqiu_callback::CallbackError> {
    if let Err(error) = client.send_json(request).await {
        for delay_ms in INLINE_RETRY_DELAYS_MS {
            if is_permanent_callback_error(&error) {
                return Err(error);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            match client.send_json(request).await {
                Ok(()) => return Ok(()),
                Err(next_error) => {
                    if is_permanent_callback_error(&next_error) {
                        return Err(next_error);
                    }

                    continue;
                }
            }
        }

        return Err(error);
    }

    Ok(())
}

fn schedule_retry(mut job: SendTokoCallbackJob) -> SendTokoCallbackJob {
    let current_attempt = job.attempt.max(1);
    let delay_seconds = JOB_RETRY_BACKOFF_SECONDS
        .get((current_attempt.saturating_sub(1)) as usize)
        .copied()
        .unwrap_or(*JOB_RETRY_BACKOFF_SECONDS.last().unwrap_or(&60));

    job.attempt = current_attempt.saturating_add(1);
    job.not_before = Some((Utc::now() + Duration::seconds(delay_seconds)).to_rfc3339());
    job
}

fn is_job_due(not_before: Option<&str>) -> bool {
    let Some(not_before) = not_before.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };

    match DateTime::parse_from_rfc3339(not_before) {
        Ok(not_before) => not_before.with_timezone(&Utc) <= Utc::now(),
        Err(_) => true,
    }
}

fn is_permanent_callback_error(error: &justqiu_callback::CallbackError) -> bool {
    matches!(
        error,
        justqiu_callback::CallbackError::InvalidUrl
            | justqiu_callback::CallbackError::InvalidHeader(_)
    )
}

async fn release_callback_lock(redis: &redis::Client, key: &str, job: &SendTokoCallbackJob) {
    if let Err(error) = justqiu_redis::release_idempotency(redis, key).await {
        tracing::error!(
            error = %error,
            event_type = %job.event_type,
            reference = %job.reference,
            "failed to release callback idempotency lock"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    use chrono::{DateTime, Utc};
    use redis::AsyncCommands;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
        time::Duration as TokioDuration,
    };

    use crate::jobs::test_support::worker_db_test_lock;

    use super::{run_once, SendTokoCallbackJob, SEND_TOKO_CALLBACK_QUEUE_NAME};

    struct CallbackTestServer {
        address: String,
        request_count: Arc<AtomicUsize>,
        handle: JoinHandle<()>,
    }

    impl CallbackTestServer {
        async fn spawn(statuses: Vec<u16>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind callback test server");
            let local_addr = listener.local_addr().expect("local addr");
            let request_count = Arc::new(AtomicUsize::new(0));
            let statuses = Arc::new(Mutex::new(VecDeque::from(statuses)));
            let fallback_status = {
                let statuses = statuses.lock().expect("statuses lock");
                statuses.back().copied().unwrap_or(500)
            };

            let handle = {
                let request_count = Arc::clone(&request_count);
                let statuses = Arc::clone(&statuses);

                tokio::spawn(async move {
                    loop {
                        let (mut stream, _) = match listener.accept().await {
                            Ok(value) => value,
                            Err(_) => break,
                        };
                        let request_count = Arc::clone(&request_count);
                        let statuses = Arc::clone(&statuses);

                        tokio::spawn(async move {
                            let _ = tokio::time::timeout(
                                TokioDuration::from_secs(1),
                                read_http_request(&mut stream),
                            )
                            .await;

                            request_count.fetch_add(1, Ordering::SeqCst);
                            let status = {
                                let mut statuses = statuses.lock().expect("statuses lock");
                                statuses.pop_front().unwrap_or(fallback_status)
                            };
                            let reason = reason_phrase(status);
                            let body = format!("status-{status}");
                            let response = format!(
                                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );

                            let _ = stream.write_all(response.as_bytes()).await;
                            let _ = stream.shutdown().await;
                        });
                    }
                })
            };

            Self {
                address: format!("http://{local_addr}/callback"),
                request_count,
                handle,
            }
        }

        fn request_count(&self) -> usize {
            self.request_count.load(Ordering::SeqCst)
        }

        fn abort(self) {
            self.handle.abort();
        }
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut header_end = None;
        let mut content_length = 0usize;

        loop {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }

            bytes.extend_from_slice(&chunk[..read]);

            if header_end.is_none() {
                if let Some(end) = find_header_end(&bytes) {
                    header_end = Some(end);
                    content_length = parse_content_length(&bytes[..end]);
                }
            }

            if let Some(end) = header_end {
                if bytes.len() >= end + 4 + content_length {
                    break;
                }
            }
        }

        Ok(bytes)
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn parse_content_length(headers: &[u8]) -> usize {
        let headers = String::from_utf8_lossy(headers);
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    return value.trim().parse::<usize>().ok();
                }
                None
            })
            .unwrap_or(0)
    }

    fn reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            500 => "Internal Server Error",
            _ => "Test",
        }
    }

    async fn cleanup_callback_test_state(
        redis: &redis::Client,
        reference: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", reference).expect("idempotency key");
        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let mut connection = redis.get_multiplexed_async_connection().await?;
        let _: i64 = redis::cmd("DEL")
            .arg(idempotency_key)
            .arg(queue_key)
            .query_async(&mut connection)
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn callback_worker_treats_invalid_not_before_as_due() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-invalid-not-before-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let server = CallbackTestServer::spawn(vec![200]).await;
        let mut job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            &server.address,
            json!({
                "amount": 10000,
                "trx_id": reference,
                "status": "success",
            }),
        );
        job.not_before = Some("not-a-timestamp".to_string());

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");
        assert!(run_once(&redis).await);

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(server.request_count(), 1);
        assert_eq!(queue_length, 0);
        assert!(idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
        server.abort();
    }

    #[tokio::test]
    async fn callback_worker_drops_permanent_error_without_requeue() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-permanent-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            "not-a-valid-url",
            json!({
                "amount": 10000,
                "trx_id": reference,
                "status": "success",
            }),
        );

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");
        assert!(run_once(&redis).await);

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(queue_length, 0);
        assert!(!idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
    }

    #[tokio::test]
    async fn callback_worker_skips_already_locked_or_delivered_job_without_attempt() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-locked-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            "http://127.0.0.1:9/callback",
            json!({
                "amount": 10000,
                "trx_id": reference,
                "status": "success",
            }),
        );
        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");

        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let acquired = justqiu_redis::acquire_idempotency(&redis, &idempotency_key, 60)
            .await
            .expect("acquire idempotency");
        assert!(acquired);

        assert!(run_once(&redis).await);

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(queue_length, 0);
        assert!(idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
    }

    #[tokio::test]
    async fn callback_worker_drops_job_after_max_attempts_without_requeue() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-max-attempt-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let server = CallbackTestServer::spawn(vec![500, 500, 500]).await;
        let mut job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            &server.address,
            json!({
                "amount": 15000,
                "trx_id": reference,
                "status": "success",
            }),
        );
        job.attempt = 4;

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");
        assert!(run_once(&redis).await);

        tokio::time::sleep(TokioDuration::from_millis(150)).await;

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(server.request_count(), 3);
        assert_eq!(queue_length, 0);
        assert!(!idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
        server.abort();
    }

    #[tokio::test]
    async fn callback_worker_requeues_future_not_before_job_without_delivery_attempt() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-deferred-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let not_before = (Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
        let mut job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            "http://127.0.0.1:9/callback",
            json!({
                "amount": 12000,
                "trx_id": reference,
                "status": "success",
            }),
        );
        job.not_before = Some(not_before.clone());

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue deferred callback job");
        assert!(run_once(&redis).await);

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let queued_job_raw: String = connection
            .lindex(&queue_key, 0)
            .await
            .expect("queued deferred job");
        let queued_job: SendTokoCallbackJob =
            serde_json::from_str(&queued_job_raw).expect("deserialize deferred job");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(queue_length, 1);
        assert_eq!(queued_job.reference, reference);
        assert_eq!(queued_job.attempt, 1);
        assert_eq!(queued_job.not_before.as_deref(), Some(not_before.as_str()));
        assert!(!idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
    }

    #[tokio::test]
    async fn callback_worker_retries_inline_until_delivery_succeeds() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-inline-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let server = CallbackTestServer::spawn(vec![500, 500, 200]).await;
        let job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            &server.address,
            json!({
                "amount": 10000,
                "trx_id": reference,
                "status": "success",
            }),
        );

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");
        assert!(run_once(&redis).await);

        tokio::time::sleep(TokioDuration::from_millis(150)).await;

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queue_length: i64 = connection.llen(&queue_key).await.expect("queue length");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(server.request_count(), 3);
        assert_eq!(queue_length, 0);
        assert!(idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
        server.abort();
    }

    #[tokio::test]
    async fn callback_worker_requeues_with_not_before_after_inline_retries_exhausted() {
        let _guard = worker_db_test_lock().await;
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis = redis::Client::open(redis_url).expect("redis client");
        let reference = format!(
            "test-callback-backoff-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup before test");

        let server = CallbackTestServer::spawn(vec![500, 500, 500]).await;
        let job = SendTokoCallbackJob::new(
            "qris",
            &reference,
            &server.address,
            json!({
                "amount": 20000,
                "trx_id": reference,
                "status": "success",
            }),
        );

        super::enqueue(&redis, &job)
            .await
            .expect("enqueue callback job");
        assert!(run_once(&redis).await);

        tokio::time::sleep(TokioDuration::from_millis(150)).await;

        let queue_key =
            justqiu_redis::job_queue_key(SEND_TOKO_CALLBACK_QUEUE_NAME).expect("queue key");
        let idempotency_key =
            justqiu_redis::idempotency_key("callback:qris", &reference).expect("idempotency key");
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let queued_job_raw: String = connection
            .lindex(&queue_key, 0)
            .await
            .expect("queued retry job");
        let queued_job: SendTokoCallbackJob =
            serde_json::from_str(&queued_job_raw).expect("deserialize retry job");
        let idempotency_exists: bool = connection.exists(&idempotency_key).await.expect("exists");

        assert_eq!(server.request_count(), 3);
        assert_eq!(queued_job.reference, reference);
        assert_eq!(queued_job.attempt, 2);
        let not_before = queued_job
            .not_before
            .as_deref()
            .expect("scheduled retry not_before");
        let not_before = DateTime::parse_from_rfc3339(not_before)
            .expect("parse not_before")
            .with_timezone(&Utc);
        assert!(not_before > Utc::now());
        assert!(!idempotency_exists);

        drop(connection);
        cleanup_callback_test_state(&redis, &reference)
            .await
            .expect("cleanup after test");
        server.abort();
    }
}
