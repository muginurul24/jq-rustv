use axum::{
    body::Body,
    extract::State,
    http::{header, Request},
    middleware::Next,
    response::Response,
};

use justqiu_errors::AppError;

use crate::{app::AppState, extractors::authenticated_toko::AuthenticatedToko};

pub async fn toko_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let toko = justqiu_auth::verify_toko_token(&state.db, authorization).await?;
    request.extensions_mut().insert(AuthenticatedToko(toko));

    Ok(next.run(request).await)
}
