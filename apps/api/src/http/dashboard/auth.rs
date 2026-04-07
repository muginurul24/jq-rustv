use axum::{
    extract::{rejection::JsonRejection, ConnectInfo, State},
    http::{header, HeaderMap, HeaderValue},
    middleware as axum_middleware,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use justqiu_errors::{AppError, AppResult};

use crate::{
    app::AppState,
    extractors::authenticated_user::AuthenticatedUser,
    middleware::{csrf::csrf, session_auth::session_auth},
};

pub fn router(state: AppState) -> Router<AppState> {
    public_router().merge(protected_router(state))
}

fn public_router() -> Router<AppState> {
    Router::new()
        .route("/captcha", get(captcha))
        .route("/login", post(login))
}

fn protected_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route_layer(axum_middleware::from_fn(csrf))
        .route_layer(axum_middleware::from_fn_with_state(state, session_auth))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct MeUser {
    id: i64,
    username: String,
    name: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct MeResponse {
    success: bool,
    user: MeUser,
}

#[derive(Debug, Serialize)]
struct LogoutResponse {
    success: bool,
}

#[derive(Debug, Serialize)]
struct CaptchaResponse {
    captcha_id: String,
    image: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
    captcha_id: String,
    captcha_answer: String,
}

#[derive(Debug, Serialize, Clone)]
struct AuthUserResponse {
    id: i64,
    username: String,
    name: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    success: bool,
    user: AuthUserResponse,
}

#[derive(Debug, sqlx::FromRow)]
struct LoginUser {
    id: i64,
    username: String,
    name: String,
    role: String,
    password: String,
    is_active: bool,
}

async fn captcha(State(state): State<AppState>) -> AppResult<Json<CaptchaResponse>> {
    let challenge = justqiu_auth::generate_captcha();
    justqiu_auth::store_captcha(&state.redis, &challenge).await?;

    Ok(Json(CaptchaResponse {
        captcha_id: challenge.captcha_id.to_string(),
        image: challenge.image,
    }))
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> AppResult<(HeaderMap, Json<LoginResponse>)> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;

    let client_ip = request_client_ip(&headers, remote_addr);
    enforce_login_rate_limit(&state.redis, &client_ip).await?;
    justqiu_auth::verify_captcha(&state.redis, &payload.captcha_id, &payload.captcha_answer)
        .await?;

    let user = sqlx::query_as::<_, LoginUser>(
        r#"
        SELECT id, username, name, role, password, is_active
        FROM users
        WHERE LOWER(username) = LOWER($1)
        LIMIT 1
        "#,
    )
    .bind(&payload.username)
    .fetch_optional(&state.db)
    .await?;

    let user =
        user.ok_or_else(|| AppError::UnauthorizedMessage("Invalid credentials".to_string()))?;

    if !user.is_active {
        return Err(AppError::ForbiddenMessage(
            "Account is not active".to_string(),
        ));
    }

    if !justqiu_auth::verify_password(&user.password, &payload.password) {
        return Err(AppError::UnauthorizedMessage(
            "Invalid credentials".to_string(),
        ));
    }

    let user_agent = request_user_agent(&headers);
    let issued = justqiu_auth::issue_session(
        &state.redis,
        &state.config.jwt_secret,
        user.id,
        user.role.clone(),
        &client_ip,
        &user_agent,
        state.config.jwt_expiry_hours,
    )
    .await?;
    let xsrf_token =
        justqiu_auth::derive_csrf_token(&issued.bundle.data.csrf_secret, issued.bundle.sid)?;

    let mut response_headers = HeaderMap::new();
    append_auth_cookie(
        &mut response_headers,
        "session_jwt",
        &issued.jwt,
        issued.bundle.data.ttl_seconds(),
        true,
    )?;
    append_auth_cookie(
        &mut response_headers,
        "XSRF-TOKEN",
        &xsrf_token,
        issued.bundle.data.ttl_seconds(),
        false,
    )?;

    Ok((
        response_headers,
        Json(LoginResponse {
            success: true,
            user: AuthUserResponse::from(user),
        }),
    ))
}

async fn me(
    State(state): State<AppState>,
    authenticated_user: AuthenticatedUser,
) -> AppResult<Json<MeResponse>> {
    let user = sqlx::query_as::<_, MeUser>(
        r#"
        SELECT id, username, name, role
        FROM users
        WHERE id = $1
          AND is_active = TRUE
        LIMIT 1
        "#,
    )
    .bind(authenticated_user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let user = user.ok_or(AppError::Unauthorized)?;

    Ok(Json(MeResponse {
        success: true,
        user,
    }))
}

async fn logout(
    State(state): State<AppState>,
    authenticated_user: AuthenticatedUser,
) -> AppResult<(HeaderMap, Json<LogoutResponse>)> {
    justqiu_auth::delete_session(&state.redis, authenticated_user.sid).await?;

    let mut headers = HeaderMap::new();
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_static("session_jwt=; Max-Age=0; Path=/; HttpOnly; Secure; SameSite=Lax"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_static("XSRF-TOKEN=; Max-Age=0; Path=/; Secure; SameSite=Lax"),
    );

    Ok((headers, Json(LogoutResponse { success: true })))
}

impl From<LoginUser> for AuthUserResponse {
    fn from(user: LoginUser) -> Self {
        Self {
            id: user.id,
            username: user.username,
            name: user.name,
            role: user.role,
        }
    }
}

async fn enforce_login_rate_limit(client: &redis::Client, client_ip: &str) -> Result<(), AppError> {
    for (scope, limit, window_seconds) in [
        ("login:minute", 10_u64, 60_u64),
        ("login:hour", 30_u64, 3600_u64),
    ] {
        let decision =
            justqiu_redis::check_rate_limit(client, scope, client_ip, limit, window_seconds)
                .await
                .map_err(|err| AppError::Internal(err.into()))?;

        if decision.exceeded {
            return Err(AppError::RateLimitExceeded);
        }
    }

    Ok(())
}

fn request_client_ip(headers: &HeaderMap, remote_addr: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| remote_addr.ip().to_string())
}

fn request_user_agent(headers: &HeaderMap) -> String {
    headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

fn append_auth_cookie(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
    max_age: i64,
    http_only: bool,
) -> Result<(), AppError> {
    let mut cookie = format!("{name}={value}; Max-Age={max_age}; Path=/; Secure; SameSite=Lax");

    if http_only {
        cookie.push_str("; HttpOnly");
    }

    let header_value =
        HeaderValue::from_str(&cookie).map_err(|err| AppError::Internal(err.into()))?;
    headers.append(header::SET_COOKIE, header_value);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        sync::{Arc, LazyLock},
    };

    use axum::{
        body::{to_bytes, Body},
        extract::ConnectInfo,
        http::{header, Request, StatusCode},
    };
    use serde_json::{json, Value};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::{
        app::{create_router, AppState},
        config::AppConfig,
    };

    const TEST_LOGIN_IP: &str = "198.51.100.10";
    const TEST_USER_AGENT: &str = "codex-auth-test";
    static AUTH_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    #[derive(Debug)]
    struct AuthFixture {
        user_id: i64,
        username: String,
    }

    async fn test_state(redis_url: &str, database_url: &str) -> AppState {
        AppState {
            db: PgPoolOptions::new()
                .max_connections(2)
                .connect(database_url)
                .await
                .expect("postgres pool"),
            redis: redis::Client::open(redis_url).expect("redis client"),
            config: Arc::new(AppConfig {
                database_url: database_url.to_string(),
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

    fn unique_suffix() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        )
    }

    async fn insert_auth_user(db: &sqlx::PgPool, password: &str) -> AuthFixture {
        let suffix = unique_suffix();
        let username = format!("test_auth_{suffix}");
        let email = format!("{username}@localhost");
        let password_hash = justqiu_auth::hash_password(password);

        let user_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO users (username, name, email, password, role, is_active)
            VALUES ($1, $2, $3, $4, 'dev', true)
            RETURNING id
            "#,
        )
        .bind(&username)
        .bind("Test Auth User")
        .bind(&email)
        .bind(password_hash)
        .fetch_one(db)
        .await
        .expect("insert auth user");

        AuthFixture { user_id, username }
    }

    async fn cleanup_auth_user(db: &sqlx::PgPool, fixture: &AuthFixture) {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(fixture.user_id)
            .execute(db)
            .await
            .expect("delete auth user");
    }

    async fn cleanup_login_rate_limit_for_ip(redis: &redis::Client, client_ip: &str) {
        let minute_key = justqiu_redis::rate_limit_key("login:minute", client_ip);
        let hour_key = justqiu_redis::rate_limit_key("login:hour", client_ip);
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let _: i64 = redis::cmd("DEL")
            .arg(minute_key)
            .arg(hour_key)
            .query_async(&mut connection)
            .await
            .expect("delete login rate limit key");
    }

    async fn cleanup_login_rate_limit(redis: &redis::Client) {
        cleanup_login_rate_limit_for_ip(redis, TEST_LOGIN_IP).await;
    }

    async fn store_known_captcha(redis: &redis::Client, answer: &str) -> String {
        let captcha_id = Uuid::new_v4().to_string();
        let challenge = justqiu_auth::CaptchaChallenge {
            captcha_id: Uuid::parse_str(&captcha_id).expect("uuid"),
            answer: answer.to_string(),
            image: "<svg></svg>".to_string(),
        };

        justqiu_auth::store_captcha(redis, &challenge)
            .await
            .expect("store captcha");

        captcha_id
    }

    fn request_with_connect_info(mut request: Request<Body>) -> Request<Body> {
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 18080))));
        request
    }

    fn set_cookie_value(headers: &axum::http::HeaderMap, cookie_name: &str) -> String {
        headers
            .get_all(axum::http::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find_map(|value| {
                value
                    .strip_prefix(&format!("{cookie_name}="))
                    .and_then(|rest| rest.split(';').next())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| panic!("missing set-cookie for {cookie_name}"))
    }

    fn set_cookie_header(headers: &axum::http::HeaderMap) -> String {
        let session_jwt = set_cookie_value(headers, "session_jwt");
        let xsrf_token = set_cookie_value(headers, "XSRF-TOKEN");
        format!("session_jwt={session_jwt}; XSRF-TOKEN={xsrf_token}")
    }

    #[tokio::test]
    async fn me_without_cookie_returns_unauthorized() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state);

        let me_request = Request::builder()
            .method("GET")
            .uri("/backoffice/api/auth/me")
            .body(Body::empty())
            .expect("me request");
        let me_response = app.oneshot(me_request).await.expect("me response");
        let me_status = me_response.status();
        let me_body: Value = serde_json::from_slice(
            &to_bytes(me_response.into_body(), usize::MAX)
                .await
                .expect("me body"),
        )
        .expect("me json");

        assert_eq!(me_status, StatusCode::UNAUTHORIZED);
        assert_eq!(me_body["success"], Value::Bool(false));
        assert_eq!(
            me_body["message"],
            Value::String("Unauthenticated".to_string())
        );
    }

    #[tokio::test]
    async fn login_me_logout_flow_restores_and_invalidates_session() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let fixture = insert_auth_user(&state.db, "Secret123!").await;
        cleanup_login_rate_limit(&state.redis).await;

        let captcha_id = store_known_captcha(&state.redis, "ABCDE").await;
        let login_request = request_with_connect_info(
            Request::builder()
                .method("POST")
                .uri("/backoffice/api/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", TEST_LOGIN_IP)
                .header(header::USER_AGENT, TEST_USER_AGENT)
                .body(Body::from(
                    json!({
                        "username": fixture.username,
                        "password": "Secret123!",
                        "captcha_id": captcha_id,
                        "captcha_answer": "ABCDE"
                    })
                    .to_string(),
                ))
                .expect("login request"),
        );

        let login_response = app
            .clone()
            .oneshot(login_request)
            .await
            .expect("login response");
        let login_headers = login_response.headers().clone();
        let login_status = login_response.status();
        let login_body: Value = serde_json::from_slice(
            &to_bytes(login_response.into_body(), usize::MAX)
                .await
                .expect("login body"),
        )
        .expect("login json");

        assert_eq!(login_status, StatusCode::OK);
        assert_eq!(login_body["success"], Value::Bool(true));
        assert_eq!(
            login_body["user"]["username"],
            Value::String(fixture.username.clone())
        );

        let session_jwt = set_cookie_value(&login_headers, "session_jwt");
        let xsrf_token = set_cookie_value(&login_headers, "XSRF-TOKEN");
        let cookie_header = set_cookie_header(&login_headers);

        let session_cookie_header = login_headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("session_jwt="))
            .expect("session jwt set-cookie");
        let xsrf_cookie_header = login_headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("XSRF-TOKEN="))
            .expect("xsrf set-cookie");

        assert!(session_cookie_header.contains("HttpOnly"));
        assert!(!xsrf_cookie_header.contains("HttpOnly"));

        let claims =
            justqiu_auth::decode_jwt(&session_jwt, &state.config.jwt_secret).expect("decode jwt");
        let sid = claims.session_id().expect("sid");
        let session = justqiu_auth::get_session(&state.redis, sid)
            .await
            .expect("get session")
            .expect("session exists");
        assert_eq!(session.user_id, fixture.user_id);
        assert_eq!(session.role, "dev");

        let me_request = Request::builder()
            .method("GET")
            .uri("/backoffice/api/auth/me")
            .header(header::COOKIE, &cookie_header)
            .body(Body::empty())
            .expect("me request");
        let me_response = app.clone().oneshot(me_request).await.expect("me response");
        let me_status = me_response.status();
        let me_body: Value = serde_json::from_slice(
            &to_bytes(me_response.into_body(), usize::MAX)
                .await
                .expect("me body"),
        )
        .expect("me json");

        assert_eq!(me_status, StatusCode::OK);
        assert_eq!(me_body["success"], Value::Bool(true));
        assert_eq!(
            me_body["user"]["username"],
            Value::String(fixture.username.clone())
        );

        let logout_request = Request::builder()
            .method("POST")
            .uri("/backoffice/api/auth/logout")
            .header(header::COOKIE, &cookie_header)
            .header("X-XSRF-TOKEN", &xsrf_token)
            .body(Body::empty())
            .expect("logout request");
        let logout_response = app
            .clone()
            .oneshot(logout_request)
            .await
            .expect("logout response");
        let logout_headers = logout_response.headers().clone();
        let logout_status = logout_response.status();
        let logout_body: Value = serde_json::from_slice(
            &to_bytes(logout_response.into_body(), usize::MAX)
                .await
                .expect("logout body"),
        )
        .expect("logout json");

        assert_eq!(logout_status, StatusCode::OK);
        assert_eq!(logout_body["success"], Value::Bool(true));
        let cleared_session_cookie = logout_headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("session_jwt="))
            .expect("cleared session cookie");
        assert!(cleared_session_cookie.contains("Max-Age=0"));

        let session_after_logout = justqiu_auth::get_session(&state.redis, sid)
            .await
            .expect("get session after logout");
        assert!(session_after_logout.is_none());

        let me_after_logout_request = Request::builder()
            .method("GET")
            .uri("/backoffice/api/auth/me")
            .header(header::COOKIE, &cookie_header)
            .body(Body::empty())
            .expect("me after logout request");
        let me_after_logout_response = app
            .clone()
            .oneshot(me_after_logout_request)
            .await
            .expect("me after logout response");
        let me_after_logout_status = me_after_logout_response.status();
        let me_after_logout_body: Value = serde_json::from_slice(
            &to_bytes(me_after_logout_response.into_body(), usize::MAX)
                .await
                .expect("me after logout body"),
        )
        .expect("me after logout json");

        assert_eq!(me_after_logout_status, StatusCode::UNAUTHORIZED);
        assert_eq!(me_after_logout_body["success"], Value::Bool(false));
        assert_eq!(
            me_after_logout_body["message"],
            Value::String("Unauthenticated".to_string())
        );

        cleanup_login_rate_limit(&state.redis).await;
        cleanup_auth_user(&state.db, &fixture).await;
    }

    #[tokio::test]
    async fn logout_without_csrf_token_is_forbidden_and_session_remains_valid() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let fixture = insert_auth_user(&state.db, "Secret123!").await;
        cleanup_login_rate_limit(&state.redis).await;

        let captcha_id = store_known_captcha(&state.redis, "ABCDE").await;
        let login_request = request_with_connect_info(
            Request::builder()
                .method("POST")
                .uri("/backoffice/api/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", TEST_LOGIN_IP)
                .header(header::USER_AGENT, TEST_USER_AGENT)
                .body(Body::from(
                    json!({
                        "username": fixture.username,
                        "password": "Secret123!",
                        "captcha_id": captcha_id,
                        "captcha_answer": "ABCDE"
                    })
                    .to_string(),
                ))
                .expect("login request"),
        );
        let login_response = app
            .clone()
            .oneshot(login_request)
            .await
            .expect("login response");
        assert_eq!(login_response.status(), StatusCode::OK);
        let login_headers = login_response.headers().clone();
        let session_jwt = set_cookie_value(&login_headers, "session_jwt");
        let cookie_header = set_cookie_header(&login_headers);
        let claims =
            justqiu_auth::decode_jwt(&session_jwt, &state.config.jwt_secret).expect("decode jwt");
        let sid = claims.session_id().expect("sid");

        let logout_request = Request::builder()
            .method("POST")
            .uri("/backoffice/api/auth/logout")
            .header(header::COOKIE, &cookie_header)
            .body(Body::empty())
            .expect("logout request");
        let logout_response = app
            .clone()
            .oneshot(logout_request)
            .await
            .expect("logout response");
        let logout_status = logout_response.status();
        let logout_body: Value = serde_json::from_slice(
            &to_bytes(logout_response.into_body(), usize::MAX)
                .await
                .expect("logout body"),
        )
        .expect("logout json");

        assert_eq!(logout_status, StatusCode::FORBIDDEN);
        assert_eq!(logout_body["success"], Value::Bool(false));
        assert_eq!(
            logout_body["message"],
            Value::String("CSRF token missing".to_string())
        );

        let session = justqiu_auth::get_session(&state.redis, sid)
            .await
            .expect("get session")
            .expect("session still exists");
        assert_eq!(session.user_id, fixture.user_id);

        let me_request = Request::builder()
            .method("GET")
            .uri("/backoffice/api/auth/me")
            .header(header::COOKIE, &cookie_header)
            .body(Body::empty())
            .expect("me request");
        let me_response = app.clone().oneshot(me_request).await.expect("me response");
        assert_eq!(me_response.status(), StatusCode::OK);

        justqiu_auth::delete_session(&state.redis, sid)
            .await
            .expect("delete session");
        cleanup_login_rate_limit(&state.redis).await;
        cleanup_auth_user(&state.db, &fixture).await;
    }

    #[tokio::test]
    async fn login_captcha_is_single_use() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let fixture = insert_auth_user(&state.db, "Secret123!").await;
        cleanup_login_rate_limit(&state.redis).await;

        let captcha_id = store_known_captcha(&state.redis, "ABCDE").await;
        let login_body = json!({
            "username": fixture.username,
            "password": "Secret123!",
            "captcha_id": captcha_id,
            "captcha_answer": "ABCDE"
        })
        .to_string();

        let first_login_request = request_with_connect_info(
            Request::builder()
                .method("POST")
                .uri("/backoffice/api/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", TEST_LOGIN_IP)
                .header(header::USER_AGENT, TEST_USER_AGENT)
                .body(Body::from(login_body.clone()))
                .expect("first login request"),
        );
        let first_login_response = app
            .clone()
            .oneshot(first_login_request)
            .await
            .expect("first login response");
        let first_login_headers = first_login_response.headers().clone();
        let first_login_status = first_login_response.status();
        assert_eq!(first_login_status, StatusCode::OK);

        let session_jwt = set_cookie_value(&first_login_headers, "session_jwt");
        let xsrf_token = set_cookie_value(&first_login_headers, "XSRF-TOKEN");
        let cookie_header = set_cookie_header(&first_login_headers);
        let claims =
            justqiu_auth::decode_jwt(&session_jwt, &state.config.jwt_secret).expect("decode jwt");
        let sid = claims.session_id().expect("sid");

        let logout_request = Request::builder()
            .method("POST")
            .uri("/backoffice/api/auth/logout")
            .header(header::COOKIE, &cookie_header)
            .header("X-XSRF-TOKEN", &xsrf_token)
            .body(Body::empty())
            .expect("logout request");
        let logout_response = app
            .clone()
            .oneshot(logout_request)
            .await
            .expect("logout response");
        assert_eq!(logout_response.status(), StatusCode::OK);
        justqiu_auth::delete_session(&state.redis, sid)
            .await
            .expect("cleanup session");

        let second_login_request = request_with_connect_info(
            Request::builder()
                .method("POST")
                .uri("/backoffice/api/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", TEST_LOGIN_IP)
                .header(header::USER_AGENT, TEST_USER_AGENT)
                .body(Body::from(login_body))
                .expect("second login request"),
        );
        let second_login_response = app
            .clone()
            .oneshot(second_login_request)
            .await
            .expect("second login response");
        let second_login_status = second_login_response.status();
        let second_login_body: Value = serde_json::from_slice(
            &to_bytes(second_login_response.into_body(), usize::MAX)
                .await
                .expect("second login body"),
        )
        .expect("second login json");

        assert_eq!(second_login_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(second_login_body["success"], Value::Bool(false));
        assert_eq!(
            second_login_body["message"],
            Value::String("Captcha is invalid or expired".to_string())
        );

        cleanup_login_rate_limit(&state.redis).await;
        cleanup_auth_user(&state.db, &fixture).await;
    }

    #[tokio::test]
    async fn login_rejects_incorrect_captcha_answer() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let fixture = insert_auth_user(&state.db, "Secret123!").await;
        cleanup_login_rate_limit(&state.redis).await;

        let captcha_id = store_known_captcha(&state.redis, "ABCDE").await;
        let login_request = request_with_connect_info(
            Request::builder()
                .method("POST")
                .uri("/backoffice/api/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", TEST_LOGIN_IP)
                .header(header::USER_AGENT, TEST_USER_AGENT)
                .body(Body::from(
                    json!({
                        "username": fixture.username,
                        "password": "Secret123!",
                        "captcha_id": captcha_id,
                        "captcha_answer": "ZZZZZ"
                    })
                    .to_string(),
                ))
                .expect("login request"),
        );

        let login_response = app
            .clone()
            .oneshot(login_request)
            .await
            .expect("login response");
        let login_status = login_response.status();
        let login_body: Value = serde_json::from_slice(
            &to_bytes(login_response.into_body(), usize::MAX)
                .await
                .expect("login body"),
        )
        .expect("login json");

        assert_eq!(login_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(login_body["success"], Value::Bool(false));
        assert_eq!(
            login_body["message"],
            Value::String("Captcha answer is incorrect".to_string())
        );

        cleanup_login_rate_limit(&state.redis).await;
        cleanup_auth_user(&state.db, &fixture).await;
    }

    #[tokio::test]
    async fn login_rate_limit_blocks_repeated_attempts_after_threshold() {
        let _guard = AUTH_TEST_LOCK.lock().await;
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let fixture = insert_auth_user(&state.db, "Secret123!").await;
        let rate_limit_ip = format!("198.51.100.10-{}", unique_suffix());
        cleanup_login_rate_limit_for_ip(&state.redis, &rate_limit_ip).await;

        let mut issued_sids = Vec::new();
        let mut last_status = StatusCode::OK;
        let mut last_body = Value::Null;

        for index in 0..11 {
            let captcha_id = store_known_captcha(&state.redis, &format!("A{index:04}")).await;
            let login_request = request_with_connect_info(
                Request::builder()
                    .method("POST")
                    .uri("/backoffice/api/auth/login")
                    .header("content-type", "application/json")
                    .header("x-forwarded-for", &rate_limit_ip)
                    .header(header::USER_AGENT, TEST_USER_AGENT)
                    .body(Body::from(
                        json!({
                            "username": fixture.username,
                            "password": "Secret123!",
                            "captcha_id": captcha_id,
                            "captcha_answer": format!("A{index:04}")
                        })
                        .to_string(),
                    ))
                    .expect("login request"),
            );

            let login_response = app
                .clone()
                .oneshot(login_request)
                .await
                .expect("login response");
            let headers = login_response.headers().clone();
            last_status = login_response.status();
            last_body = serde_json::from_slice(
                &to_bytes(login_response.into_body(), usize::MAX)
                    .await
                    .expect("login body"),
            )
            .expect("login json");

            if index < 10 {
                assert_eq!(last_status, StatusCode::OK);
                let session_jwt = set_cookie_value(&headers, "session_jwt");
                let claims = justqiu_auth::decode_jwt(&session_jwt, &state.config.jwt_secret)
                    .expect("decode jwt");
                issued_sids.push(claims.session_id().expect("sid"));
            }
        }

        assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(last_body["success"], Value::Bool(false));
        assert_eq!(
            last_body["message"],
            Value::String("Rate limit exceeded. Try again later.".to_string())
        );

        let minute_key = justqiu_redis::rate_limit_key("login:minute", &rate_limit_ip);
        let hour_key = justqiu_redis::rate_limit_key("login:hour", &rate_limit_ip);
        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let minute_exists: bool = redis::cmd("EXISTS")
            .arg(&minute_key)
            .query_async(&mut connection)
            .await
            .expect("minute exists");
        let hour_exists: bool = redis::cmd("EXISTS")
            .arg(&hour_key)
            .query_async(&mut connection)
            .await
            .expect("hour exists");
        drop(connection);
        assert!(minute_exists);
        assert!(hour_exists);

        for sid in issued_sids {
            justqiu_auth::delete_session(&state.redis, sid)
                .await
                .expect("delete session");
        }

        cleanup_login_rate_limit_for_ip(&state.redis, &rate_limit_ip).await;
        cleanup_auth_user(&state.db, &fixture).await;
    }
}
