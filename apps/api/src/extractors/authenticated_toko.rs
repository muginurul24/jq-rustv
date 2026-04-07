use axum::{extract::FromRequestParts, http::request::Parts};
use std::future::Future;

use justqiu_domain::models::Toko;
use justqiu_errors::AppError;

#[derive(Debug, Clone)]
pub struct AuthenticatedToko(pub Toko);

impl<S> FromRequestParts<S> for AuthenticatedToko
where
    S: Send + Sync,
{
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let toko = parts
            .extensions
            .get::<AuthenticatedToko>()
            .cloned()
            .ok_or(AppError::Unauthorized);

        async move { toko }
    }
}
