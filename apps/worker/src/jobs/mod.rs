pub mod process_disbursement;
pub mod process_qris;
pub mod send_toko_callback;
#[cfg(test)]
pub mod test_support;

pub async fn run_once(db: &sqlx::PgPool, redis: &redis::Client) {
    if process_qris::run_once(db, redis).await {
        return;
    }

    if process_disbursement::run_once(db, redis).await {
        return;
    }

    if send_toko_callback::run_once(redis).await {
        return;
    }
}
