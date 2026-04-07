use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::jobs::send_toko_callback::SendTokoCallbackJob;

const PROCESS_QRIS_QUEUE_NAME: &str = "process_qris";
const QRIS_SUCCESS_STATUS: &str = "success";
const REGULAR_DEPOSIT_PURPOSE: &str = "generate";
const NEXUSGGR_TOPUP_PURPOSE: &str = "nexusggr_topup";

#[derive(Debug, Clone, Deserialize, Serialize)]
struct QrisWebhookPayload {
    amount: i64,
    terminal_id: String,
    merchant_id: String,
    trx_id: String,
    rrn: Option<String>,
    custom_ref: Option<String>,
    vendor: Option<String>,
    status: String,
    created_at: Option<String>,
    finish_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProcessQrisWebhookJob {
    event_type: String,
    received_at: String,
    payload: QrisWebhookPayload,
}

#[derive(Debug)]
struct SanitizedQrisWebhookPayload {
    amount: i64,
    terminal_id: String,
    trx_id: String,
    rrn: Option<String>,
    custom_ref: Option<String>,
    vendor: Option<String>,
    status: String,
    created_at: Option<String>,
    finish_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedQrisTransaction {
    id: i64,
    toko_id: i64,
    status: String,
    note: Option<String>,
    callback_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExistingQrisTransactionNote {
    purpose: Option<String>,
    custom_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegularDepositTransactionNote {
    rrn: Option<String>,
    vendor: Option<String>,
    custom_ref: Option<String>,
    finish_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct QrisCallbackPayload {
    amount: i64,
    terminal_id: String,
    trx_id: String,
    rrn: Option<String>,
    custom_ref: Option<String>,
    vendor: Option<String>,
    status: String,
    created_at: Option<String>,
    finish_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedIncome {
    id: i64,
    ggr: i64,
    fee_transaction: i64,
}

pub async fn run_once(db: &sqlx::PgPool, redis: &redis::Client) -> bool {
    let job =
        match justqiu_redis::dequeue_json::<ProcessQrisWebhookJob>(redis, PROCESS_QRIS_QUEUE_NAME)
            .await
        {
            Ok(job) => job,
            Err(error) => {
                tracing::error!(error = %error, "failed to dequeue process_qris job");
                return false;
            }
        };

    let Some(job) = job else {
        return false;
    };

    let payload = sanitize_payload(job.payload.clone());

    match process_job(db, &payload).await {
        Ok(callback_job) => {
            if let Some(callback_job) = callback_job {
                if let Err(error) =
                    crate::jobs::send_toko_callback::enqueue(redis, &callback_job).await
                {
                    tracing::error!(
                        error = %error,
                        trx_id = %payload.trx_id,
                        "failed to enqueue toko callback job after qris processing"
                    );
                    if let Err(requeue_error) =
                        justqiu_redis::enqueue_json(redis, PROCESS_QRIS_QUEUE_NAME, &job).await
                    {
                        tracing::error!(
                            error = %requeue_error,
                            trx_id = %job.payload.trx_id,
                            "failed to requeue process_qris job after callback enqueue failure"
                        );
                    }
                    return true;
                }
            }

            true
        }
        Err(error) => {
            tracing::error!(
                error = %error,
                trx_id = %payload.trx_id,
                status = %payload.status,
                "failed to process qris webhook job"
            );
            if let Err(requeue_error) =
                justqiu_redis::enqueue_json(redis, PROCESS_QRIS_QUEUE_NAME, &job).await
            {
                tracing::error!(
                    error = %requeue_error,
                    trx_id = %job.payload.trx_id,
                    "failed to requeue process_qris job after database lookup failure"
                );
            }
            true
        }
    }
}

type WorkerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn process_job(
    db: &sqlx::PgPool,
    payload: &SanitizedQrisWebhookPayload,
) -> WorkerResult<Option<SendTokoCallbackJob>> {
    if !payload.status.eq_ignore_ascii_case(QRIS_SUCCESS_STATUS) {
        tracing::info!(
            trx_id = %payload.trx_id,
            status = %payload.status,
            "qris webhook status not handled in process_qris; skipping"
        );
        return Ok(None);
    }

    let mut transaction = db.begin().await?;

    let locked_transaction = sqlx::query_as::<_, LockedQrisTransaction>(
        r#"
        SELECT transactions.id,
               transactions.toko_id,
               transactions.status,
               transactions.note,
               tokos.callback_url
        FROM transactions
        JOIN tokos ON tokos.id = transactions.toko_id
        WHERE code = $1
          AND category = 'qris'
          AND type = 'deposit'
          AND transactions.deleted_at IS NULL
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(&payload.trx_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(locked_transaction) = locked_transaction else {
        tracing::warn!(
            trx_id = %payload.trx_id,
            status = %payload.status,
            amount = payload.amount,
            terminal_id = %payload.terminal_id,
            rrn = ?payload.rrn,
            custom_ref = ?payload.custom_ref,
            created_at = ?payload.created_at,
            finish_at = ?payload.finish_at,
            "pending qris transaction not found during process_qris"
        );
        transaction.rollback().await?;
        return Ok(None);
    };

    if locked_transaction.status != "pending" {
        tracing::info!(
            trx_id = %payload.trx_id,
            transaction_id = locked_transaction.id,
            toko_id = locked_transaction.toko_id,
            transaction_status = %locked_transaction.status,
            "qris transaction already processed; skipping process_qris"
        );
        transaction.rollback().await?;
        if locked_transaction
            .status
            .eq_ignore_ascii_case(QRIS_SUCCESS_STATUS)
        {
            return Ok(build_qris_callback_job(&locked_transaction, payload)?);
        }
        return Ok(None);
    }

    let existing_note = parse_existing_note(locked_transaction.note.as_deref(), &payload.trx_id)?;

    match existing_note.purpose.as_deref() {
        Some(REGULAR_DEPOSIT_PURPOSE) => {
            process_regular_deposit(
                &mut transaction,
                &locked_transaction,
                &existing_note,
                payload,
            )
            .await?;

            transaction.commit().await?;

            tracing::info!(
                trx_id = %payload.trx_id,
                transaction_id = locked_transaction.id,
                toko_id = locked_transaction.toko_id,
                amount = payload.amount,
                vendor = ?payload.vendor,
                rrn = ?payload.rrn,
                custom_ref = ?payload.custom_ref,
                finish_at = ?payload.finish_at,
                "processed qris regular deposit success"
            );
            return Ok(build_qris_callback_job(&locked_transaction, payload)?);
        }
        Some(NEXUSGGR_TOPUP_PURPOSE) => {
            process_nexusggr_topup(&mut transaction, &locked_transaction, payload).await?;

            transaction.commit().await?;

            tracing::info!(
                trx_id = %payload.trx_id,
                transaction_id = locked_transaction.id,
                toko_id = locked_transaction.toko_id,
                amount = payload.amount,
                vendor = ?payload.vendor,
                rrn = ?payload.rrn,
                finish_at = ?payload.finish_at,
                "processed qris nexusggr topup success"
            );
            return Ok(build_qris_callback_job(&locked_transaction, payload)?);
        }
        Some(purpose) => {
            tracing::warn!(
                trx_id = %payload.trx_id,
                transaction_id = locked_transaction.id,
                toko_id = locked_transaction.toko_id,
                purpose = %purpose,
                "qris transaction purpose is unsupported; leaving transaction pending"
            );
            transaction.rollback().await?;
            return Ok(None);
        }
        None => {
            tracing::warn!(
                trx_id = %payload.trx_id,
                transaction_id = locked_transaction.id,
                toko_id = locked_transaction.toko_id,
                "qris transaction note purpose is missing; leaving transaction pending"
            );
            transaction.rollback().await?;
            return Ok(None);
        }
    }
}

async fn process_regular_deposit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    locked_transaction: &LockedQrisTransaction,
    existing_note: &ExistingQrisTransactionNote,
    payload: &SanitizedQrisWebhookPayload,
) -> WorkerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO balances (toko_id, pending, settle, nexusggr)
        VALUES ($1, 0, 0, 0)
        ON CONFLICT (toko_id) DO NOTHING
        "#,
    )
    .bind(locked_transaction.toko_id)
    .execute(&mut **transaction)
    .await?;

    let income = sqlx::query_as::<_, LockedIncome>(
        r#"
        SELECT id, ggr, fee_transaction
        FROM incomes
        ORDER BY id ASC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| internal_worker_error("missing incomes bootstrap row"))?;

    let fee_income = calculate_fee(payload.amount, income.fee_transaction)?;
    let pending_increment = payload
        .amount
        .checked_sub(fee_income)
        .ok_or_else(|| internal_worker_error("qris pending increment underflow"))?;

    let note_payload = RegularDepositTransactionNote {
        rrn: payload.rrn.clone(),
        vendor: payload.vendor.clone(),
        custom_ref: payload
            .custom_ref
            .clone()
            .or(existing_note.custom_ref.clone()),
        finish_at: payload.finish_at.clone(),
    };
    let note_json = serde_json::to_string(&note_payload)?;

    let updated_transaction = sqlx::query(
        r#"
        UPDATE transactions
        SET status = 'success',
            amount = $2,
            player = $3,
            note = $4,
            updated_at = NOW()
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(locked_transaction.id)
    .bind(payload.amount)
    .bind(&payload.terminal_id)
    .bind(note_json)
    .execute(&mut **transaction)
    .await?;
    if updated_transaction.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update qris transaction {} as success",
            locked_transaction.id
        )));
    }

    let updated_balance = sqlx::query(
        r#"
        UPDATE balances
        SET pending = pending + $2,
            updated_at = NOW()
        WHERE toko_id = $1
        "#,
    )
    .bind(locked_transaction.toko_id)
    .bind(pending_increment)
    .execute(&mut **transaction)
    .await?;
    if updated_balance.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update pending balance for toko {}",
            locked_transaction.toko_id
        )));
    }

    let updated_income = sqlx::query(
        r#"
        UPDATE incomes
        SET amount = amount + $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(income.id)
    .bind(fee_income)
    .execute(&mut **transaction)
    .await?;
    if updated_income.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update income row {}",
            income.id
        )));
    }

    Ok(())
}

async fn process_nexusggr_topup(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    locked_transaction: &LockedQrisTransaction,
    payload: &SanitizedQrisWebhookPayload,
) -> WorkerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO balances (toko_id, pending, settle, nexusggr)
        VALUES ($1, 0, 0, 0)
        ON CONFLICT (toko_id) DO NOTHING
        "#,
    )
    .bind(locked_transaction.toko_id)
    .execute(&mut **transaction)
    .await?;

    let income = sqlx::query_as::<_, LockedIncome>(
        r#"
        SELECT id, ggr, fee_transaction
        FROM incomes
        ORDER BY id ASC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| internal_worker_error("missing incomes bootstrap row"))?;

    let nexusggr_increment = calculate_nexusggr_increment(payload.amount, income.ggr)?;
    let income_increment = payload
        .amount
        .checked_sub(1800)
        .ok_or_else(|| internal_worker_error("qris nexusggr topup income underflow"))?;

    let note_json = merge_topup_note(locked_transaction.note.as_deref(), payload, &payload.trx_id)?;

    let updated_transaction = sqlx::query(
        r#"
        UPDATE transactions
        SET status = 'success',
            amount = $2,
            player = $3,
            note = $4,
            updated_at = NOW()
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(locked_transaction.id)
    .bind(payload.amount)
    .bind(&payload.terminal_id)
    .bind(note_json)
    .execute(&mut **transaction)
    .await?;
    if updated_transaction.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update qris topup transaction {} as success",
            locked_transaction.id
        )));
    }

    let updated_balance = sqlx::query(
        r#"
        UPDATE balances
        SET nexusggr = nexusggr + $2,
            updated_at = NOW()
        WHERE toko_id = $1
        "#,
    )
    .bind(locked_transaction.toko_id)
    .bind(nexusggr_increment)
    .execute(&mut **transaction)
    .await?;
    if updated_balance.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update nexusggr balance for toko {}",
            locked_transaction.toko_id
        )));
    }

    let updated_income = sqlx::query(
        r#"
        UPDATE incomes
        SET amount = amount + $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(income.id)
    .bind(income_increment)
    .execute(&mut **transaction)
    .await?;
    if updated_income.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update income row {}",
            income.id
        )));
    }

    Ok(())
}

fn parse_existing_note(
    note: Option<&str>,
    trx_id: &str,
) -> WorkerResult<ExistingQrisTransactionNote> {
    let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) else {
        return Ok(ExistingQrisTransactionNote::default());
    };

    serde_json::from_str(note).map_err(|error| {
        internal_worker_error(format!(
            "invalid qris transaction note for trx_id {trx_id}: {error}"
        ))
    })
}

fn calculate_fee(amount: i64, fee_transaction: i64) -> WorkerResult<i64> {
    if amount < 0 {
        return Err(internal_worker_error(
            "qris webhook amount cannot be negative during processing",
        ));
    }

    if fee_transaction < 0 {
        return Err(internal_worker_error(
            "income fee_transaction cannot be negative",
        ));
    }

    let multiplied = amount
        .checked_mul(fee_transaction)
        .ok_or_else(|| internal_worker_error("qris fee multiplication overflow"))?;

    Ok(multiplied / 100)
}

fn calculate_nexusggr_increment(amount: i64, ggr: i64) -> WorkerResult<i64> {
    if amount < 0 {
        return Err(internal_worker_error(
            "qris webhook amount cannot be negative during nexusggr topup processing",
        ));
    }

    if ggr <= 0 {
        return Err(internal_worker_error(
            "income ggr must be greater than 0 for nexusggr topup conversion",
        ));
    }

    let scaled = amount
        .checked_mul(100)
        .ok_or_else(|| internal_worker_error("qris nexusggr conversion overflow"))?;
    let rounded = ((scaled as i128) + ((ggr as i128) / 2)) / (ggr as i128);
    i64::try_from(rounded)
        .map_err(|_| internal_worker_error("qris nexusggr conversion result out of range"))
}

fn merge_topup_note(
    note: Option<&str>,
    payload: &SanitizedQrisWebhookPayload,
    trx_id: &str,
) -> WorkerResult<String> {
    let mut object = match note.map(str::trim).filter(|note| !note.is_empty()) {
        Some(note) => match serde_json::from_str::<JsonValue>(note) {
            Ok(JsonValue::Object(object)) => object,
            Ok(_) => {
                return Err(internal_worker_error(format!(
                    "invalid qris topup note object for trx_id {trx_id}"
                )));
            }
            Err(error) => {
                return Err(internal_worker_error(format!(
                    "invalid qris topup note json for trx_id {trx_id}: {error}"
                )));
            }
        },
        None => JsonMap::new(),
    };

    insert_optional_json_string(&mut object, "rrn", payload.rrn.as_deref());
    insert_optional_json_string(&mut object, "vendor", payload.vendor.as_deref());
    insert_optional_json_string(&mut object, "finish_at", payload.finish_at.as_deref());

    serde_json::to_string(&object).map_err(|error| {
        internal_worker_error(format!(
            "failed to serialize merged qris topup note for trx_id {trx_id}: {error}"
        ))
    })
}

fn insert_optional_json_string(
    object: &mut JsonMap<String, JsonValue>,
    key: &str,
    value: Option<&str>,
) {
    object.insert(
        key.to_string(),
        value
            .map(|value| JsonValue::String(value.to_string()))
            .unwrap_or(JsonValue::Null),
    );
}

fn internal_worker_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

fn build_qris_callback_job(
    transaction: &LockedQrisTransaction,
    payload: &SanitizedQrisWebhookPayload,
) -> WorkerResult<Option<SendTokoCallbackJob>> {
    let Some(callback_url) = transaction
        .callback_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let callback_payload = QrisCallbackPayload {
        amount: payload.amount,
        terminal_id: payload.terminal_id.clone(),
        trx_id: payload.trx_id.clone(),
        rrn: payload.rrn.clone(),
        custom_ref: payload.custom_ref.clone(),
        vendor: payload.vendor.clone(),
        status: payload.status.clone(),
        created_at: payload.created_at.clone(),
        finish_at: payload.finish_at.clone(),
    };
    let callback_payload = serde_json::to_value(callback_payload)?;

    Ok(Some(SendTokoCallbackJob::new(
        "qris",
        payload.trx_id.clone(),
        callback_url.to_string(),
        callback_payload,
    )))
}

fn sanitize_payload(payload: QrisWebhookPayload) -> SanitizedQrisWebhookPayload {
    let _ = payload.merchant_id;

    SanitizedQrisWebhookPayload {
        amount: payload.amount,
        terminal_id: payload.terminal_id,
        trx_id: payload.trx_id,
        rrn: payload.rrn,
        custom_ref: payload.custom_ref,
        vendor: payload.vendor,
        status: payload.status,
        created_at: payload.created_at,
        finish_at: payload.finish_at,
    }
}

#[cfg(test)]
mod tests {
    use super::{process_job, SanitizedQrisWebhookPayload};
    use crate::jobs::test_support::worker_db_test_lock;
    use serde_json::Value;
    use sqlx::postgres::{PgPool, PgPoolOptions};

    #[derive(Debug)]
    struct FixtureIds {
        user_id: i64,
        toko_id: i64,
        transaction_id: i64,
    }

    #[derive(Debug)]
    struct IncomeSnapshot {
        id: i64,
        ggr: i64,
        fee_transaction: i64,
        fee_withdrawal: i64,
        amount: i64,
        created: bool,
    }

    async fn test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });

        PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .expect("postgres pool")
    }

    fn unique_suffix() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        )
    }

    async fn snapshot_income(db: &PgPool) -> IncomeSnapshot {
        if let Some((id, ggr, fee_transaction, fee_withdrawal, amount)) =
            sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
                "SELECT id, ggr, fee_transaction, fee_withdrawal, amount FROM incomes ORDER BY id ASC LIMIT 1",
            )
            .fetch_optional(db)
            .await
            .expect("select incomes")
        {
            IncomeSnapshot {
                id,
                ggr,
                fee_transaction,
                fee_withdrawal,
                amount,
                created: false,
            }
        } else {
            let id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO incomes (ggr, fee_transaction, fee_withdrawal, amount)
                VALUES (10, 2, 14, 0)
                RETURNING id
                "#,
            )
            .fetch_one(db)
            .await
            .expect("insert incomes");

            IncomeSnapshot {
                id,
                ggr: 10,
                fee_transaction: 2,
                fee_withdrawal: 14,
                amount: 0,
                created: true,
            }
        }
    }

    async fn prepare_income(db: &PgPool, snapshot: &IncomeSnapshot) {
        sqlx::query(
            r#"
            UPDATE incomes
            SET ggr = 10,
                fee_transaction = 2,
                fee_withdrawal = 14,
                amount = 0,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(snapshot.id)
        .execute(db)
        .await
        .expect("prepare incomes");
    }

    async fn restore_income(db: &PgPool, snapshot: IncomeSnapshot) {
        if snapshot.created {
            sqlx::query("DELETE FROM incomes WHERE id = $1")
                .bind(snapshot.id)
                .execute(db)
                .await
                .expect("delete incomes");
        } else {
            sqlx::query(
                r#"
                UPDATE incomes
                SET ggr = $2,
                    fee_transaction = $3,
                    fee_withdrawal = $4,
                    amount = $5,
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(snapshot.id)
            .bind(snapshot.ggr)
            .bind(snapshot.fee_transaction)
            .bind(snapshot.fee_withdrawal)
            .bind(snapshot.amount)
            .execute(db)
            .await
            .expect("restore incomes");
        }
    }

    async fn insert_qris_fixture(
        db: &PgPool,
        transaction_code: &str,
        callback_url: Option<&str>,
        note_json: &str,
    ) -> FixtureIds {
        let suffix = unique_suffix();
        let username = format!("test_qris_worker_{suffix}");
        let email = format!("{username}@localhost");

        let user_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO users (username, name, email, password, role, is_active)
            VALUES ($1, $2, $3, $4, $5, true)
            RETURNING id
            "#,
        )
        .bind(&username)
        .bind("Test QRIS Worker User")
        .bind(&email)
        .bind("not-used")
        .bind("dev")
        .fetch_one(db)
        .await
        .expect("insert user");

        let toko_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO tokos (user_id, name, callback_url, token, is_active)
            VALUES ($1, $2, $3, $4, true)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind("Test QRIS Worker Toko")
        .bind(callback_url)
        .bind("test-qris-worker-token")
        .fetch_one(db)
        .await
        .expect("insert toko");

        let transaction_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO transactions (toko_id, category, type, status, amount, code, note)
            VALUES ($1, 'qris', 'deposit', 'pending', 0, $2, $3)
            RETURNING id
            "#,
        )
        .bind(toko_id)
        .bind(transaction_code)
        .bind(note_json)
        .fetch_one(db)
        .await
        .expect("insert transaction");

        FixtureIds {
            user_id,
            toko_id,
            transaction_id,
        }
    }

    async fn cleanup_fixture(db: &PgPool, ids: &FixtureIds) {
        sqlx::query("DELETE FROM balances WHERE toko_id = $1")
            .bind(ids.toko_id)
            .execute(db)
            .await
            .expect("delete balances");
        sqlx::query("DELETE FROM transactions WHERE id = $1")
            .bind(ids.transaction_id)
            .execute(db)
            .await
            .expect("delete transaction");
        sqlx::query("DELETE FROM tokos WHERE id = $1")
            .bind(ids.toko_id)
            .execute(db)
            .await
            .expect("delete toko");
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(ids.user_id)
            .execute(db)
            .await
            .expect("delete user");
    }

    #[tokio::test]
    async fn missing_transaction_returns_none_without_side_effects() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;
        let balance_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM balances")
            .fetch_one(&db)
            .await
            .expect("count balances before");
        let transaction_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions")
            .fetch_one(&db)
            .await
            .expect("count transactions before");

        let payload = SanitizedQrisWebhookPayload {
            amount: 10_000,
            terminal_id: "TERM-MISSING-01".to_string(),
            trx_id: format!("test-qris-missing-{}", unique_suffix()),
            rrn: Some("RRN-MISSING".to_string()),
            custom_ref: Some("REF-MISSING".to_string()),
            vendor: Some("qris-test".to_string()),
            status: "success".to_string(),
            created_at: Some("2026-04-08T03:55:00+07:00".to_string()),
            finish_at: Some("2026-04-08T03:56:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process qris missing transaction");
        let balance_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM balances")
            .fetch_one(&db)
            .await
            .expect("count balances after");
        let transaction_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transactions")
            .fetch_one(&db)
            .await
            .expect("count transactions after");
        let income_amount: i64 = sqlx::query_scalar("SELECT amount FROM incomes WHERE id = $1")
            .bind(income_snapshot.id)
            .fetch_one(&db)
            .await
            .expect("select income amount");

        assert!(callback_job.is_none());
        assert_eq!(balance_count_after, balance_count_before);
        assert_eq!(transaction_count_after, transaction_count_before);
        assert_eq!(income_amount, 0);

        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn non_terminal_status_leaves_transaction_pending_and_skips_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;

        let trx_id = format!("test-qris-pending-{}", unique_suffix());
        let fixture = insert_qris_fixture(
            &db,
            &trx_id,
            Some("https://callback.test/qris"),
            r#"{"purpose":"generate","custom_ref":"REF-PENDING"}"#,
        )
        .await;

        let payload = SanitizedQrisWebhookPayload {
            amount: 10_000,
            terminal_id: "TERM-PENDING-01".to_string(),
            trx_id: trx_id.clone(),
            rrn: Some("RRN-PENDING".to_string()),
            custom_ref: Some("REF-PENDING".to_string()),
            vendor: Some("qris-test".to_string()),
            status: "pending".to_string(),
            created_at: Some("2026-04-08T03:55:00+07:00".to_string()),
            finish_at: Some("2026-04-08T03:56:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process qris pending");

        let transaction_row: (String, i64, Option<String>, String) = sqlx::query_as(
            r#"
            SELECT status, amount, player, note
            FROM transactions
            WHERE id = $1
            "#,
        )
        .bind(fixture.transaction_id)
        .fetch_one(&db)
        .await
        .expect("select transaction");
        let balance_row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT pending, settle, nexusggr FROM balances WHERE toko_id = $1 LIMIT 1",
        )
        .bind(fixture.toko_id)
        .fetch_optional(&db)
        .await
        .expect("select balance");
        let income_amount: i64 = sqlx::query_scalar("SELECT amount FROM incomes WHERE id = $1")
            .bind(income_snapshot.id)
            .fetch_one(&db)
            .await
            .expect("select income amount");

        assert!(callback_job.is_none());
        assert_eq!(transaction_row.0, "pending");
        assert_eq!(transaction_row.1, 0);
        assert!(transaction_row.2.is_none());
        assert_eq!(
            transaction_row.3,
            r#"{"purpose":"generate","custom_ref":"REF-PENDING"}"#
        );
        assert_eq!(balance_row, None);
        assert_eq!(income_amount, 0);

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn already_success_transaction_skips_mutation_and_still_builds_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;

        let trx_id = format!("test-qris-replay-{}", unique_suffix());
        let fixture = insert_qris_fixture(
            &db,
            &trx_id,
            Some("https://callback.test/qris"),
            r#"{"purpose":"generate","custom_ref":"REF-REPLAY"}"#,
        )
        .await;

        sqlx::query(
            r#"
            UPDATE transactions
            SET status = 'success',
                amount = 10_000,
                player = 'TERM-REPLAY-01',
                note = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(fixture.transaction_id)
        .bind(r#"{"rrn":"RRN-REPLAY","vendor":"qris-test","custom_ref":"REF-REPLAY","finish_at":"2026-04-08T03:56:00+07:00"}"#)
        .execute(&db)
        .await
        .expect("update transaction success");

        let payload = SanitizedQrisWebhookPayload {
            amount: 10_000,
            terminal_id: "TERM-REPLAY-01".to_string(),
            trx_id: trx_id.clone(),
            rrn: Some("RRN-REPLAY".to_string()),
            custom_ref: Some("REF-REPLAY".to_string()),
            vendor: Some("qris-test".to_string()),
            status: "success".to_string(),
            created_at: Some("2026-04-08T03:55:00+07:00".to_string()),
            finish_at: Some("2026-04-08T03:56:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process qris replay")
            .expect("callback job");

        let transaction_row: (String, i64, String, String) = sqlx::query_as(
            r#"
            SELECT status, amount, player, note
            FROM transactions
            WHERE id = $1
            "#,
        )
        .bind(fixture.transaction_id)
        .fetch_one(&db)
        .await
        .expect("select transaction");
        let balance_row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT pending, settle, nexusggr FROM balances WHERE toko_id = $1 LIMIT 1",
        )
        .bind(fixture.toko_id)
        .fetch_optional(&db)
        .await
        .expect("select balance");
        let income_amount: i64 = sqlx::query_scalar("SELECT amount FROM incomes WHERE id = $1")
            .bind(income_snapshot.id)
            .fetch_one(&db)
            .await
            .expect("select income amount");

        assert_eq!(transaction_row.0, "success");
        assert_eq!(transaction_row.1, 10_000);
        assert_eq!(transaction_row.2, "TERM-REPLAY-01");
        assert_eq!(
            transaction_row.3,
            r#"{"rrn":"RRN-REPLAY","vendor":"qris-test","custom_ref":"REF-REPLAY","finish_at":"2026-04-08T03:56:00+07:00"}"#
        );
        assert_eq!(balance_row, None);
        assert_eq!(income_amount, 0);
        assert_eq!(callback_job.event_type, "qris");
        assert_eq!(callback_job.reference, trx_id);

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn regular_deposit_success_updates_pending_and_income_and_builds_sanitized_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;

        let trx_id = format!("test-qris-generate-{}", unique_suffix());
        let fixture = insert_qris_fixture(
            &db,
            &trx_id,
            Some("https://callback.test/qris"),
            r#"{"purpose":"generate","custom_ref":"REF-001"}"#,
        )
        .await;

        let payload = SanitizedQrisWebhookPayload {
            amount: 10_000,
            terminal_id: "TERM-REG-01".to_string(),
            trx_id: trx_id.clone(),
            rrn: Some("RRN-0001".to_string()),
            custom_ref: Some("REF-001".to_string()),
            vendor: Some("qris-test".to_string()),
            status: "success".to_string(),
            created_at: Some("2026-04-08T03:55:00+07:00".to_string()),
            finish_at: Some("2026-04-08T03:56:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process qris generate")
            .expect("callback job");

        let (status, amount, player, note, pending, nexusggr, income_amount): (
            String,
            i64,
            String,
            String,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            r#"
            SELECT
                (SELECT status FROM transactions WHERE id = $1),
                (SELECT amount FROM transactions WHERE id = $1),
                (SELECT player FROM transactions WHERE id = $1),
                (SELECT note FROM transactions WHERE id = $1),
                (SELECT pending FROM balances WHERE toko_id = $2),
                (SELECT nexusggr FROM balances WHERE toko_id = $2),
                (SELECT amount FROM incomes WHERE id = $3)
            "#,
        )
        .bind(fixture.transaction_id)
        .bind(fixture.toko_id)
        .bind(income_snapshot.id)
        .fetch_one(&db)
        .await
        .expect("load updated rows");

        assert_eq!(status, "success");
        assert_eq!(amount, 10_000);
        assert_eq!(player, "TERM-REG-01");
        assert_eq!(pending, 9_800);
        assert_eq!(nexusggr, 0);
        assert_eq!(income_amount, 200);

        let note_json: Value = serde_json::from_str(&note).expect("note json");
        assert_eq!(note_json["rrn"], Value::String("RRN-0001".to_string()));
        assert_eq!(note_json["vendor"], Value::String("qris-test".to_string()));
        assert_eq!(
            note_json["custom_ref"],
            Value::String("REF-001".to_string())
        );
        assert_eq!(
            note_json["finish_at"],
            Value::String("2026-04-08T03:56:00+07:00".to_string())
        );
        assert!(note_json.get("purpose").is_none());

        assert_eq!(callback_job.event_type, "qris");
        assert_eq!(callback_job.reference, trx_id);
        assert_eq!(callback_job.callback_url, "https://callback.test/qris");
        assert_eq!(callback_job.payload["amount"], Value::Number(10_000.into()));
        assert_eq!(
            callback_job.payload["terminal_id"],
            Value::String("TERM-REG-01".to_string())
        );
        assert_eq!(
            callback_job.payload["trx_id"],
            Value::String(trx_id.clone())
        );
        assert_eq!(
            callback_job.payload["rrn"],
            Value::String("RRN-0001".to_string())
        );
        assert_eq!(
            callback_job.payload["custom_ref"],
            Value::String("REF-001".to_string())
        );
        assert_eq!(
            callback_job.payload["vendor"],
            Value::String("qris-test".to_string())
        );
        assert_eq!(
            callback_job.payload["created_at"],
            Value::String("2026-04-08T03:55:00+07:00".to_string())
        );
        assert_eq!(
            callback_job.payload["finish_at"],
            Value::String("2026-04-08T03:56:00+07:00".to_string())
        );
        assert!(callback_job.payload.get("merchant_id").is_none());
        assert!(callback_job.payload.get("purpose").is_none());

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn nexusggr_topup_success_updates_balance_and_income_and_builds_sanitized_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;

        let trx_id = format!("test-qris-topup-{}", unique_suffix());
        let fixture = insert_qris_fixture(
            &db,
            &trx_id,
            Some("https://callback.test/qris"),
            r#"{"purpose":"nexusggr_topup","inquiry_id":"INQ123","platform_fee":1800}"#,
        )
        .await;

        let payload = SanitizedQrisWebhookPayload {
            amount: 10_000,
            terminal_id: "TERM-TOPUP-01".to_string(),
            trx_id: trx_id.clone(),
            rrn: Some("RRN-0002".to_string()),
            custom_ref: Some("TOPUP-001".to_string()),
            vendor: Some("qris-test".to_string()),
            status: "success".to_string(),
            created_at: Some("2026-04-08T04:00:00+07:00".to_string()),
            finish_at: Some("2026-04-08T04:01:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process qris topup")
            .expect("callback job");

        let (status, amount, player, note, pending, nexusggr, income_amount): (
            String,
            i64,
            String,
            String,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            r#"
            SELECT
                (SELECT status FROM transactions WHERE id = $1),
                (SELECT amount FROM transactions WHERE id = $1),
                (SELECT player FROM transactions WHERE id = $1),
                (SELECT note FROM transactions WHERE id = $1),
                (SELECT pending FROM balances WHERE toko_id = $2),
                (SELECT nexusggr FROM balances WHERE toko_id = $2),
                (SELECT amount FROM incomes WHERE id = $3)
            "#,
        )
        .bind(fixture.transaction_id)
        .bind(fixture.toko_id)
        .bind(income_snapshot.id)
        .fetch_one(&db)
        .await
        .expect("load updated rows");

        assert_eq!(status, "success");
        assert_eq!(amount, 10_000);
        assert_eq!(player, "TERM-TOPUP-01");
        assert_eq!(pending, 0);
        assert_eq!(nexusggr, 100_000);
        assert_eq!(income_amount, 8_200);

        let note_json: Value = serde_json::from_str(&note).expect("note json");
        assert_eq!(
            note_json["purpose"],
            Value::String("nexusggr_topup".to_string())
        );
        assert_eq!(note_json["inquiry_id"], Value::String("INQ123".to_string()));
        assert_eq!(note_json["platform_fee"], Value::Number(1800.into()));
        assert_eq!(note_json["rrn"], Value::String("RRN-0002".to_string()));
        assert_eq!(note_json["vendor"], Value::String("qris-test".to_string()));
        assert_eq!(
            note_json["finish_at"],
            Value::String("2026-04-08T04:01:00+07:00".to_string())
        );

        assert_eq!(callback_job.event_type, "qris");
        assert_eq!(callback_job.reference, trx_id);
        assert_eq!(callback_job.callback_url, "https://callback.test/qris");
        assert_eq!(callback_job.payload["amount"], Value::Number(10_000.into()));
        assert_eq!(
            callback_job.payload["terminal_id"],
            Value::String("TERM-TOPUP-01".to_string())
        );
        assert_eq!(
            callback_job.payload["trx_id"],
            Value::String(trx_id.clone())
        );
        assert_eq!(
            callback_job.payload["rrn"],
            Value::String("RRN-0002".to_string())
        );
        assert_eq!(
            callback_job.payload["custom_ref"],
            Value::String("TOPUP-001".to_string())
        );
        assert_eq!(
            callback_job.payload["vendor"],
            Value::String("qris-test".to_string())
        );
        assert!(callback_job.payload.get("merchant_id").is_none());
        assert!(callback_job.payload.get("inquiry_id").is_none());
        assert!(callback_job.payload.get("platform_fee").is_none());

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }
}
