use std::{net::SocketAddr, sync::Arc};

mod app;
pub mod config;
mod extractors;
mod http;
mod middleware;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    justqiu_observability::init_tracing();

    tracing::info!("starting justqiu-api");

    let config = config::AppConfig::from_env().expect("failed to load config");
    let bind_address = config.bind_address.clone();

    let db = justqiu_database::pool::create_pool(&config.database_url)
        .await
        .expect("failed to connect to database");

    let redis =
        redis::Client::open(config.redis_url.as_str()).expect("failed to create redis client");

    let state = app::AppState {
        db,
        redis,
        config: Arc::new(config),
    };

    let router = app::create_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .expect("failed to bind to address");

    tracing::info!(address = %bind_address, "listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}
