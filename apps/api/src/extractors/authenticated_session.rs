use axum::{extract::FromRequestParts, http::request::Parts};
use std::future::Future;
use uuid::Uuid;

use justqiu_errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSession {
    pub user_id: i64,
    pub role: String,
    pub sid: Uuid,
    pub csrf_secret: String,
}

impl<S> FromRequestParts<S> for AuthenticatedSession
where
    S: Send + Sync,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let session = parts
            .extensions
            .get::<AuthenticatedSession>()
            .cloned()
            .ok_or(AppError::Unauthorized);

        async move { session }
    }
}
