use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use axum_extra::extract::cookie::CookieJar;

use justqiu_auth::decode_jwt;
use justqiu_errors::AppError;

use crate::{
    app::AppState,
    extractors::{
        authenticated_session::AuthenticatedSession, authenticated_user::AuthenticatedUser,
    },
};

pub async fn session_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let jar = CookieJar::from_headers(request.headers());
    let token = jar
        .get("session_jwt")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;

    let claims = decode_jwt(&token, &state.config.jwt_secret)?;
    let sid = claims.session_id()?;
    let user_id = claims.user_id()?;
    let session = justqiu_auth::get_session(&state.redis, sid)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if session.user_id != user_id || session.role != claims.role {
        return Err(AppError::Unauthorized);
    }

    request.extensions_mut().insert(AuthenticatedSession {
        user_id: session.user_id,
        role: session.role.clone(),
        sid,
        csrf_secret: session.csrf_secret,
    });

    request.extensions_mut().insert(AuthenticatedUser {
        user_id: session.user_id,
        role: session.role,
        sid,
    });

    Ok(next.run(request).await)
}
