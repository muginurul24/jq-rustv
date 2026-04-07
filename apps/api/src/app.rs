use axum::{routing::get, Router};
use sqlx::PgPool;
use std::sync::Arc;

use crate::{config, http};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub config: Arc<config::AppConfig>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .nest("/api/v1", http::bridge::router(state.clone()))
        .nest("/api/webhook", http::webhook::router(state.clone()))
        .nest(
            "/backoffice/api/auth",
            http::dashboard::auth::router(state.clone()),
        )
        .with_state(state)
}

async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "success": true, "message": "OK" }))
}
