use std::collections::{HashMap, HashSet};

use axum::{
    extract::{rejection::JsonRejection, State},
    middleware as axum_middleware,
    routing::{get, post},
    Json, Router,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use justqiu_errors::{AppError, AppResult};

use crate::{
    app::AppState,
    extractors::authenticated_toko::AuthenticatedToko,
    middleware::{api_rate_limit::api_rate_limit, toko_auth::toko_auth},
};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/balance", get(balance))
        .route("/check-status", post(check_status))
        .route("/generate", post(generate))
        .route("/game/launch", post(game_launch))
        .route("/games", post(games))
        .route("/games/v2", post(games_v2))
        .route("/merchant-active", post(merchant_active))
        .route("/money/info", post(money_info))
        .route("/providers", get(providers))
        .route("/call/players", get(call_players))
        .route("/call/list", post(call_list))
        .route("/call/apply", post(call_apply))
        .route("/call/history", post(call_history))
        .route("/call/cancel", post(call_cancel))
        .route("/control/rtp", post(control_rtp))
        .route("/control/users-rtp", post(control_users_rtp))
        .route("/user/create", post(user_create))
        .route("/user/deposit", post(user_deposit))
        .route("/user/withdraw", post(user_withdraw))
        .route("/user/withdraw-reset", post(user_withdraw_reset))
        .route("/transfer/status", post(transfer_status))
        .route_layer(axum_middleware::from_fn_with_state(
            state.clone(),
            api_rate_limit,
        ))
        .route_layer(axum_middleware::from_fn_with_state(state, toko_auth))
}

#[derive(Debug, Serialize)]
struct BalanceResponse {
    success: bool,
    pending_balance: i64,
    settle_balance: i64,
    nexusggr_balance: i64,
}

#[derive(Debug, Serialize)]
struct MerchantActiveResponse {
    success: bool,
    store: MerchantActiveStore,
    balance: MerchantActiveBalance,
}

#[derive(Debug, Serialize)]
struct MerchantActiveStore {
    name: String,
    callback_url: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Serialize)]
struct MerchantActiveBalance {
    nexusggr: i64,
    pending: i64,
    settle: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProvidersResponse {
    success: bool,
    providers: Vec<BridgeProviderRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BridgeProviderRecord {
    code: String,
    name: String,
    status: i64,
}

#[derive(Debug, Deserialize)]
struct GamesRequest {
    provider_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GamesResponse {
    success: bool,
    provider_code: String,
    games: Vec<BridgeGameRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BridgeGameRecord {
    id: Option<i64>,
    game_code: Option<String>,
    game_name: Option<String>,
    banner: Option<String>,
    status: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GamesV2Response {
    success: bool,
    provider_code: String,
    games: Vec<BridgeGameV2Record>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BridgeGameV2Record {
    id: Option<i64>,
    game_code: Option<String>,
    game_name: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct CheckStatusRequest {
    trx_id: String,
}

#[derive(Debug, Serialize)]
struct CheckStatusResponse {
    success: bool,
    trx_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct GenerateRequestBody {
    username: String,
    amount: i64,
    expire: Option<i64>,
    custom_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GameLaunchRequestBody {
    username: String,
    provider_code: String,
    game_code: Option<String>,
    lang: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MoneyInfoRequestBody {
    username: Option<String>,
    all_users: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UserCreateRequestBody {
    username: String,
}

#[derive(Debug, Deserialize)]
struct UserDepositRequestBody {
    username: String,
    amount: i64,
    agent_sign: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserWithdrawResetRequestBody {
    username: Option<String>,
    all_users: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TransferStatusRequestBody {
    username: String,
    agent_sign: String,
}

#[derive(Debug, Deserialize)]
struct CallListRequestBody {
    provider_code: String,
    game_code: String,
}

#[derive(Debug, Deserialize)]
struct CallApplyRequestBody {
    provider_code: String,
    game_code: String,
    username: String,
    call_rtp: i64,
    call_type: i64,
}

#[derive(Debug, Deserialize)]
struct CallHistoryRequestBody {
    offset: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CallCancelRequestBody {
    call_id: i64,
}

#[derive(Debug, Deserialize)]
struct ControlRtpRequestBody {
    provider_code: String,
    username: String,
    rtp: f64,
}

#[derive(Debug, Deserialize)]
struct ControlUsersRtpRequestBody {
    user_codes: Vec<String>,
    rtp: f64,
}

#[derive(Debug, Serialize)]
struct GenerateResponse {
    success: bool,
    data: String,
    trx_id: String,
}

#[derive(Debug, Serialize)]
struct GameLaunchResponse {
    success: bool,
    launch_url: String,
}

#[derive(Debug, Serialize)]
struct MoneyInfoResponse {
    success: bool,
    agent: MoneyInfoAgent,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<MoneyInfoUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_list: Option<Vec<MoneyInfoUser>>,
}

#[derive(Debug, Serialize)]
struct MoneyInfoAgent {
    code: String,
    balance: i64,
}

#[derive(Debug, Clone, Serialize)]
struct MoneyInfoUser {
    username: String,
    balance: i64,
}

#[derive(Debug, Serialize)]
struct UserCreateResponse {
    success: bool,
    username: String,
}

#[derive(Debug, Serialize)]
struct UserDepositResponse {
    success: bool,
    agent: MoneyInfoAgent,
    user: MoneyInfoUser,
}

#[derive(Debug, Serialize)]
struct UserWithdrawResetResponse {
    success: bool,
    agent: MoneyInfoAgent,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<UserWithdrawResetUser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_list: Option<Vec<UserWithdrawResetUser>>,
}

#[derive(Debug, Clone, Serialize)]
struct UserWithdrawResetUser {
    username: String,
    withdraw_amount: i64,
    balance: i64,
}

#[derive(Debug, Serialize)]
struct TransferStatusResponse {
    success: bool,
    amount: i64,
    r#type: Option<String>,
    agent: MoneyInfoAgent,
    user: MoneyInfoUser,
}

#[derive(Debug, Serialize)]
struct CallPlayersResponse {
    success: bool,
    data: Vec<CallPlayerResponse>,
}

#[derive(Debug, Serialize)]
struct CallPlayerResponse {
    username: String,
    provider_code: Option<String>,
    game_code: Option<String>,
    bet: i64,
    balance: i64,
    total_debit: i64,
    total_credit: i64,
    target_rtp: i64,
    real_rtp: i64,
}

#[derive(Debug, Serialize)]
struct CallListResponse {
    success: bool,
    calls: Vec<CallListResponseRecord>,
}

#[derive(Debug, Serialize)]
struct CallListResponseRecord {
    rtp: Option<i64>,
    call_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct CallApplyResponse {
    success: bool,
    called_money: i64,
}

#[derive(Debug, Serialize)]
struct CallHistoryResponse {
    success: bool,
    data: Vec<CallHistoryResponseRecord>,
}

#[derive(Debug, Serialize)]
struct CallHistoryResponseRecord {
    id: i64,
    username: String,
    provider_code: Option<String>,
    game_code: Option<String>,
    bet: i64,
    user_prev: i64,
    user_after: i64,
    agent_prev: i64,
    agent_after: i64,
    expect: i64,
    missed: i64,
    real: i64,
    rtp: i64,
    r#type: Option<String>,
    status: i64,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct CallCancelResponse {
    success: bool,
    canceled_money: i64,
}

#[derive(Debug, Serialize)]
struct ControlRtpResponse {
    success: bool,
    changed_rtp: f64,
}

#[derive(Debug, sqlx::FromRow)]
struct CheckStatusTransaction {
    id: i64,
    code: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LaunchPlayer {
    username: String,
    ext_username: String,
}

async fn balance(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
) -> AppResult<Json<BalanceResponse>> {
    let balance = load_or_create_balance(&state.db, toko.id).await?;

    Ok(Json(BalanceResponse {
        success: true,
        pending_balance: balance.pending,
        settle_balance: balance.settle,
        nexusggr_balance: balance.nexusggr,
    }))
}

async fn providers(
    State(state): State<AppState>,
    _authenticated_toko: AuthenticatedToko,
) -> AppResult<Json<ProvidersResponse>> {
    if let Some(cached) = load_cached_provider_list(&state.redis).await? {
        return Ok(Json(cached));
    }

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .provider_list()
        .await
        .map_err(map_provider_list_error)?;

    let response = ProvidersResponse {
        success: true,
        providers: upstream
            .providers
            .into_iter()
            .map(|provider| BridgeProviderRecord {
                code: provider.code,
                name: provider.name,
                status: provider.status,
            })
            .collect(),
    };

    cache_provider_list(&state.redis, &response).await?;

    Ok(Json(response))
}

async fn merchant_active(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
) -> AppResult<Json<MerchantActiveResponse>> {
    let balance = load_or_create_balance(&state.db, toko.id).await?;

    Ok(Json(MerchantActiveResponse {
        success: true,
        store: MerchantActiveStore {
            name: toko.name,
            callback_url: toko.callback_url,
            token: toko.token,
        },
        balance: MerchantActiveBalance {
            nexusggr: balance.nexusggr,
            pending: balance.pending,
            settle: balance.settle,
        },
    }))
}

async fn games(
    State(state): State<AppState>,
    _authenticated_toko: AuthenticatedToko,
    payload: Result<Json<GamesRequest>, JsonRejection>,
) -> AppResult<Json<GamesResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let provider_code = validate_provider_code(&payload.provider_code)?;

    if let Some(cached) = load_cached_game_list(&state.redis, &provider_code).await? {
        return Ok(Json(cached));
    }

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .game_list(&provider_code)
        .await
        .map_err(map_game_list_error)?;

    let response = GamesResponse {
        success: true,
        provider_code,
        games: upstream
            .games
            .into_iter()
            .map(|game| BridgeGameRecord {
                id: game.id,
                game_code: game.game_code,
                game_name: game.game_name,
                banner: game.banner,
                status: game.status,
            })
            .collect(),
    };

    cache_game_list(&state.redis, &response).await?;

    Ok(Json(response))
}

async fn games_v2(
    State(state): State<AppState>,
    _authenticated_toko: AuthenticatedToko,
    payload: Result<Json<GamesRequest>, JsonRejection>,
) -> AppResult<Json<GamesV2Response>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let provider_code = validate_provider_code(&payload.provider_code)?;

    if let Some(cached) = load_cached_game_list_v2(&state.redis, &provider_code).await? {
        return Ok(Json(cached));
    }

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .game_list_v2(&provider_code)
        .await
        .map_err(map_game_list_v2_error)?;

    let response = GamesV2Response {
        success: true,
        provider_code,
        games: upstream
            .games
            .into_iter()
            .map(|game| BridgeGameV2Record {
                id: game.id,
                game_code: game.game_code,
                game_name: game.game_name,
            })
            .collect(),
    };

    cache_game_list_v2(&state.redis, &response).await?;

    Ok(Json(response))
}

async fn game_launch(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<GameLaunchRequestBody>, JsonRejection>,
) -> AppResult<Json<GameLaunchResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_game_launch_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .game_launch(&justqiu_nexusggr::GameLaunchRequest {
            user_code: player.ext_username,
            provider_code: request.provider_code,
            lang: request.lang,
            game_code: request.game_code,
        })
        .await
        .map_err(map_game_launch_error)?;

    Ok(Json(GameLaunchResponse {
        success: true,
        launch_url: upstream.launch_url,
    }))
}

async fn money_info(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<MoneyInfoRequestBody>, JsonRejection>,
) -> AppResult<Json<MoneyInfoResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_money_info_request(payload)?;

    let balance = load_or_create_balance(&state.db, toko.id).await?;

    let player = match request.username.as_deref() {
        Some(username) => {
            let player = sqlx::query_as::<_, LaunchPlayer>(
                r#"
                SELECT username, ext_username
                FROM players
                WHERE toko_id = $1
                  AND LOWER(username) = $2
                  AND deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(toko.id)
            .bind(username)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

            Some(player)
        }
        None => None,
    };

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .money_info(
            player.as_ref().map(|value| value.ext_username.as_str()),
            request.all_users,
        )
        .await
        .map_err(map_money_info_error)?;

    let user = match (upstream.user, player.as_ref()) {
        (Some(user), Some(player)) => Some(MoneyInfoUser {
            username: player.username.clone(),
            balance: user.balance,
        }),
        _ => None,
    };

    let user_list = if upstream.user_list.is_empty() {
        None
    } else {
        let username_map = load_player_username_map(&state.db, toko.id).await?;
        let records = upstream
            .user_list
            .into_iter()
            .filter_map(|record| {
                let external_username = record.user_code?;
                let username = username_map.get(&external_username)?.clone();

                Some(MoneyInfoUser {
                    username,
                    balance: record.balance,
                })
            })
            .collect::<Vec<_>>();

        Some(records)
    };

    Ok(Json(MoneyInfoResponse {
        success: true,
        agent: MoneyInfoAgent {
            code: toko.name,
            balance: balance.nexusggr,
        },
        user,
        user_list,
    }))
}

async fn user_create(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<UserCreateRequestBody>, JsonRejection>,
) -> AppResult<Json<UserCreateResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_user_create_request(payload)?;

    let existing_player_id = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT id
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?;

    if existing_player_id.is_some() {
        return Err(username_taken_error());
    }

    let ext_username = Ulid::new().to_string().to_ascii_lowercase();
    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    nexusggr_client
        .user_create(&ext_username)
        .await
        .map_err(map_user_create_error)?;

    sqlx::query(
        r#"
        INSERT INTO players (toko_id, username, ext_username)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .bind(&ext_username)
    .execute(&state.db)
    .await
    .map_err(map_player_insert_error)?;

    Ok(Json(UserCreateResponse {
        success: true,
        username: request.username,
    }))
}

async fn user_deposit(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<UserDepositRequestBody>, JsonRejection>,
) -> AppResult<Json<UserDepositResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_user_deposit_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO balances (toko_id)
        VALUES ($1)
        ON CONFLICT (toko_id) DO NOTHING
        "#,
    )
    .bind(toko.id)
    .execute(&mut *tx)
    .await?;

    let current_balance = sqlx::query_as::<_, justqiu_domain::models::Balance>(
        r#"
        SELECT id, toko_id, pending, settle, nexusggr, created_at, updated_at
        FROM balances
        WHERE toko_id = $1
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(toko.id)
    .fetch_one(&mut *tx)
    .await?;

    if current_balance.nexusggr <= request.amount {
        return Err(AppError::BadRequest("Insufficient balance".to_string()));
    }

    let updated_balance = sqlx::query_as::<_, justqiu_domain::models::Balance>(
        r#"
        UPDATE balances
        SET nexusggr = nexusggr - $2, updated_at = NOW()
        WHERE toko_id = $1
        RETURNING id, toko_id, pending, settle, nexusggr, created_at, updated_at
        "#,
    )
    .bind(toko.id)
    .bind(request.amount)
    .fetch_one(&mut *tx)
    .await?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .user_deposit(&justqiu_nexusggr::UserDepositRequest {
            user_code: player.ext_username.clone(),
            amount: request.amount,
            agent_sign: request.agent_sign.clone(),
        })
        .await
        .map_err(map_user_deposit_error)?;

    let note = serde_json::json!({
        "method": "user_deposit",
        "agent_sign": request.agent_sign,
        "user_balance": upstream.user_balance,
    });

    sqlx::query(
        r#"
        INSERT INTO transactions (toko_id, player, external_player, category, type, status, amount, note)
        VALUES ($1, $2, $3, 'nexusggr', 'deposit', 'success', $4, $5)
        "#,
    )
    .bind(toko.id)
    .bind(&player.username)
    .bind(&player.ext_username)
    .bind(request.amount)
    .bind(note.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(UserDepositResponse {
        success: true,
        agent: MoneyInfoAgent {
            code: toko.name,
            balance: updated_balance.nexusggr,
        },
        user: MoneyInfoUser {
            username: player.username,
            balance: upstream.user_balance,
        },
    }))
}

async fn user_withdraw(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<UserDepositRequestBody>, JsonRejection>,
) -> AppResult<Json<UserDepositResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_user_deposit_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let user_info = nexusggr_client
        .money_info(Some(&player.ext_username), false)
        .await
        .map_err(map_user_balance_info_error)?;
    let current_user_balance = user_info
        .user
        .ok_or_else(|| map_missing_user_balance())?
        .balance;

    if current_user_balance < request.amount {
        return Err(AppError::BadRequest(
            "User has insufficient balance on upstream platform".to_string(),
        ));
    }

    let mut tx = state.db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO balances (toko_id)
        VALUES ($1)
        ON CONFLICT (toko_id) DO NOTHING
        "#,
    )
    .bind(toko.id)
    .execute(&mut *tx)
    .await?;

    let upstream = nexusggr_client
        .user_withdraw(&justqiu_nexusggr::UserWithdrawRequest {
            user_code: player.ext_username.clone(),
            amount: request.amount,
            agent_sign: request.agent_sign.clone(),
        })
        .await
        .map_err(map_user_withdraw_error)?;

    let updated_balance = sqlx::query_as::<_, justqiu_domain::models::Balance>(
        r#"
        UPDATE balances
        SET nexusggr = nexusggr + $2, updated_at = NOW()
        WHERE toko_id = $1
        RETURNING id, toko_id, pending, settle, nexusggr, created_at, updated_at
        "#,
    )
    .bind(toko.id)
    .bind(request.amount)
    .fetch_one(&mut *tx)
    .await?;

    let note = serde_json::json!({
        "method": "user_withdraw",
        "agent_sign": request.agent_sign,
        "user_balance": upstream.user_balance,
    });

    sqlx::query(
        r#"
        INSERT INTO transactions (toko_id, player, external_player, category, type, status, amount, note)
        VALUES ($1, $2, $3, 'nexusggr', 'withdrawal', 'success', $4, $5)
        "#,
    )
    .bind(toko.id)
    .bind(&player.username)
    .bind(&player.ext_username)
    .bind(request.amount)
    .bind(note.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Json(UserDepositResponse {
        success: true,
        agent: MoneyInfoAgent {
            code: toko.name,
            balance: updated_balance.nexusggr,
        },
        user: MoneyInfoUser {
            username: player.username,
            balance: upstream.user_balance,
        },
    }))
}

async fn user_withdraw_reset(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<UserWithdrawResetRequestBody>, JsonRejection>,
) -> AppResult<Json<UserWithdrawResetResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_user_withdraw_reset_request(payload)?;

    let player = if let Some(username) = request.username.as_ref() {
        Some(
            sqlx::query_as::<_, LaunchPlayer>(
                r#"
                SELECT username, ext_username
                FROM players
                WHERE toko_id = $1
                  AND LOWER(username) = $2
                  AND deleted_at IS NULL
                LIMIT 1
                "#,
            )
            .bind(toko.id)
            .bind(username)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?,
        )
    } else {
        None
    };

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .user_withdraw_reset(
            player.as_ref().map(|value| value.ext_username.as_str()),
            request.all_users,
        )
        .await
        .map_err(map_user_withdraw_reset_error)?;

    let username_map = load_player_username_map(&state.db, toko.id).await?;
    create_nexusggr_withdraw_reset_transactions(
        &state.db,
        toko.id,
        request.all_users,
        &upstream,
        &username_map,
    )
    .await?;

    let user = upstream
        .user
        .as_ref()
        .and_then(|record| map_user_withdraw_reset_record(record, &username_map));
    let user_list = map_user_withdraw_reset_records(&upstream.user_list, &username_map);

    Ok(Json(UserWithdrawResetResponse {
        success: true,
        agent: MoneyInfoAgent {
            code: toko.name,
            balance: upstream
                .agent
                .as_ref()
                .map(|agent| agent.balance)
                .unwrap_or(0),
        },
        user,
        user_list: (!user_list.is_empty()).then_some(user_list),
    }))
}

async fn transfer_status(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<TransferStatusRequestBody>, JsonRejection>,
) -> AppResult<Json<TransferStatusResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_transfer_status_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .transfer_status(&player.ext_username, &request.agent_sign)
        .await
        .map_err(map_transfer_status_error)?;

    Ok(Json(TransferStatusResponse {
        success: true,
        amount: upstream.amount,
        r#type: upstream.r#type,
        agent: MoneyInfoAgent {
            code: toko.name,
            balance: upstream.agent_balance,
        },
        user: MoneyInfoUser {
            username: player.username,
            balance: upstream.user_balance,
        },
    }))
}

async fn call_players(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
) -> AppResult<Json<CallPlayersResponse>> {
    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .call_players()
        .await
        .map_err(map_call_players_error)?;
    let username_map = load_player_username_map(&state.db, toko.id).await?;

    Ok(Json(CallPlayersResponse {
        success: true,
        data: map_call_player_records(&upstream.data, &username_map),
    }))
}

async fn call_list(
    State(state): State<AppState>,
    AuthenticatedToko(_toko): AuthenticatedToko,
    payload: Result<Json<CallListRequestBody>, JsonRejection>,
) -> AppResult<Json<CallListResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_call_list_request(payload)?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .call_list(&request.provider_code, &request.game_code)
        .await
        .map_err(map_call_list_error)?;

    Ok(Json(CallListResponse {
        success: true,
        calls: upstream
            .calls
            .into_iter()
            .map(|record| CallListResponseRecord {
                rtp: record.rtp,
                call_type: record.call_type,
            })
            .collect(),
    }))
}

async fn call_apply(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<CallApplyRequestBody>, JsonRejection>,
) -> AppResult<Json<CallApplyResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_call_apply_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .call_apply(&justqiu_nexusggr::CallApplyRequest {
            provider_code: request.provider_code,
            game_code: request.game_code,
            user_code: player.ext_username,
            call_rtp: request.call_rtp,
            call_type: request.call_type,
        })
        .await
        .map_err(map_call_apply_error)?;

    Ok(Json(CallApplyResponse {
        success: true,
        called_money: upstream.called_money,
    }))
}

async fn call_history(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<CallHistoryRequestBody>, JsonRejection>,
) -> AppResult<Json<CallHistoryResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_call_history_request(payload)?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .call_history(request.offset, request.limit)
        .await
        .map_err(map_call_history_error)?;
    let username_map = load_player_username_map(&state.db, toko.id).await?;

    Ok(Json(CallHistoryResponse {
        success: true,
        data: map_call_history_records(&upstream.data, &username_map),
    }))
}

async fn call_cancel(
    State(state): State<AppState>,
    AuthenticatedToko(_toko): AuthenticatedToko,
    payload: Result<Json<CallCancelRequestBody>, JsonRejection>,
) -> AppResult<Json<CallCancelResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_call_cancel_request(payload)?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .call_cancel(request.call_id)
        .await
        .map_err(map_call_cancel_error)?;

    Ok(Json(CallCancelResponse {
        success: true,
        canceled_money: upstream.canceled_money,
    }))
}

async fn control_rtp(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<ControlRtpRequestBody>, JsonRejection>,
) -> AppResult<Json<ControlRtpResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_control_rtp_request(payload)?;

    let player = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND LOWER(username) = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .control_rtp(&request.provider_code, &player.ext_username, request.rtp)
        .await
        .map_err(map_control_rtp_error)?;

    Ok(Json(ControlRtpResponse {
        success: true,
        changed_rtp: upstream.changed_rtp,
    }))
}

async fn control_users_rtp(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<ControlUsersRtpRequestBody>, JsonRejection>,
) -> AppResult<Json<ControlRtpResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_control_users_rtp_request(payload)?;

    let players = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND deleted_at IS NULL
        "#,
    )
    .bind(toko.id)
    .fetch_all(&state.db)
    .await?;

    let player_map = players
        .into_iter()
        .map(|player| (player.username, player.ext_username))
        .collect::<HashMap<_, _>>();

    let external_user_codes = request
        .user_codes
        .iter()
        .map(|username| {
            player_map
                .get(username)
                .cloned()
                .ok_or_else(|| AppError::NotFoundMessage("Player not found".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let nexusggr_client = justqiu_nexusggr::NexusggrClient::new(
        &state.config.nexusggr_api_url,
        &state.config.nexusggr_agent_code,
        &state.config.nexusggr_agent_token,
    )
    .map_err(|err| AppError::Internal(err.into()))?;

    let upstream = nexusggr_client
        .control_users_rtp(&external_user_codes, request.rtp)
        .await
        .map_err(map_control_users_rtp_error)?;

    Ok(Json(ControlRtpResponse {
        success: true,
        changed_rtp: upstream.changed_rtp,
    }))
}

async fn check_status(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<CheckStatusRequest>, JsonRejection>,
) -> AppResult<Json<CheckStatusResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;

    let transaction = sqlx::query_as::<_, CheckStatusTransaction>(
        r#"
        SELECT id, code
        FROM transactions
        WHERE toko_id = $1
          AND code = $2
          AND category = 'qris'
          AND type = 'deposit'
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(toko.id)
    .bind(&payload.trx_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFoundMessage("Transaction not found".to_string()))?;

    let qris_client =
        justqiu_qris::QrisClient::new(&state.config.qris_api_url, &state.config.qris_merchant_uuid)
            .map_err(|err| AppError::Internal(err.into()))?;
    let upstream = qris_client
        .check_status(
            transaction.code.as_deref().unwrap_or(&payload.trx_id),
            &state.config.qris_client,
            &state.config.qris_client_key,
        )
        .await
        .map_err(map_check_status_error)?;
    let status = normalize_public_qris_status(&upstream.status)?;

    sqlx::query(
        r#"
        UPDATE transactions
        SET status = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(status)
    .bind(transaction.id)
    .execute(&state.db)
    .await?;

    Ok(Json(CheckStatusResponse {
        success: true,
        trx_id: transaction.code.unwrap_or(payload.trx_id),
        status: status.to_string(),
    }))
}

async fn generate(
    State(state): State<AppState>,
    AuthenticatedToko(toko): AuthenticatedToko,
    payload: Result<Json<GenerateRequestBody>, JsonRejection>,
) -> AppResult<Json<GenerateResponse>> {
    let Json(payload) =
        payload.map_err(|_| AppError::BadRequest("Invalid request body".to_string()))?;
    let request = validate_generate_request(payload)?;

    let qris_client =
        justqiu_qris::QrisClient::new(&state.config.qris_api_url, &state.config.qris_merchant_uuid)
            .map_err(|err| AppError::Internal(err.into()))?;
    let upstream = qris_client
        .generate(&request)
        .await
        .map_err(map_generate_error)?;

    let note = serde_json::json!({
        "purpose": "generate",
        "custom_ref": request.custom_ref,
    });

    sqlx::query(
        r#"
        INSERT INTO transactions (toko_id, player, category, type, status, amount, code, note)
        VALUES ($1, $2, 'qris', 'deposit', 'pending', $3, $4, $5)
        "#,
    )
    .bind(toko.id)
    .bind(&request.username)
    .bind(request.amount)
    .bind(&upstream.trx_id)
    .bind(note.to_string())
    .execute(&state.db)
    .await?;

    Ok(Json(GenerateResponse {
        success: true,
        data: upstream.data,
        trx_id: upstream.trx_id,
    }))
}

async fn load_or_create_balance(
    db: &sqlx::PgPool,
    toko_id: i64,
) -> Result<justqiu_domain::models::Balance, sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO balances (toko_id)
        VALUES ($1)
        ON CONFLICT (toko_id) DO NOTHING
        "#,
    )
    .bind(toko_id)
    .execute(db)
    .await?;

    sqlx::query_as::<_, justqiu_domain::models::Balance>(
        r#"
        SELECT id, toko_id, pending, settle, nexusggr, created_at, updated_at
        FROM balances
        WHERE toko_id = $1
        LIMIT 1
        "#,
    )
    .bind(toko_id)
    .fetch_one(db)
    .await
}

fn normalize_public_qris_status(status: &str) -> Result<&'static str, AppError> {
    match status.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok("pending"),
        "success" | "paid" => Ok("success"),
        "failed" => Ok("failed"),
        "expired" => Ok("expired"),
        other => Err(AppError::Internal(anyhow::anyhow!(
            "Unsupported QRIS transaction status for public response: {other}"
        ))),
    }
}

fn validate_generate_request(
    payload: GenerateRequestBody,
) -> Result<justqiu_qris::GenerateRequest, AppError> {
    let username = payload.username.trim();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if username.len() > 255 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 255 characters".to_string(),
        ));
    }

    if payload.amount < 10_000 {
        return Err(AppError::UnprocessableEntity(
            "amount must be at least 10000".to_string(),
        ));
    }

    if let Some(expire) = payload.expire {
        if expire < 1 {
            return Err(AppError::UnprocessableEntity(
                "expire must be at least 1".to_string(),
            ));
        }
    }

    let custom_ref = payload
        .custom_ref
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(custom_ref) = custom_ref.as_ref() {
        if custom_ref.len() > 36 {
            return Err(AppError::UnprocessableEntity(
                "custom_ref must not exceed 36 characters".to_string(),
            ));
        }

        if !custom_ref.chars().all(|char| char.is_ascii_alphanumeric()) {
            return Err(AppError::UnprocessableEntity(
                "custom_ref must be alphanumeric".to_string(),
            ));
        }
    }

    Ok(justqiu_qris::GenerateRequest {
        username: username.to_string(),
        amount: payload.amount,
        expire: Some(payload.expire.unwrap_or(300)),
        custom_ref,
    })
}

fn validate_provider_code(provider_code: &str) -> Result<String, AppError> {
    let provider_code = provider_code.trim();

    if provider_code.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "provider_code is required".to_string(),
        ));
    }

    if provider_code.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "provider_code must not exceed 50 characters".to_string(),
        ));
    }

    Ok(provider_code.to_string())
}

struct ValidatedGameLaunchRequest {
    username: String,
    provider_code: String,
    game_code: Option<String>,
    lang: String,
}

fn validate_game_launch_request(
    payload: GameLaunchRequestBody,
) -> Result<ValidatedGameLaunchRequest, AppError> {
    let username = payload.username.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }

    let provider_code = validate_provider_code(&payload.provider_code)?;

    let game_code = payload
        .game_code
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(game_code) = game_code.as_ref() {
        if game_code.len() > 50 {
            return Err(AppError::UnprocessableEntity(
                "game_code must not exceed 50 characters".to_string(),
            ));
        }
    }

    let lang = payload
        .lang
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "en".to_string());
    if lang.len() > 5 {
        return Err(AppError::UnprocessableEntity(
            "lang must not exceed 5 characters".to_string(),
        ));
    }

    Ok(ValidatedGameLaunchRequest {
        username,
        provider_code,
        game_code,
        lang,
    })
}

struct ValidatedMoneyInfoRequest {
    username: Option<String>,
    all_users: bool,
}

struct ValidatedUserCreateRequest {
    username: String,
}

struct ValidatedUserDepositRequest {
    username: String,
    amount: i64,
    agent_sign: Option<String>,
}

struct ValidatedUserWithdrawResetRequest {
    username: Option<String>,
    all_users: bool,
}

struct ValidatedTransferStatusRequest {
    username: String,
    agent_sign: String,
}

struct ValidatedCallListRequest {
    provider_code: String,
    game_code: String,
}

struct ValidatedCallApplyRequest {
    provider_code: String,
    game_code: String,
    username: String,
    call_rtp: i64,
    call_type: i64,
}

struct ValidatedCallHistoryRequest {
    offset: i64,
    limit: i64,
}

struct ValidatedCallCancelRequest {
    call_id: i64,
}

struct ValidatedControlRtpRequest {
    provider_code: String,
    username: String,
    rtp: f64,
}

struct ValidatedControlUsersRtpRequest {
    user_codes: Vec<String>,
    rtp: f64,
}

fn validate_money_info_request(
    payload: MoneyInfoRequestBody,
) -> Result<ValidatedMoneyInfoRequest, AppError> {
    let username = payload
        .username
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    if let Some(username) = username.as_ref() {
        if username.len() > 50 {
            return Err(AppError::UnprocessableEntity(
                "username must not exceed 50 characters".to_string(),
            ));
        }
    }

    Ok(ValidatedMoneyInfoRequest {
        username,
        all_users: payload.all_users.unwrap_or(false),
    })
}

fn validate_user_create_request(
    payload: UserCreateRequestBody,
) -> Result<ValidatedUserCreateRequest, AppError> {
    let username = payload.username.trim().to_ascii_lowercase();

    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }

    Ok(ValidatedUserCreateRequest { username })
}

fn validate_user_deposit_request(
    payload: UserDepositRequestBody,
) -> Result<ValidatedUserDepositRequest, AppError> {
    let username = payload.username.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }

    if payload.amount < 10_000 {
        return Err(AppError::UnprocessableEntity(
            "amount must be at least 10000".to_string(),
        ));
    }

    let agent_sign = payload
        .agent_sign
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(agent_sign) = agent_sign.as_ref() {
        if agent_sign.len() > 255 {
            return Err(AppError::UnprocessableEntity(
                "agent_sign must not exceed 255 characters".to_string(),
            ));
        }
    }

    Ok(ValidatedUserDepositRequest {
        username,
        amount: payload.amount,
        agent_sign,
    })
}

fn validate_user_withdraw_reset_request(
    payload: UserWithdrawResetRequestBody,
) -> Result<ValidatedUserWithdrawResetRequest, AppError> {
    let username = payload
        .username
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let all_users = payload.all_users.unwrap_or(false);

    if !all_users && username.is_none() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if let Some(username) = username.as_ref() {
        if username.len() > 50 {
            return Err(AppError::UnprocessableEntity(
                "username must not exceed 50 characters".to_string(),
            ));
        }
    }

    Ok(ValidatedUserWithdrawResetRequest {
        username,
        all_users,
    })
}

fn validate_transfer_status_request(
    payload: TransferStatusRequestBody,
) -> Result<ValidatedTransferStatusRequest, AppError> {
    let username = payload.username.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }

    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }

    let agent_sign = payload.agent_sign.trim().to_string();
    if agent_sign.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "agent_sign is required".to_string(),
        ));
    }

    if agent_sign.len() > 255 {
        return Err(AppError::UnprocessableEntity(
            "agent_sign must not exceed 255 characters".to_string(),
        ));
    }

    Ok(ValidatedTransferStatusRequest {
        username,
        agent_sign,
    })
}

fn validate_call_list_request(
    payload: CallListRequestBody,
) -> Result<ValidatedCallListRequest, AppError> {
    let provider_code = payload.provider_code.trim().to_string();
    if provider_code.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "provider_code is required".to_string(),
        ));
    }
    if provider_code.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "provider_code must not exceed 50 characters".to_string(),
        ));
    }

    let game_code = payload.game_code.trim().to_string();
    if game_code.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "game_code is required".to_string(),
        ));
    }
    if game_code.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "game_code must not exceed 50 characters".to_string(),
        ));
    }

    Ok(ValidatedCallListRequest {
        provider_code,
        game_code,
    })
}

fn validate_call_apply_request(
    payload: CallApplyRequestBody,
) -> Result<ValidatedCallApplyRequest, AppError> {
    let provider_code = validate_provider_code(&payload.provider_code)?;

    let game_code = payload.game_code.trim().to_string();
    if game_code.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "game_code is required".to_string(),
        ));
    }
    if game_code.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "game_code must not exceed 50 characters".to_string(),
        ));
    }

    let username = payload.username.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }
    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }

    if !(payload.call_type == 1 || payload.call_type == 2) {
        return Err(AppError::UnprocessableEntity(
            "call_type must be one of: 1, 2".to_string(),
        ));
    }

    Ok(ValidatedCallApplyRequest {
        provider_code,
        game_code,
        username,
        call_rtp: payload.call_rtp,
        call_type: payload.call_type,
    })
}

fn validate_call_history_request(
    payload: CallHistoryRequestBody,
) -> Result<ValidatedCallHistoryRequest, AppError> {
    let offset = payload.offset.unwrap_or(0);
    if offset < 0 {
        return Err(AppError::UnprocessableEntity(
            "offset must be at least 0".to_string(),
        ));
    }

    let limit = payload.limit.unwrap_or(50);
    if limit < 1 {
        return Err(AppError::UnprocessableEntity(
            "limit must be at least 1".to_string(),
        ));
    }
    if limit > 500 {
        return Err(AppError::UnprocessableEntity(
            "limit must not exceed 500".to_string(),
        ));
    }

    Ok(ValidatedCallHistoryRequest { offset, limit })
}

fn validate_call_cancel_request(
    payload: CallCancelRequestBody,
) -> Result<ValidatedCallCancelRequest, AppError> {
    if payload.call_id < 1 {
        return Err(AppError::UnprocessableEntity(
            "call_id must be at least 1".to_string(),
        ));
    }

    Ok(ValidatedCallCancelRequest {
        call_id: payload.call_id,
    })
}

fn validate_control_rtp_request(
    payload: ControlRtpRequestBody,
) -> Result<ValidatedControlRtpRequest, AppError> {
    let provider_code = validate_provider_code(&payload.provider_code)?;

    let username = payload.username.trim().to_ascii_lowercase();
    if username.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "username is required".to_string(),
        ));
    }
    if username.len() > 50 {
        return Err(AppError::UnprocessableEntity(
            "username must not exceed 50 characters".to_string(),
        ));
    }
    if !payload.rtp.is_finite() {
        return Err(AppError::UnprocessableEntity(
            "rtp must be a finite number".to_string(),
        ));
    }
    if payload.rtp < 0.0 {
        return Err(AppError::UnprocessableEntity(
            "rtp must be at least 0".to_string(),
        ));
    }

    Ok(ValidatedControlRtpRequest {
        provider_code,
        username,
        rtp: payload.rtp,
    })
}

fn validate_control_users_rtp_request(
    payload: ControlUsersRtpRequestBody,
) -> Result<ValidatedControlUsersRtpRequest, AppError> {
    if payload.user_codes.is_empty() {
        return Err(AppError::UnprocessableEntity(
            "user_codes must contain at least 1 item".to_string(),
        ));
    }
    if !payload.rtp.is_finite() {
        return Err(AppError::UnprocessableEntity(
            "rtp must be a finite number".to_string(),
        ));
    }
    if payload.rtp < 0.0 {
        return Err(AppError::UnprocessableEntity(
            "rtp must be at least 0".to_string(),
        ));
    }

    let user_codes = payload
        .user_codes
        .into_iter()
        .enumerate()
        .map(|(index, username)| {
            let username = username.trim().to_ascii_lowercase();
            if username.is_empty() {
                return Err(AppError::UnprocessableEntity(format!(
                    "user_codes[{index}] is required"
                )));
            }
            if username.len() > 50 {
                return Err(AppError::UnprocessableEntity(format!(
                    "user_codes[{index}] must not exceed 50 characters"
                )));
            }
            Ok(username)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ValidatedControlUsersRtpRequest {
        user_codes,
        rtp: payload.rtp,
    })
}

async fn load_player_username_map(
    db: &sqlx::PgPool,
    toko_id: i64,
) -> Result<HashMap<String, String>, sqlx::Error> {
    let rows = sqlx::query_as::<_, LaunchPlayer>(
        r#"
        SELECT username, ext_username
        FROM players
        WHERE toko_id = $1
          AND deleted_at IS NULL
        "#,
    )
    .bind(toko_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|player| (player.ext_username, player.username))
        .collect())
}

async fn create_nexusggr_withdraw_reset_transactions(
    db: &sqlx::PgPool,
    toko_id: i64,
    all_users: bool,
    response: &justqiu_nexusggr::UserWithdrawResetResponse,
    username_map: &HashMap<String, String>,
) -> Result<(), sqlx::Error> {
    let mut seen_external_usernames = HashSet::new();
    let mut tx = db.begin().await?;

    for record in response.user.iter().chain(response.user_list.iter()) {
        let Some(external_username) = record.user_code.as_deref() else {
            continue;
        };

        if !seen_external_usernames.insert(external_username.to_string()) {
            continue;
        }

        let Some(username) = username_map.get(external_username) else {
            continue;
        };

        let note = serde_json::json!({
            "method": "user_withdraw_reset",
            "scope": if all_users { "all_users" } else { "single_user" },
            "user_balance": record.balance,
        });

        sqlx::query(
            r#"
            INSERT INTO transactions (toko_id, player, external_player, category, type, status, amount, note)
            VALUES ($1, $2, $3, 'nexusggr', 'withdrawal', 'success', $4, $5)
            "#,
        )
        .bind(toko_id)
        .bind(username)
        .bind(external_username)
        .bind(record.withdraw_amount)
        .bind(note.to_string())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

fn map_user_withdraw_reset_record(
    record: &justqiu_nexusggr::UserWithdrawResetUser,
    username_map: &HashMap<String, String>,
) -> Option<UserWithdrawResetUser> {
    let external_username = record.user_code.as_ref()?;
    let username = username_map.get(external_username)?.clone();

    Some(UserWithdrawResetUser {
        username,
        withdraw_amount: record.withdraw_amount,
        balance: record.balance,
    })
}

fn map_user_withdraw_reset_records(
    records: &[justqiu_nexusggr::UserWithdrawResetUser],
    username_map: &HashMap<String, String>,
) -> Vec<UserWithdrawResetUser> {
    records
        .iter()
        .filter_map(|record| map_user_withdraw_reset_record(record, username_map))
        .collect()
}

fn map_call_player_record(
    record: &justqiu_nexusggr::CallPlayerRecord,
    username_map: &HashMap<String, String>,
) -> Option<CallPlayerResponse> {
    let external_username = record.user_code.as_ref()?;
    let username = username_map.get(external_username)?.clone();

    Some(CallPlayerResponse {
        username,
        provider_code: record.provider_code.clone(),
        game_code: record.game_code.clone(),
        bet: record.bet,
        balance: record.balance,
        total_debit: record.total_debit,
        total_credit: record.total_credit,
        target_rtp: record.target_rtp,
        real_rtp: record.real_rtp,
    })
}

fn map_call_player_records(
    records: &[justqiu_nexusggr::CallPlayerRecord],
    username_map: &HashMap<String, String>,
) -> Vec<CallPlayerResponse> {
    records
        .iter()
        .filter_map(|record| map_call_player_record(record, username_map))
        .collect()
}

fn map_call_history_record(
    record: &justqiu_nexusggr::CallHistoryRecord,
    username_map: &HashMap<String, String>,
) -> Option<CallHistoryResponseRecord> {
    let external_username = record.user_code.as_ref()?;
    let username = username_map.get(external_username)?.clone();

    Some(CallHistoryResponseRecord {
        id: record.id,
        username,
        provider_code: record.provider_code.clone(),
        game_code: record.game_code.clone(),
        bet: record.bet,
        user_prev: record.user_prev,
        user_after: record.user_after,
        agent_prev: record.agent_prev,
        agent_after: record.agent_after,
        expect: record.expect,
        missed: record.missed,
        real: record.real,
        rtp: record.rtp,
        r#type: record.r#type.clone(),
        status: record.status,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
    })
}

fn map_call_history_records(
    records: &[justqiu_nexusggr::CallHistoryRecord],
    username_map: &HashMap<String, String>,
) -> Vec<CallHistoryResponseRecord> {
    records
        .iter()
        .filter_map(|record| map_call_history_record(record, username_map))
        .collect()
}

fn map_generate_error(error: justqiu_qris::QrisError) -> AppError {
    match error {
        justqiu_qris::QrisError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_qris::QrisError::Transport(_)
        | justqiu_qris::QrisError::UpstreamFailure { .. }
        | justqiu_qris::QrisError::InvalidResponse(_) => {
            AppError::InternalMessage("Failed to generate QRIS from upstream provider".to_string())
        }
    }
}

async fn load_cached_provider_list(
    client: &redis::Client,
) -> Result<Option<ProvidersResponse>, AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload: Option<String> = connection
        .get(provider_list_cache_key())
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    payload
        .map(|value| serde_json::from_str::<ProvidersResponse>(&value))
        .transpose()
        .map_err(|err| AppError::Internal(err.into()))
}

async fn cache_provider_list(
    client: &redis::Client,
    response: &ProvidersResponse,
) -> Result<(), AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload = serde_json::to_string(response).map_err(|err| AppError::Internal(err.into()))?;
    let _: () = connection
        .set_ex(provider_list_cache_key(), payload, 86_400)
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    Ok(())
}

async fn load_cached_game_list(
    client: &redis::Client,
    provider_code: &str,
) -> Result<Option<GamesResponse>, AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload: Option<String> = connection
        .get(game_list_cache_key(provider_code))
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    payload
        .map(|value| serde_json::from_str::<GamesResponse>(&value))
        .transpose()
        .map_err(|err| AppError::Internal(err.into()))
}

async fn cache_game_list(client: &redis::Client, response: &GamesResponse) -> Result<(), AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload = serde_json::to_string(response).map_err(|err| AppError::Internal(err.into()))?;
    let _: () = connection
        .set_ex(
            game_list_cache_key(&response.provider_code),
            payload,
            86_400,
        )
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    Ok(())
}

async fn load_cached_game_list_v2(
    client: &redis::Client,
    provider_code: &str,
) -> Result<Option<GamesV2Response>, AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload: Option<String> = connection
        .get(game_list_v2_cache_key(provider_code))
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    payload
        .map(|value| serde_json::from_str::<GamesV2Response>(&value))
        .transpose()
        .map_err(|err| AppError::Internal(err.into()))
}

async fn cache_game_list_v2(
    client: &redis::Client,
    response: &GamesV2Response,
) -> Result<(), AppError> {
    let mut connection = client
        .get_connection_manager()
        .await
        .map_err(|err| AppError::Internal(err.into()))?;
    let payload = serde_json::to_string(response).map_err(|err| AppError::Internal(err.into()))?;
    let _: () = connection
        .set_ex(
            game_list_v2_cache_key(&response.provider_code),
            payload,
            86_400,
        )
        .await
        .map_err(|err| AppError::Internal(err.into()))?;

    Ok(())
}

fn provider_list_cache_key() -> &'static str {
    "cache:nexusggr:provider-list"
}

fn game_list_cache_key(provider_code: &str) -> String {
    format!("cache:nexusggr:game-list:{provider_code}")
}

fn game_list_v2_cache_key(provider_code: &str) -> String {
    format!("cache:nexusggr:game-list-v2:{provider_code}")
}

fn map_provider_list_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get provider list from upstream platform".to_string(),
        ),
    }
}

fn map_game_list_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to get game list from upstream platform".to_string())
        }
    }
}

fn map_game_list_v2_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get localized game list from upstream platform".to_string(),
        ),
    }
}

fn map_game_launch_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to launch game on upstream platform".to_string())
        }
    }
}

fn map_money_info_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get balance information from upstream platform".to_string(),
        ),
    }
}

fn map_user_create_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to create user on upstream platform".to_string())
        }
    }
}

fn map_user_deposit_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to deposit user on upstream platform".to_string())
        }
    }
}

fn map_user_balance_info_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get user balance from upstream platform".to_string(),
        ),
    }
}

fn map_missing_user_balance() -> AppError {
    AppError::InternalMessage("Failed to get user balance from upstream platform".to_string())
}

fn map_user_withdraw_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to withdraw user on upstream platform".to_string())
        }
    }
}

fn map_user_withdraw_reset_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to reset withdraw on upstream platform".to_string())
        }
    }
}

fn map_transfer_status_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get transfer status from upstream platform".to_string(),
        ),
    }
}

fn map_call_players_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get active players from upstream platform".to_string(),
        ),
    }
}

fn map_call_list_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to get call list from upstream platform".to_string())
        }
    }
}

fn map_call_apply_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to apply call on upstream platform".to_string())
        }
    }
}

fn map_call_history_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to get call history from upstream platform".to_string(),
        ),
    }
}

fn map_call_cancel_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to cancel call on upstream platform".to_string())
        }
    }
}

fn map_control_rtp_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => {
            AppError::InternalMessage("Failed to control RTP on upstream platform".to_string())
        }
    }
}

fn map_control_users_rtp_error(error: justqiu_nexusggr::NexusggrError) -> AppError {
    match error {
        justqiu_nexusggr::NexusggrError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_nexusggr::NexusggrError::Transport(_)
        | justqiu_nexusggr::NexusggrError::InvalidResponse(_)
        | justqiu_nexusggr::NexusggrError::UpstreamFailure { .. } => AppError::InternalMessage(
            "Failed to control users RTP on upstream platform".to_string(),
        ),
    }
}

fn map_player_insert_error(error: sqlx::Error) -> AppError {
    if let Some(database_error) = error.as_database_error() {
        if database_error.code().as_deref() == Some("23505") {
            return username_taken_error();
        }
    }

    AppError::Database(error)
}

fn username_taken_error() -> AppError {
    AppError::UnprocessableEntity("username has already been taken".to_string())
}

fn map_check_status_error(error: justqiu_qris::QrisError) -> AppError {
    match error {
        justqiu_qris::QrisError::InvalidConfig(_) => AppError::Internal(error.into()),
        justqiu_qris::QrisError::Transport(_)
        | justqiu_qris::QrisError::UpstreamFailure { .. }
        | justqiu_qris::QrisError::InvalidResponse(_) => AppError::InternalMessage(
            "Failed to get QRIS transaction status from upstream provider".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use redis::AsyncCommands;
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use sqlx::postgres::{PgPool, PgPoolOptions};
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::{
        app::{create_router, AppState},
        config::AppConfig,
    };

    fn authorization_header(personal_access_token_id: i64, plain_token: &str) -> String {
        format!("Bearer {personal_access_token_id}|{plain_token}")
    }

    fn hash_sanctum_token(plaintext: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        hex::encode(hasher.finalize())
    }

    async fn test_state(redis_url: &str, database_url: &str) -> AppState {
        AppState {
            db: PgPoolOptions::new()
                .max_connections(2)
                .connect(database_url)
                .await
                .expect("postgres pool"),
            redis: redis::Client::open(redis_url).expect("redis client"),
            config: Arc::new(AppConfig {
                database_url: database_url.to_string(),
                redis_url: redis_url.to_string(),
                bind_address: "127.0.0.1:0".to_string(),
                jwt_secret: "test-jwt-secret".to_string(),
                jwt_expiry_hours: 8,
                nexusggr_api_url: "https://api.nexusggr.test".to_string(),
                nexusggr_agent_code: "agent".to_string(),
                nexusggr_agent_token: "token".to_string(),
                qris_api_url: "https://qris.test/api".to_string(),
                qris_merchant_uuid: "merchant-uuid".to_string(),
                qris_client: "client".to_string(),
                qris_client_key: "client-key".to_string(),
            }),
        }
    }

    async fn insert_toko_fixture(db: &PgPool) -> (i64, i64, i64, String, String) {
        let suffix = Uuid::new_v4().simple().to_string();
        let username = format!("test_api_rl_{suffix}");
        let email = format!("{username}@localhost");
        let plain_token = format!("plain_{suffix}");
        let token_hash = hash_sanctum_token(&plain_token);

        let user_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO users (username, name, email, password, role, is_active)
            VALUES ($1, $2, $3, $4, $5, true)
            RETURNING id
            "#,
        )
        .bind(&username)
        .bind("Test API Rate Limit User")
        .bind(&email)
        .bind("not-used")
        .bind("dev")
        .fetch_one(db)
        .await
        .expect("insert user");

        let toko_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO tokos (user_id, name, token, is_active)
            VALUES ($1, $2, $3, true)
            RETURNING id
            "#,
        )
        .bind(user_id)
        .bind("Test API Rate Limit Toko")
        .bind("test-api-rate-limit-store-token")
        .fetch_one(db)
        .await
        .expect("insert toko");

        let personal_access_token_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO personal_access_tokens (tokenable_type, tokenable_id, name, token, abilities)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(r#"App\Models\Toko"#)
        .bind(toko_id)
        .bind("test-api-rate-limit")
        .bind(token_hash)
        .bind(r#"["*"]"#)
        .fetch_one(db)
        .await
        .expect("insert personal access token");

        (
            user_id,
            toko_id,
            personal_access_token_id,
            username,
            plain_token,
        )
    }

    async fn cleanup_toko_fixture(
        db: &PgPool,
        user_id: i64,
        toko_id: i64,
        personal_access_token_id: i64,
    ) {
        sqlx::query("DELETE FROM transactions WHERE toko_id = $1")
            .bind(toko_id)
            .execute(db)
            .await
            .expect("delete transactions");
        sqlx::query("DELETE FROM players WHERE toko_id = $1")
            .bind(toko_id)
            .execute(db)
            .await
            .expect("delete players");
        sqlx::query("DELETE FROM balances WHERE toko_id = $1")
            .bind(toko_id)
            .execute(db)
            .await
            .expect("delete balances");
        sqlx::query("DELETE FROM personal_access_tokens WHERE id = $1")
            .bind(personal_access_token_id)
            .execute(db)
            .await
            .expect("delete personal access token");
        sqlx::query("DELETE FROM tokos WHERE id = $1")
            .bind(toko_id)
            .execute(db)
            .await
            .expect("delete toko");
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(db)
            .await
            .expect("delete user");
    }

    async fn insert_balance_fixture(
        db: &PgPool,
        toko_id: i64,
        pending: i64,
        settle: i64,
        nexusggr: i64,
    ) {
        sqlx::query(
            r#"
            INSERT INTO balances (toko_id, pending, settle, nexusggr)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (toko_id) DO UPDATE
            SET pending = EXCLUDED.pending,
                settle = EXCLUDED.settle,
                nexusggr = EXCLUDED.nexusggr,
                updated_at = NOW()
            "#,
        )
        .bind(toko_id)
        .bind(pending)
        .bind(settle)
        .bind(nexusggr)
        .execute(db)
        .await
        .expect("insert balance fixture");
    }

    async fn insert_player_fixture(db: &PgPool, toko_id: i64, username: &str, ext_username: &str) {
        sqlx::query(
            r#"
            INSERT INTO players (toko_id, username, ext_username)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(toko_id)
        .bind(username)
        .bind(ext_username)
        .execute(db)
        .await
        .expect("insert player fixture");
    }

    async fn insert_qris_transaction_fixture(
        db: &PgPool,
        toko_id: i64,
        trx_id: &str,
        status: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO transactions (toko_id, player, category, type, status, amount, code, note)
            VALUES ($1, $2, 'qris', 'deposit', $3, $4, $5, $6)
            "#,
        )
        .bind(toko_id)
        .bind("fixture-player")
        .bind(status)
        .bind(10_000_i64)
        .bind(trx_id)
        .bind(r#"{"purpose":"generate"}"#)
        .execute(db)
        .await
        .expect("insert qris transaction fixture");
    }

    #[tokio::test]
    async fn balance_creates_missing_balance_row_and_returns_zero_values() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/v1/balance")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .body(Body::empty())
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], Value::Bool(true));
        assert_eq!(body["pending_balance"], Value::Number(0.into()));
        assert_eq!(body["settle_balance"], Value::Number(0.into()));
        assert_eq!(body["nexusggr_balance"], Value::Number(0.into()));

        let balance_row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT pending, settle, nexusggr FROM balances WHERE toko_id = $1 LIMIT 1",
        )
        .bind(toko_id)
        .fetch_optional(&state.db)
        .await
        .expect("select balance");
        assert_eq!(balance_row, Some((0, 0, 0)));

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn merchant_active_returns_store_and_current_balance() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        sqlx::query("UPDATE tokos SET callback_url = $2, token = $3, name = $4 WHERE id = $1")
            .bind(toko_id)
            .bind("https://callback.test/store")
            .bind("legacy-store-token")
            .bind("Test Merchant Active Toko")
            .execute(&state.db)
            .await
            .expect("update toko");
        insert_balance_fixture(&state.db, toko_id, 12_345, 67_890, 123_456).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/merchant-active")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .body(Body::empty())
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], Value::Bool(true));
        assert_eq!(
            body["store"]["name"],
            Value::String("Test Merchant Active Toko".to_string())
        );
        assert_eq!(
            body["store"]["callback_url"],
            Value::String("https://callback.test/store".to_string())
        );
        assert_eq!(
            body["store"]["token"],
            Value::String("legacy-store-token".to_string())
        );
        assert_eq!(body["balance"]["pending"], Value::Number(12_345.into()));
        assert_eq!(body["balance"]["settle"], Value::Number(67_890.into()));
        assert_eq!(body["balance"]["nexusggr"], Value::Number(123_456.into()));

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn user_deposit_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/user/deposit")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "missing-player",
                    "amount": 10000
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let balance_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM balances WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count balances");
        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE toko_id = $1 AND category = 'nexusggr' AND type = 'deposit'",
        )
        .bind(toko_id)
        .fetch_one(&state.db)
        .await
        .expect("count transactions");

        assert_eq!(balance_count, 0);
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn user_deposit_returns_bad_request_for_insufficient_balance_without_ledger_mutation() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        insert_player_fixture(&state.db, toko_id, "wallet-player", "ext-wallet-player").await;
        insert_balance_fixture(&state.db, toko_id, 0, 0, 5_000).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/user/deposit")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "wallet-player",
                    "amount": 10000
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Insufficient balance".to_string())
        );

        let nexusggr_balance: i64 =
            sqlx::query_scalar("SELECT nexusggr FROM balances WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("select nexusggr balance");
        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE toko_id = $1 AND category = 'nexusggr' AND type = 'deposit'",
        )
        .bind(toko_id)
        .fetch_one(&state.db)
        .await
        .expect("count transactions");

        assert_eq!(nexusggr_balance, 5_000);
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn user_withdraw_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/user/withdraw")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "missing-player",
                    "amount": 10000
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let balance_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM balances WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count balances");
        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE toko_id = $1 AND category = 'nexusggr' AND type = 'withdrawal'",
        )
        .bind(toko_id)
        .fetch_one(&state.db)
        .await
        .expect("count transactions");

        assert_eq!(balance_count, 0);
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn money_info_returns_not_found_for_missing_player() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/money/info")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "missing-player"
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let balance_row: Option<(i64, i64, i64)> = sqlx::query_as(
            "SELECT pending, settle, nexusggr FROM balances WHERE toko_id = $1 LIMIT 1",
        )
        .bind(toko_id)
        .fetch_optional(&state.db)
        .await
        .expect("select balance");
        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE toko_id = $1 AND category = 'nexusggr'",
        )
        .bind(toko_id)
        .fetch_one(&state.db)
        .await
        .expect("count transactions");

        assert_eq!(balance_row, Some((0, 0, 0)));
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn game_launch_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/game/launch")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "missing-player",
                    "provider_code": "PLAYNGO",
                    "lang": "en",
                    "game_code": "piggyblitzcasinogold"
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let transaction_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn transfer_status_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/transfer/status")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "missing-player",
                    "agent_sign": "test-agent-sign"
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let transaction_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn call_apply_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/call/apply")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "provider_code": "PLAYNGO",
                    "game_code": "piggyblitzcasinogold",
                    "username": "missing-player",
                    "call_rtp": 210,
                    "call_type": 1
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let transaction_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn control_rtp_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/control/rtp")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "provider_code": "PLAYNGO",
                    "username": "missing-player",
                    "rtp": 91.5
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let transaction_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn control_users_rtp_returns_not_found_for_missing_player_without_side_effects() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/control/users-rtp")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "user_codes": ["missing-player"],
                    "rtp": 91.5
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Player not found".to_string())
        );

        let transaction_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE toko_id = $1")
                .bind(toko_id)
                .fetch_one(&state.db)
                .await
                .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn call_cancel_rejects_invalid_call_id() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/call/cancel")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "call_id": 0
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("call_id must be at least 1".to_string())
        );

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn call_list_rejects_missing_game_code() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/call/list")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "provider_code": "PLAYNGO",
                    "game_code": ""
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("game_code is required".to_string())
        );

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn generate_rejects_invalid_custom_ref_and_amount_without_creating_transaction() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let invalid_custom_ref_request = Request::builder()
            .method("POST")
            .uri("/api/v1/generate")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "player-one",
                    "amount": 10000,
                    "custom_ref": "NOT-ALNUM"
                })
                .to_string(),
            ))
            .expect("request");

        let invalid_custom_ref_response = app
            .clone()
            .oneshot(invalid_custom_ref_request)
            .await
            .expect("router response");
        let invalid_custom_ref_status = invalid_custom_ref_response.status();
        let invalid_custom_ref_body: Value = serde_json::from_slice(
            &to_bytes(invalid_custom_ref_response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(invalid_custom_ref_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(invalid_custom_ref_body["success"], Value::Bool(false));
        assert_eq!(
            invalid_custom_ref_body["message"],
            Value::String("custom_ref must be alphanumeric".to_string())
        );

        let invalid_amount_request = Request::builder()
            .method("POST")
            .uri("/api/v1/generate")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "username": "player-one",
                    "amount": 9999,
                    "custom_ref": "VALIDREF01"
                })
                .to_string(),
            ))
            .expect("request");

        let invalid_amount_response = app
            .clone()
            .oneshot(invalid_amount_request)
            .await
            .expect("router response");
        let invalid_amount_status = invalid_amount_response.status();
        let invalid_amount_body: Value = serde_json::from_slice(
            &to_bytes(invalid_amount_response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(invalid_amount_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(invalid_amount_body["success"], Value::Bool(false));
        assert_eq!(
            invalid_amount_body["message"],
            Value::String("amount must be at least 10000".to_string())
        );

        let transaction_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE toko_id = $1 AND category = 'qris' AND type = 'deposit'",
        )
        .bind(toko_id)
        .fetch_one(&state.db)
        .await
        .expect("count transactions");
        assert_eq!(transaction_count, 0);

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }

    #[tokio::test]
    async fn check_status_returns_not_found_for_transaction_owned_by_another_toko() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;
        let (
            other_user_id,
            other_toko_id,
            other_personal_access_token_id,
            _other_username,
            _other_plain_token,
        ) = insert_toko_fixture(&state.db).await;
        let trx_id = format!("trx-foreign-{}", Uuid::new_v4().simple());
        insert_qris_transaction_fixture(&state.db, other_toko_id, &trx_id, "pending").await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/check-status")
            .header(
                "authorization",
                authorization_header(personal_access_token_id, &plain_token),
            )
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "trx_id": trx_id
                })
                .to_string(),
            ))
            .expect("request");

        let response = app.clone().oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Transaction not found".to_string())
        );

        let foreign_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM transactions WHERE toko_id = $1 AND code = $2")
                .bind(other_toko_id)
                .bind(&trx_id)
                .fetch_optional(&state.db)
                .await
                .expect("select foreign transaction status");
        assert_eq!(foreign_status.as_deref(), Some("pending"));

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
        cleanup_toko_fixture(
            &state.db,
            other_user_id,
            other_toko_id,
            other_personal_access_token_id,
        )
        .await;
    }

    #[tokio::test]
    async fn merchant_active_returns_unauthenticated_for_invalid_bearer_token() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/merchant-active")
            .header("authorization", "Bearer 999999|invalid-token")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("router response");
        let status = response.status();
        let body: Value = serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body");

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(
            body["message"],
            Value::String("Unauthenticated".to_string())
        );
    }

    #[tokio::test]
    async fn merchant_active_enforces_api_rate_limit_per_toko_per_route() {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:postgres@127.0.0.1:5432/justqiu".to_string()
        });
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let state = test_state(&redis_url, &database_url).await;
        let app = create_router(state.clone());
        let (user_id, toko_id, personal_access_token_id, _username, plain_token) =
            insert_toko_fixture(&state.db).await;

        let rate_limit_key =
            justqiu_redis::rate_limit_key(&format!("api:{toko_id}"), "merchant-active");
        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let _: i64 = redis::cmd("DEL")
            .arg(&rate_limit_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup rate limit key");
        drop(connection);

        let authorization = format!("Bearer {personal_access_token_id}|{plain_token}");
        let mut success_count = 0_u64;
        let mut limited_count = 0_u64;
        let mut last_status = StatusCode::OK;
        let mut last_body = Value::Null;

        for _ in 0..121 {
            let request = Request::builder()
                .method("POST")
                .uri("/api/v1/merchant-active")
                .header("authorization", &authorization)
                .body(Body::empty())
                .expect("request");

            let response = app.clone().oneshot(request).await.expect("router response");
            let status = response.status();
            let body: Value = serde_json::from_slice(
                &to_bytes(response.into_body(), usize::MAX)
                    .await
                    .expect("response body"),
            )
            .expect("json body");

            match status {
                StatusCode::OK => success_count += 1,
                StatusCode::TOO_MANY_REQUESTS => limited_count += 1,
                other => panic!("unexpected status: {other}"),
            }

            last_status = status;
            last_body = body;
        }

        assert_eq!(success_count, 120);
        assert_eq!(limited_count, 1);
        assert_eq!(last_status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(last_body["success"], Value::Bool(false));
        assert_eq!(
            last_body["message"],
            Value::String("Rate limit exceeded. Try again later.".to_string())
        );

        let mut connection = state
            .redis
            .get_multiplexed_async_connection()
            .await
            .expect("redis connection");
        let rate_limit_exists: bool = connection.exists(&rate_limit_key).await.expect("exists");
        assert!(rate_limit_exists);

        let _: i64 = redis::cmd("DEL")
            .arg(&rate_limit_key)
            .query_async(&mut connection)
            .await
            .expect("cleanup rate limit key");

        cleanup_toko_fixture(&state.db, user_id, toko_id, personal_access_token_id).await;
    }
}
