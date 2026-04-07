pub mod disbursement;
pub mod qris;

use axum::{middleware::from_fn_with_state, Router};

use crate::app::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/disbursement", disbursement::router())
        .nest("/qris", qris::router())
        .layer(from_fn_with_state(
            state.clone(),
            crate::middleware::webhook_rate_limit::webhook_rate_limit,
        ))
        .with_state(state)
}
