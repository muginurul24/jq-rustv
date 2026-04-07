use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::extractors::authenticated_session::AuthenticatedSession;

pub async fn csrf(request: Request<Body>, next: Next) -> Response {
    if !requires_csrf(request.method()) {
        return next.run(request).await;
    }

    let session = match request.extensions().get::<AuthenticatedSession>().cloned() {
        Some(session) => session,
        None => return justqiu_errors::AppError::Unauthorized.into_response(),
    };

    let header = match request.headers().get("X-XSRF-TOKEN") {
        Some(value) => value,
        None => return csrf_error("CSRF token missing"),
    };

    let token = match header.to_str() {
        Ok(value) => value,
        Err(_) => return csrf_error("CSRF token mismatch"),
    };

    match justqiu_auth::verify_csrf_token(&session.csrf_secret, session.sid, token) {
        Ok(true) => next.run(request).await,
        Ok(false) => csrf_error("CSRF token mismatch"),
        Err(error) => error.into_response(),
    }
}

fn requires_csrf(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn csrf_error(message: &'static str) -> Response {
    (
        StatusCode::FORBIDDEN,
        axum::Json(json!({
            "success": false,
            "message": message,
        })),
    )
        .into_response()
}
