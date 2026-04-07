use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};

use justqiu_errors::AppError;

use crate::{app::AppState, extractors::authenticated_toko::AuthenticatedToko};

const API_RATE_LIMIT_PER_MINUTE: u64 = 120;
const API_RATE_WINDOW_SECONDS: u64 = 60;

pub async fn api_rate_limit(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let toko = request
        .extensions()
        .get::<AuthenticatedToko>()
        .cloned()
        .ok_or(AppError::Unauthorized)?;

    let route = api_route(request.uri().path());
    let identifier = toko.0.id.to_string();

    let decision = justqiu_redis::check_rate_limit(
        &state.redis,
        &format!("api:{identifier}"),
        &route,
        API_RATE_LIMIT_PER_MINUTE,
        API_RATE_WINDOW_SECONDS,
    )
    .await
    .map_err(|error| AppError::Internal(error.into()))?;

    if decision.exceeded {
        return Err(AppError::RateLimitExceeded);
    }

    Ok(next.run(request).await)
}

fn api_route(path: &str) -> String {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return "root".to_string();
    }

    path.replace('/', ":")
}

#[cfg(test)]
mod tests {
    use super::api_route;

    #[test]
    fn normalizes_api_route_into_single_segment_key() {
        assert_eq!(api_route("/balance"), "balance");
        assert_eq!(api_route("/call/list"), "call:list");
    }
}
