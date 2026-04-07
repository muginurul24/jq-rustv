use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,

    #[error("{0}")]
    NotFoundMessage(String),

    #[error("Unauthenticated")]
    Unauthorized,

    #[error("{0}")]
    UnauthorizedMessage(String),

    #[error("Forbidden")]
    Forbidden,

    #[error("{0}")]
    ForbiddenMessage(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    UnprocessableEntity(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("{0}")]
    InternalMessage(String),

    #[error("Internal error")]
    Internal(#[from] anyhow::Error),

    #[error("Database error")]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::NotFoundMessage(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthenticated".to_string()),
            AppError::UnauthorizedMessage(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".to_string()),
            AppError::ForbiddenMessage(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded. Try again later.".to_string(),
            ),
            AppError::InternalMessage(msg) => {
                tracing::error!(message = %msg, "Internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
            AppError::Internal(err) => {
                tracing::error!(error = %err, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Database(err) => {
                tracing::error!(error = %err, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        let body = json!({ "success": false, "message": message });

        (status, axum::Json(body)).into_response()
    }
}

/// Convenience type alias for handlers.
pub type AppResult<T> = Result<T, AppError>;
