mod jobs;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    justqiu_observability::init_tracing();

    tracing::info!("starting justqiu-worker");

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");

    let db = justqiu_database::pool::create_pool(&database_url)
        .await
        .expect("failed to connect to database");

    let redis = redis::Client::open(redis_url.as_str()).expect("failed to create redis client");

    tracing::info!("worker started — waiting for jobs");

    loop {
        jobs::run_once(&db, &redis).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
