use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::jobs::send_toko_callback::SendTokoCallbackJob;

const PROCESS_DISBURSEMENT_QUEUE_NAME: &str = "process_disbursement";
const DISBURSEMENT_SUCCESS_STATUS: &str = "success";
const DISBURSEMENT_FAILED_STATUS: &str = "failed";

#[derive(Debug, Clone, Deserialize, Serialize)]
struct DisbursementWebhookPayload {
    amount: i64,
    partner_ref_no: String,
    status: String,
    transaction_date: Option<String>,
    merchant_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProcessDisbursementWebhookJob {
    event_type: String,
    received_at: String,
    payload: DisbursementWebhookPayload,
}

#[derive(Debug)]
struct SanitizedDisbursementWebhookPayload {
    amount: i64,
    partner_ref_no: String,
    status: String,
    transaction_date: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedDisbursementTransaction {
    id: i64,
    toko_id: i64,
    status: String,
    amount: i64,
    note: Option<String>,
    callback_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExistingDisbursementNote {
    platform_fee: Option<i64>,
    fee: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedIncome {
    id: i64,
}

#[derive(Debug, Serialize)]
struct DisbursementCallbackPayload {
    amount: i64,
    partner_ref_no: String,
    status: String,
    transaction_date: Option<String>,
}

type WorkerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn run_once(db: &sqlx::PgPool, redis: &redis::Client) -> bool {
    let job = match justqiu_redis::dequeue_json::<ProcessDisbursementWebhookJob>(
        redis,
        PROCESS_DISBURSEMENT_QUEUE_NAME,
    )
    .await
    {
        Ok(job) => job,
        Err(error) => {
            tracing::error!(error = %error, "failed to dequeue process_disbursement job");
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
                        partner_ref_no = %payload.partner_ref_no,
                        "failed to enqueue toko callback job after disbursement processing"
                    );
                    if let Err(requeue_error) =
                        justqiu_redis::enqueue_json(redis, PROCESS_DISBURSEMENT_QUEUE_NAME, &job)
                            .await
                    {
                        tracing::error!(
                            error = %requeue_error,
                            partner_ref_no = %job.payload.partner_ref_no,
                            "failed to requeue process_disbursement job after callback enqueue failure"
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
                partner_ref_no = %payload.partner_ref_no,
                status = %payload.status,
                "failed to process disbursement webhook job"
            );
            if let Err(requeue_error) =
                justqiu_redis::enqueue_json(redis, PROCESS_DISBURSEMENT_QUEUE_NAME, &job).await
            {
                tracing::error!(
                    error = %requeue_error,
                    partner_ref_no = %job.payload.partner_ref_no,
                    "failed to requeue process_disbursement job after database failure"
                );
            }
            true
        }
    }
}

async fn process_job(
    db: &sqlx::PgPool,
    payload: &SanitizedDisbursementWebhookPayload,
) -> WorkerResult<Option<SendTokoCallbackJob>> {
    let normalized_status = normalize_status(&payload.status);
    if normalized_status != DISBURSEMENT_SUCCESS_STATUS
        && normalized_status != DISBURSEMENT_FAILED_STATUS
    {
        tracing::info!(
            partner_ref_no = %payload.partner_ref_no,
            status = %payload.status,
            normalized_status = %normalized_status,
            "disbursement webhook status not handled in process_disbursement; skipping"
        );
        return Ok(None);
    }

    let mut transaction = db.begin().await?;

    let locked_transaction = sqlx::query_as::<_, LockedDisbursementTransaction>(
        r#"
        SELECT transactions.id,
               transactions.toko_id,
               transactions.status,
               transactions.amount,
               transactions.note,
               tokos.callback_url
        FROM transactions
        JOIN tokos ON tokos.id = transactions.toko_id
        WHERE transactions.code = $1
          AND transactions.category = 'qris'
          AND transactions.type = 'withdrawal'
          AND transactions.deleted_at IS NULL
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(&payload.partner_ref_no)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(locked_transaction) = locked_transaction else {
        tracing::warn!(
            partner_ref_no = %payload.partner_ref_no,
            status = %normalized_status,
            amount = payload.amount,
            transaction_date = ?payload.transaction_date,
            "pending disbursement transaction not found during process_disbursement"
        );
        transaction.rollback().await?;
        return Ok(None);
    };

    if locked_transaction.status != "pending" {
        tracing::info!(
            partner_ref_no = %payload.partner_ref_no,
            transaction_id = locked_transaction.id,
            toko_id = locked_transaction.toko_id,
            transaction_status = %locked_transaction.status,
            "disbursement transaction already processed; skipping process_disbursement"
        );
        transaction.rollback().await?;
        return Ok(build_disbursement_callback_job(
            &locked_transaction,
            payload,
            &normalized_status,
        )?);
    }

    let existing_note =
        parse_existing_note(locked_transaction.note.as_deref(), &payload.partner_ref_no)?;

    match normalized_status.as_str() {
        DISBURSEMENT_SUCCESS_STATUS => {
            process_success(
                &mut transaction,
                &locked_transaction,
                &existing_note,
                payload,
            )
            .await?;
        }
        DISBURSEMENT_FAILED_STATUS => {
            process_failed(
                &mut transaction,
                &locked_transaction,
                &existing_note,
                payload,
            )
            .await?;
        }
        _ => unreachable!(),
    }

    transaction.commit().await?;

    tracing::info!(
        partner_ref_no = %payload.partner_ref_no,
        transaction_id = locked_transaction.id,
        toko_id = locked_transaction.toko_id,
        status = %normalized_status,
        amount = payload.amount,
        transaction_date = ?payload.transaction_date,
        "processed disbursement callback"
    );

    Ok(build_disbursement_callback_job(
        &locked_transaction,
        payload,
        &normalized_status,
    )?)
}

async fn process_success(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    locked_transaction: &LockedDisbursementTransaction,
    existing_note: &ExistingDisbursementNote,
    payload: &SanitizedDisbursementWebhookPayload,
) -> WorkerResult<()> {
    ensure_income_row(transaction).await?;

    let income = sqlx::query_as::<_, LockedIncome>(
        r#"
        SELECT id
        FROM incomes
        ORDER BY id ASC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| {
        internal_worker_error("missing incomes bootstrap row during disbursement success")
    })?;

    let note_json = merge_transaction_date_note(
        locked_transaction.note.as_deref(),
        payload,
        &payload.partner_ref_no,
    )?;

    let updated_transaction = sqlx::query(
        r#"
        UPDATE transactions
        SET status = $2,
            note = $3,
            updated_at = NOW()
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(locked_transaction.id)
    .bind(DISBURSEMENT_SUCCESS_STATUS)
    .bind(note_json)
    .execute(&mut **transaction)
    .await?;
    if updated_transaction.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to mark disbursement transaction {} as success",
            locked_transaction.id
        )));
    }

    let platform_fee = existing_note.platform_fee.unwrap_or(0).max(0);
    let updated_income = sqlx::query(
        r#"
        UPDATE incomes
        SET amount = amount + $2,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(income.id)
    .bind(platform_fee)
    .execute(&mut **transaction)
    .await?;
    if updated_income.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to update income row {} during disbursement success",
            income.id
        )));
    }

    Ok(())
}

async fn process_failed(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    locked_transaction: &LockedDisbursementTransaction,
    existing_note: &ExistingDisbursementNote,
    payload: &SanitizedDisbursementWebhookPayload,
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

    let note_json = merge_transaction_date_note(
        locked_transaction.note.as_deref(),
        payload,
        &payload.partner_ref_no,
    )?;

    let updated_transaction = sqlx::query(
        r#"
        UPDATE transactions
        SET status = $2,
            note = $3,
            updated_at = NOW()
        WHERE id = $1
          AND status = 'pending'
        "#,
    )
    .bind(locked_transaction.id)
    .bind(DISBURSEMENT_FAILED_STATUS)
    .bind(note_json)
    .execute(&mut **transaction)
    .await?;
    if updated_transaction.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to mark disbursement transaction {} as failed",
            locked_transaction.id
        )));
    }

    let platform_fee = existing_note.platform_fee.unwrap_or(0).max(0);
    let bank_fee = existing_note.fee.unwrap_or(0).max(0);
    let refund_amount = locked_transaction
        .amount
        .checked_add(platform_fee)
        .and_then(|value| value.checked_add(bank_fee))
        .ok_or_else(|| internal_worker_error("disbursement refund overflow"))?;

    let updated_balance = sqlx::query(
        r#"
        UPDATE balances
        SET settle = settle + $2,
            updated_at = NOW()
        WHERE toko_id = $1
        "#,
    )
    .bind(locked_transaction.toko_id)
    .bind(refund_amount)
    .execute(&mut **transaction)
    .await?;
    if updated_balance.rows_affected() != 1 {
        return Err(internal_worker_error(format!(
            "failed to refund settle balance for toko {}",
            locked_transaction.toko_id
        )));
    }

    Ok(())
}

async fn ensure_income_row(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> WorkerResult<()> {
    sqlx::query(
        r#"
        INSERT INTO incomes (ggr, fee_transaction, fee_withdrawal, amount)
        SELECT 0, 0, 0, 0
        WHERE NOT EXISTS (SELECT 1 FROM incomes)
        "#,
    )
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

fn parse_existing_note(
    note: Option<&str>,
    partner_ref_no: &str,
) -> WorkerResult<ExistingDisbursementNote> {
    let Some(note) = note.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(ExistingDisbursementNote::default());
    };

    serde_json::from_str(note).map_err(|error| {
        internal_worker_error(format!(
            "invalid disbursement transaction note for partner_ref_no {partner_ref_no}: {error}"
        ))
    })
}

fn merge_transaction_date_note(
    note: Option<&str>,
    payload: &SanitizedDisbursementWebhookPayload,
    partner_ref_no: &str,
) -> WorkerResult<String> {
    let mut object = match note.map(str::trim).filter(|value| !value.is_empty()) {
        Some(note) => match serde_json::from_str::<JsonValue>(note) {
            Ok(JsonValue::Object(object)) => object,
            Ok(_) => {
                return Err(internal_worker_error(format!(
                    "invalid disbursement note object for partner_ref_no {partner_ref_no}"
                )));
            }
            Err(error) => {
                return Err(internal_worker_error(format!(
                    "invalid disbursement note json for partner_ref_no {partner_ref_no}: {error}"
                )));
            }
        },
        None => JsonMap::new(),
    };

    object.insert(
        "transaction_date".to_string(),
        payload
            .transaction_date
            .as_ref()
            .map(|value| JsonValue::String(value.clone()))
            .unwrap_or(JsonValue::Null),
    );

    serde_json::to_string(&object).map_err(|error| {
        internal_worker_error(format!(
            "failed to serialize disbursement note for partner_ref_no {partner_ref_no}: {error}"
        ))
    })
}

fn build_disbursement_callback_job(
    transaction: &LockedDisbursementTransaction,
    payload: &SanitizedDisbursementWebhookPayload,
    normalized_status: &str,
) -> WorkerResult<Option<SendTokoCallbackJob>> {
    let Some(callback_url) = transaction
        .callback_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let callback_payload = DisbursementCallbackPayload {
        amount: payload.amount,
        partner_ref_no: payload.partner_ref_no.clone(),
        status: normalized_status.to_string(),
        transaction_date: payload.transaction_date.clone(),
    };
    let callback_payload = serde_json::to_value(callback_payload)?;

    Ok(Some(SendTokoCallbackJob::new(
        "disbursement",
        payload.partner_ref_no.clone(),
        callback_url.to_string(),
        callback_payload,
    )))
}

fn normalize_status(status: &str) -> String {
    status.trim().to_ascii_lowercase()
}

fn internal_worker_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

fn sanitize_payload(payload: DisbursementWebhookPayload) -> SanitizedDisbursementWebhookPayload {
    let _ = payload.merchant_id;

    SanitizedDisbursementWebhookPayload {
        amount: payload.amount,
        partner_ref_no: payload.partner_ref_no,
        status: payload.status,
        transaction_date: payload.transaction_date,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        process_job, DisbursementWebhookPayload, ExistingDisbursementNote,
        SanitizedDisbursementWebhookPayload,
    };
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
        if let Some((id, amount)) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT id, amount FROM incomes ORDER BY id ASC LIMIT 1",
        )
        .fetch_optional(db)
        .await
        .expect("select incomes")
        {
            IncomeSnapshot {
                id,
                amount,
                created: false,
            }
        } else {
            let id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO incomes (ggr, fee_transaction, fee_withdrawal, amount)
                VALUES (0, 0, 0, 0)
                RETURNING id
                "#,
            )
            .fetch_one(db)
            .await
            .expect("insert incomes");

            IncomeSnapshot {
                id,
                amount: 0,
                created: true,
            }
        }
    }

    async fn restore_income(db: &PgPool, snapshot: IncomeSnapshot) {
        if snapshot.created {
            sqlx::query("DELETE FROM incomes WHERE id = $1")
                .bind(snapshot.id)
                .execute(db)
                .await
                .expect("delete incomes");
        } else {
            sqlx::query("UPDATE incomes SET amount = $2, updated_at = NOW() WHERE id = $1")
                .bind(snapshot.id)
                .bind(snapshot.amount)
                .execute(db)
                .await
                .expect("restore incomes");
        }
    }

    async fn prepare_income(db: &PgPool, snapshot: &IncomeSnapshot) {
        sqlx::query("UPDATE incomes SET amount = 0, updated_at = NOW() WHERE id = $1")
            .bind(snapshot.id)
            .execute(db)
            .await
            .expect("prepare incomes");
    }

    async fn insert_disbursement_fixture(
        db: &PgPool,
        transaction_code: &str,
        callback_url: Option<&str>,
        amount: i64,
        note_json: &str,
    ) -> FixtureIds {
        let suffix = unique_suffix();
        let username = format!("test_disb_worker_{suffix}");
        let email = format!("{username}@localhost");

        let user_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO users (username, name, email, password, role, is_active)
            VALUES ($1, $2, $3, $4, $5, true)
            RETURNING id
            "#,
        )
        .bind(&username)
        .bind("Test Disbursement Worker User")
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
        .bind("Test Disbursement Worker Toko")
        .bind(callback_url)
        .bind("test-disbursement-worker-token")
        .fetch_one(db)
        .await
        .expect("insert toko");

        let transaction_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO transactions (toko_id, category, type, status, amount, code, note)
            VALUES ($1, 'qris', 'withdrawal', 'pending', $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(toko_id)
        .bind(amount)
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

        let payload = SanitizedDisbursementWebhookPayload {
            amount: 50_000,
            partner_ref_no: format!("test-disb-missing-{}", unique_suffix()),
            status: "success".to_string(),
            transaction_date: Some("2026-04-08T04:14:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process missing disbursement transaction");
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
        let partner_ref_no = format!("test-disb-pending-{}", unique_suffix());
        let fixture = insert_disbursement_fixture(
            &db,
            &partner_ref_no,
            Some("https://callback.test/disbursement"),
            50_000,
            r#"{"platform_fee":1500,"fee":2500}"#,
        )
        .await;

        let payload = SanitizedDisbursementWebhookPayload {
            amount: 50_000,
            partner_ref_no: partner_ref_no.clone(),
            status: "processing".to_string(),
            transaction_date: Some("2026-04-08T04:14:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process non terminal disbursement");

        let transaction_row: (String, String) = sqlx::query_as(
            r#"
            SELECT status, note
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
        assert_eq!(transaction_row.1, r#"{"platform_fee":1500,"fee":2500}"#);
        assert_eq!(balance_row, None);
        assert_eq!(income_amount, 0);

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn already_terminal_transaction_skips_mutation_and_still_builds_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;
        let partner_ref_no = format!("test-disb-replay-{}", unique_suffix());
        let fixture = insert_disbursement_fixture(
            &db,
            &partner_ref_no,
            Some("https://callback.test/disbursement"),
            50_000,
            r#"{"platform_fee":1500,"fee":2500,"transaction_date":"2026-04-08T04:14:00+07:00"}"#,
        )
        .await;

        sqlx::query(
            r#"
            UPDATE transactions
            SET status = 'failed',
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(fixture.transaction_id)
        .execute(&db)
        .await
        .expect("update transaction failed");

        let payload = SanitizedDisbursementWebhookPayload {
            amount: 50_000,
            partner_ref_no: partner_ref_no.clone(),
            status: "failed".to_string(),
            transaction_date: Some("2026-04-08T04:14:30+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process terminal disbursement")
            .expect("callback job");

        let transaction_row: (String, String) = sqlx::query_as(
            r#"
            SELECT status, note
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

        assert_eq!(transaction_row.0, "failed");
        assert_eq!(
            transaction_row.1,
            r#"{"platform_fee":1500,"fee":2500,"transaction_date":"2026-04-08T04:14:00+07:00"}"#
        );
        assert_eq!(balance_row, None);
        assert_eq!(income_amount, 0);
        assert_eq!(callback_job.event_type, "disbursement");
        assert_eq!(callback_job.reference, partner_ref_no);
        assert_eq!(
            callback_job.payload["status"],
            Value::String("failed".to_string())
        );

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn success_updates_income_and_builds_sanitized_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;
        let partner_ref_no = format!("test-disb-success-{}", unique_suffix());
        let fixture = insert_disbursement_fixture(
            &db,
            &partner_ref_no,
            Some("https://callback.test/disbursement"),
            50_000,
            r#"{"platform_fee":1500,"fee":2500,"internal":"keep"}"#,
        )
        .await;

        let payload = SanitizedDisbursementWebhookPayload {
            amount: 50_000,
            partner_ref_no: partner_ref_no.clone(),
            status: "SUCCESS".to_string(),
            transaction_date: Some("2026-04-08T04:14:00+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process success")
            .expect("callback job");

        let (status, note, income_amount): (String, String, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT status FROM transactions WHERE id = $1),
                (SELECT note FROM transactions WHERE id = $1),
                (SELECT amount FROM incomes WHERE id = $2)
            "#,
        )
        .bind(fixture.transaction_id)
        .bind(income_snapshot.id)
        .fetch_one(&db)
        .await
        .expect("load updated rows");

        assert_eq!(status, "success");
        assert_eq!(income_amount, income_snapshot.amount + 1_500);

        let note_json: Value = serde_json::from_str(&note).expect("note json");
        assert_eq!(
            note_json["transaction_date"],
            Value::String("2026-04-08T04:14:00+07:00".to_string())
        );
        assert_eq!(note_json["platform_fee"], Value::Number(1500.into()));
        assert_eq!(note_json["fee"], Value::Number(2500.into()));
        assert_eq!(note_json["internal"], Value::String("keep".to_string()));

        assert_eq!(callback_job.event_type, "disbursement");
        assert_eq!(callback_job.reference, partner_ref_no);
        assert_eq!(
            callback_job.callback_url,
            "https://callback.test/disbursement"
        );
        assert_eq!(callback_job.attempt, 1);
        assert!(callback_job.not_before.is_none());
        assert_eq!(callback_job.payload["amount"], Value::Number(50_000.into()));
        assert_eq!(
            callback_job.payload["status"],
            Value::String("success".to_string())
        );
        assert_eq!(
            callback_job.payload["partner_ref_no"],
            Value::String(partner_ref_no.clone())
        );
        assert_eq!(
            callback_job.payload["transaction_date"],
            Value::String("2026-04-08T04:14:00+07:00".to_string())
        );
        assert!(callback_job.payload.get("merchant_id").is_none());
        assert!(callback_job.payload.get("fee").is_none());
        assert!(callback_job.payload.get("platform_fee").is_none());

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[tokio::test]
    async fn failed_refunds_settle_and_builds_sanitized_callback() {
        let _guard = worker_db_test_lock().await;
        let db = test_db().await;
        let income_snapshot = snapshot_income(&db).await;
        prepare_income(&db, &income_snapshot).await;
        let partner_ref_no = format!("test-disb-failed-{}", unique_suffix());
        let fixture = insert_disbursement_fixture(
            &db,
            &partner_ref_no,
            Some("https://callback.test/disbursement"),
            50_000,
            r#"{"platform_fee":1500,"fee":2500}"#,
        )
        .await;

        sqlx::query(
            "INSERT INTO balances (toko_id, pending, settle, nexusggr) VALUES ($1, 0, 100000, 0)",
        )
        .bind(fixture.toko_id)
        .execute(&db)
        .await
        .expect("insert balance");

        let payload = SanitizedDisbursementWebhookPayload {
            amount: 50_000,
            partner_ref_no: partner_ref_no.clone(),
            status: "FAILED".to_string(),
            transaction_date: Some("2026-04-08T04:14:30+07:00".to_string()),
        };

        let callback_job = process_job(&db, &payload)
            .await
            .expect("process failed")
            .expect("callback job");

        let (status, note, settle, income_amount): (String, String, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT status FROM transactions WHERE id = $1),
                (SELECT note FROM transactions WHERE id = $1),
                (SELECT settle FROM balances WHERE toko_id = $2),
                (SELECT amount FROM incomes WHERE id = $3)
            "#,
        )
        .bind(fixture.transaction_id)
        .bind(fixture.toko_id)
        .bind(income_snapshot.id)
        .fetch_one(&db)
        .await
        .expect("load updated rows");

        assert_eq!(status, "failed");
        assert_eq!(settle, 154_000);
        assert_eq!(income_amount, income_snapshot.amount);

        let note_json: Value = serde_json::from_str(&note).expect("note json");
        assert_eq!(
            note_json["transaction_date"],
            Value::String("2026-04-08T04:14:30+07:00".to_string())
        );
        assert_eq!(note_json["platform_fee"], Value::Number(1500.into()));
        assert_eq!(note_json["fee"], Value::Number(2500.into()));

        assert_eq!(callback_job.event_type, "disbursement");
        assert_eq!(callback_job.reference, partner_ref_no);
        assert_eq!(
            callback_job.callback_url,
            "https://callback.test/disbursement"
        );
        assert_eq!(callback_job.payload["amount"], Value::Number(50_000.into()));
        assert_eq!(
            callback_job.payload["status"],
            Value::String("failed".to_string())
        );
        assert_eq!(
            callback_job.payload["partner_ref_no"],
            Value::String(partner_ref_no.clone())
        );
        assert_eq!(
            callback_job.payload["transaction_date"],
            Value::String("2026-04-08T04:14:30+07:00".to_string())
        );
        assert!(callback_job.payload.get("merchant_id").is_none());

        cleanup_fixture(&db, &fixture).await;
        restore_income(&db, income_snapshot).await;
    }

    #[test]
    fn existing_note_defaults_when_missing() {
        let note = serde_json::from_str::<ExistingDisbursementNote>("{}").expect("note");
        assert_eq!(note.platform_fee, None);
        assert_eq!(note.fee, None);
    }

    #[test]
    fn payload_sanitizer_drops_merchant_id() {
        let sanitized = super::sanitize_payload(DisbursementWebhookPayload {
            amount: 10_000,
            partner_ref_no: "ref-1".to_string(),
            status: "success".to_string(),
            transaction_date: Some("2026-04-08T00:00:00+07:00".to_string()),
            merchant_id: "MID-SECRET".to_string(),
        });

        assert_eq!(sanitized.amount, 10_000);
        assert_eq!(sanitized.partner_ref_no, "ref-1");
        assert_eq!(sanitized.status, "success");
        assert_eq!(
            sanitized.transaction_date.as_deref(),
            Some("2026-04-08T00:00:00+07:00")
        );
    }
}
