use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
};

use justqiu_errors::AppError;

use crate::app::AppState;

const WEBHOOK_RATE_LIMIT_PER_MINUTE: u64 = 120;
const WEBHOOK_RATE_WINDOW_SECONDS: u64 = 60;

pub async fn webhook_rate_limit(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, AppError> {
    let route = webhook_route(request.uri().path());
    let source = request_source(
        request.headers(),
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|connect_info| connect_info.0),
    );

    let scope = format!("webhook:{source}");
    let decision = justqiu_redis::check_rate_limit(
        &state.redis,
        &scope,
        &route,
        WEBHOOK_RATE_LIMIT_PER_MINUTE,
        WEBHOOK_RATE_WINDOW_SECONDS,
    )
    .await
    .map_err(|error| AppError::Internal(error.into()))?;

    if decision.exceeded {
        return Err(AppError::RateLimitExceeded);
    }

    Ok(next.run(request).await)
}

fn webhook_route(path: &str) -> String {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return "root".to_string();
    }

    path.replace('/', ":")
}

fn request_source(headers: &HeaderMap, remote_addr: Option<SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| remote_addr.map(|value| value.ip().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::{request_source, webhook_route};
    use axum::http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn normalizes_webhook_route_into_single_segment_key() {
        assert_eq!(webhook_route("/qris"), "qris");
        assert_eq!(webhook_route("/nested/path"), "nested:path");
    }

    #[test]
    fn prefers_forwarded_ip_over_remote_addr() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 127.0.0.1"),
        );

        let source = request_source(
            &headers,
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080)),
        );

        assert_eq!(source, "203.0.113.10");
    }
}
