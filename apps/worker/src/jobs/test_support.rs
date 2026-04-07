use std::sync::OnceLock;

use tokio::sync::{Mutex, MutexGuard};

static WORKER_DB_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub async fn worker_db_test_lock() -> MutexGuard<'static, ()> {
    WORKER_DB_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await
}
