use std::collections::BTreeMap;

use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};

/// Typed HTTP client for NexusGGR upstream.
/// Current scope: `provider_list`, `game_list`, `game_list_v2`, `game_launch`,
/// `money_info`, `user_create`, `user_deposit`, `user_withdraw`,
/// `user_withdraw_reset`, `transfer_status`, `call_players`, `call_list`,
/// `call_apply`, `call_history`, `call_cancel`, `control_rtp`, and
/// `control_users_rtp`.
#[derive(Debug, Clone)]
pub struct NexusggrClient {
    http: Client,
    base_url: String,
    agent_code: String,
    agent_token: String,
}

impl NexusggrClient {
    pub fn new(
        base_url: impl Into<String>,
        agent_code: impl Into<String>,
        agent_token: impl Into<String>,
    ) -> Result<Self, NexusggrError> {
        Self::with_http(Client::new(), base_url, agent_code, agent_token)
    }

    pub fn with_http(
        http: Client,
        base_url: impl Into<String>,
        agent_code: impl Into<String>,
        agent_token: impl Into<String>,
    ) -> Result<Self, NexusggrError> {
        let base_url = normalize_required(base_url.into(), "NEXUSGGR_API_URL")?;
        let agent_code = normalize_required(agent_code.into(), "NEXUSGGR_AGENT_CODE")?;
        let agent_token = normalize_required(agent_token.into(), "NEXUSGGR_AGENT_TOKEN")?;

        Ok(Self {
            http,
            base_url,
            agent_code,
            agent_token,
        })
    }

    pub async fn provider_list(&self) -> Result<ProviderListResponse, NexusggrError> {
        let raw: RawProviderListResponse = self.call("provider_list", None).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "provider_list",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(ProviderListResponse {
            providers: raw.providers.unwrap_or_default(),
        })
    }

    pub async fn game_list(&self, provider_code: &str) -> Result<GameListResponse, NexusggrError> {
        let raw: RawGameListResponse = self
            .call("game_list", Some(provider_code_param(provider_code)?))
            .await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "game_list",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(GameListResponse {
            games: raw.games.unwrap_or_default(),
        })
    }

    pub async fn game_list_v2(
        &self,
        provider_code: &str,
    ) -> Result<GameListV2Response, NexusggrError> {
        let raw: RawGameListV2Response = self
            .call("game_list_v2", Some(provider_code_param(provider_code)?))
            .await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "game_list_v2",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(GameListV2Response {
            games: raw
                .games
                .unwrap_or_default()
                .into_iter()
                .map(GameRecordV2::from)
                .collect(),
        })
    }

    pub async fn game_launch(
        &self,
        request: &GameLaunchRequest,
    ) -> Result<GameLaunchResponse, NexusggrError> {
        let mut params = provider_code_param(&request.provider_code)?;
        params.insert(
            "user_code".to_string(),
            Value::String(request.user_code.clone()),
        );
        params.insert("lang".to_string(), Value::String(request.lang.clone()));

        if let Some(game_code) = request.game_code.as_ref() {
            params.insert("game_code".to_string(), Value::String(game_code.clone()));
        }

        let raw: RawGameLaunchResponse = self.call("game_launch", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "game_launch",
                status: raw.status,
                message: raw.msg,
            });
        }

        let launch_url = raw
            .launch_url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                NexusggrError::InvalidResponse(
                    "game_launch succeeded without launch_url".to_string(),
                )
            })?;

        Ok(GameLaunchResponse { launch_url })
    }

    pub async fn money_info(
        &self,
        user_code: Option<&str>,
        all_users: bool,
    ) -> Result<MoneyInfoResponse, NexusggrError> {
        let mut params = Map::new();

        if let Some(user_code) = user_code.map(str::trim).filter(|value| !value.is_empty()) {
            params.insert(
                "user_code".to_string(),
                Value::String(user_code.to_string()),
            );
        }

        if all_users {
            params.insert("all_users".to_string(), Value::Bool(true));
        }

        let raw: RawMoneyInfoResponse = self.call("money_info", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "money_info",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(MoneyInfoResponse {
            user: raw.user.map(TypedMoneyInfoUser::try_from).transpose()?,
            user_list: raw
                .user_list
                .unwrap_or_default()
                .into_iter()
                .map(TypedMoneyInfoUser::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn user_create(&self, user_code: &str) -> Result<UserCreateResponse, NexusggrError> {
        let raw: RawUserCreateResponse = self
            .call(
                "user_create",
                Some(required_string_param("user_code", user_code)?),
            )
            .await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "user_create",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(UserCreateResponse {
            user_code: raw.user_code,
        })
    }

    pub async fn user_deposit(
        &self,
        request: &UserDepositRequest,
    ) -> Result<UserDepositResponse, NexusggrError> {
        let mut params = required_string_param("user_code", &request.user_code)?;
        if request.amount < 1 {
            return Err(NexusggrError::InvalidConfig(
                "amount must be greater than 0".to_string(),
            ));
        }

        params.insert("amount".to_string(), Value::Number(request.amount.into()));

        if let Some(agent_sign) = request
            .agent_sign
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            params.insert(
                "agent_sign".to_string(),
                Value::String(agent_sign.to_string()),
            );
        }

        let raw: RawUserDepositResponse = self.call("user_deposit", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "user_deposit",
                status: raw.status,
                message: raw.msg,
            });
        }

        UserDepositResponse::try_from(raw)
    }

    pub async fn user_withdraw(
        &self,
        request: &UserWithdrawRequest,
    ) -> Result<UserWithdrawResponse, NexusggrError> {
        let mut params = required_string_param("user_code", &request.user_code)?;
        if request.amount < 1 {
            return Err(NexusggrError::InvalidConfig(
                "amount must be greater than 0".to_string(),
            ));
        }

        params.insert("amount".to_string(), Value::Number(request.amount.into()));

        if let Some(agent_sign) = request
            .agent_sign
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            params.insert(
                "agent_sign".to_string(),
                Value::String(agent_sign.to_string()),
            );
        }

        let raw: RawUserWithdrawResponse = self.call("user_withdraw", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "user_withdraw",
                status: raw.status,
                message: raw.msg,
            });
        }

        UserWithdrawResponse::try_from(raw)
    }

    pub async fn user_withdraw_reset(
        &self,
        user_code: Option<&str>,
        all_users: bool,
    ) -> Result<UserWithdrawResetResponse, NexusggrError> {
        let mut params = Map::new();

        if let Some(user_code) = user_code.map(str::trim).filter(|value| !value.is_empty()) {
            params.insert(
                "user_code".to_string(),
                Value::String(user_code.to_string()),
            );
        }

        if all_users {
            params.insert("all_users".to_string(), Value::Bool(true));
        }

        let raw: RawUserWithdrawResetResponse =
            self.call("user_withdraw_reset", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "user_withdraw_reset",
                status: raw.status,
                message: raw.msg,
            });
        }

        UserWithdrawResetResponse::try_from(raw)
    }

    pub async fn transfer_status(
        &self,
        user_code: &str,
        agent_sign: &str,
    ) -> Result<TransferStatusResponse, NexusggrError> {
        let mut params = required_string_param("user_code", user_code)?;
        params.extend(required_string_param("agent_sign", agent_sign)?);

        let raw: RawTransferStatusResponse = self.call("transfer_status", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "transfer_status",
                status: raw.status,
                message: raw.msg,
            });
        }

        TransferStatusResponse::try_from(raw)
    }

    pub async fn call_players(&self) -> Result<CallPlayersResponse, NexusggrError> {
        let raw: RawCallPlayersResponse = self.call("call_players", None).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "call_players",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(CallPlayersResponse {
            data: raw
                .data
                .unwrap_or_default()
                .into_iter()
                .map(CallPlayerRecord::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn call_list(
        &self,
        provider_code: &str,
        game_code: &str,
    ) -> Result<CallListResponse, NexusggrError> {
        let mut params = provider_code_param(provider_code)?;
        params.extend(required_string_param("game_code", game_code)?);

        let raw: RawCallListResponse = self.call("call_list", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "call_list",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(CallListResponse {
            calls: raw.calls.unwrap_or_default(),
        })
    }

    pub async fn call_apply(
        &self,
        request: &CallApplyRequest,
    ) -> Result<CallApplyResponse, NexusggrError> {
        let mut params = provider_code_param(&request.provider_code)?;
        params.extend(required_string_param("game_code", &request.game_code)?);
        params.extend(required_string_param("user_code", &request.user_code)?);
        params.insert(
            "call_rtp".to_string(),
            Value::Number(request.call_rtp.into()),
        );
        params.insert(
            "call_type".to_string(),
            Value::Number(request.call_type.into()),
        );

        let raw: RawCallApplyResponse = self.call("call_apply", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "call_apply",
                status: raw.status,
                message: raw.msg,
            });
        }

        CallApplyResponse::try_from(raw)
    }

    pub async fn call_history(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<CallHistoryResponse, NexusggrError> {
        let mut params = Map::new();
        params.insert("offset".to_string(), Value::Number(offset.into()));
        params.insert("limit".to_string(), Value::Number(limit.into()));

        let raw: RawCallHistoryResponse = self.call("call_history", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "call_history",
                status: raw.status,
                message: raw.msg,
            });
        }

        Ok(CallHistoryResponse {
            data: raw
                .data
                .unwrap_or_default()
                .into_iter()
                .map(CallHistoryRecord::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn call_cancel(&self, call_id: i64) -> Result<CallCancelResponse, NexusggrError> {
        let mut params = Map::new();
        params.insert("call_id".to_string(), Value::Number(call_id.into()));

        let raw: RawCallCancelResponse = self.call("call_cancel", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "call_cancel",
                status: raw.status,
                message: raw.msg,
            });
        }

        CallCancelResponse::try_from(raw)
    }

    pub async fn control_rtp(
        &self,
        provider_code: &str,
        user_code: &str,
        rtp: f64,
    ) -> Result<ControlRtpResponse, NexusggrError> {
        if rtp < 0.0 {
            return Err(NexusggrError::InvalidConfig(
                "rtp must be at least 0".to_string(),
            ));
        }

        let mut params = provider_code_param(provider_code)?;
        params.extend(required_string_param("user_code", user_code)?);
        params.insert(
            "rtp".to_string(),
            serde_json::Number::from_f64(rtp)
                .map(Value::Number)
                .ok_or_else(|| NexusggrError::InvalidConfig("rtp must be finite".to_string()))?,
        );

        let raw: RawControlRtpResponse = self.call("control_rtp", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "control_rtp",
                status: raw.status,
                message: raw.msg,
            });
        }

        ControlRtpResponse::try_from(raw)
    }

    pub async fn control_users_rtp(
        &self,
        user_codes: &[String],
        rtp: f64,
    ) -> Result<ControlUsersRtpResponse, NexusggrError> {
        if user_codes.is_empty() {
            return Err(NexusggrError::InvalidConfig(
                "user_codes must not be empty".to_string(),
            ));
        }
        if rtp < 0.0 {
            return Err(NexusggrError::InvalidConfig(
                "rtp must be at least 0".to_string(),
            ));
        }

        let normalized_user_codes = user_codes
            .iter()
            .map(|value| value.trim())
            .map(|value| {
                if value.is_empty() {
                    Err(NexusggrError::InvalidConfig(
                        "user_codes must not contain empty values".to_string(),
                    ))
                } else {
                    Ok(value.to_string())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut params = Map::new();
        params.insert(
            "user_codes".to_string(),
            Value::String(
                serde_json::to_string(&normalized_user_codes)
                    .map_err(|err| NexusggrError::InvalidConfig(err.to_string()))?,
            ),
        );
        params.insert(
            "rtp".to_string(),
            serde_json::Number::from_f64(rtp)
                .map(Value::Number)
                .ok_or_else(|| NexusggrError::InvalidConfig("rtp must be finite".to_string()))?,
        );

        let raw: RawControlUsersRtpResponse = self.call("control_users_rtp", Some(params)).await?;

        if raw.status != 1 {
            return Err(NexusggrError::UpstreamFailure {
                method: "control_users_rtp",
                status: raw.status,
                message: raw.msg,
            });
        }

        ControlUsersRtpResponse::try_from(raw)
    }

    async fn call<T>(
        &self,
        method: &'static str,
        params: Option<Map<String, Value>>,
    ) -> Result<T, NexusggrError>
    where
        T: DeserializeOwned,
    {
        let mut payload = Map::new();
        payload.insert("method".to_string(), Value::String(method.to_string()));
        payload.insert(
            "agent_code".to_string(),
            Value::String(self.agent_code.clone()),
        );
        payload.insert(
            "agent_token".to_string(),
            Value::String(self.agent_token.clone()),
        );

        for (key, value) in params.unwrap_or_default() {
            payload.insert(key, value);
        }

        let response = self
            .http
            .post(self.endpoint_url())
            .json(&payload)
            .send()
            .await
            .map_err(NexusggrError::Transport)?
            .error_for_status()
            .map_err(NexusggrError::Transport)?;

        response.json::<T>().await.map_err(NexusggrError::Transport)
    }

    fn endpoint_url(&self) -> String {
        format!("{}/", self.base_url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderListResponse {
    pub providers: Vec<ProviderRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameListResponse {
    pub games: Vec<GameRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameListV2Response {
    pub games: Vec<GameRecordV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameLaunchRequest {
    pub user_code: String,
    pub provider_code: String,
    pub lang: String,
    pub game_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameLaunchResponse {
    pub launch_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoneyInfoResponse {
    pub user: Option<TypedMoneyInfoUser>,
    pub user_list: Vec<TypedMoneyInfoUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypedMoneyInfoUser {
    pub user_code: Option<String>,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserCreateResponse {
    pub user_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDepositRequest {
    pub user_code: String,
    pub amount: i64,
    pub agent_sign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDepositResponse {
    pub agent_balance: i64,
    pub user_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserWithdrawRequest {
    pub user_code: String,
    pub amount: i64,
    pub agent_sign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserWithdrawResponse {
    pub agent_balance: i64,
    pub user_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserWithdrawResetResponse {
    pub agent: Option<UserWithdrawResetAgent>,
    pub user: Option<UserWithdrawResetUser>,
    pub user_list: Vec<UserWithdrawResetUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserWithdrawResetAgent {
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserWithdrawResetUser {
    pub user_code: Option<String>,
    pub withdraw_amount: i64,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferStatusResponse {
    pub amount: i64,
    pub r#type: Option<String>,
    pub agent_balance: i64,
    pub user_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPlayersResponse {
    pub data: Vec<CallPlayerRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallListResponse {
    pub calls: Vec<CallListRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallApplyRequest {
    pub provider_code: String,
    pub game_code: String,
    pub user_code: String,
    pub call_rtp: i64,
    pub call_type: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallApplyResponse {
    pub called_money: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallHistoryResponse {
    pub data: Vec<CallHistoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallCancelResponse {
    pub canceled_money: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlRtpResponse {
    pub changed_rtp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlUsersRtpResponse {
    pub changed_rtp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPlayerRecord {
    pub user_code: Option<String>,
    pub provider_code: Option<String>,
    pub game_code: Option<String>,
    pub bet: i64,
    pub balance: i64,
    pub total_debit: i64,
    pub total_credit: i64,
    pub target_rtp: i64,
    pub real_rtp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallListRecord {
    pub rtp: Option<i64>,
    pub call_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallHistoryRecord {
    pub id: i64,
    pub user_code: Option<String>,
    pub provider_code: Option<String>,
    pub game_code: Option<String>,
    pub bet: i64,
    pub user_prev: i64,
    pub user_after: i64,
    pub agent_prev: i64,
    pub agent_after: i64,
    pub expect: i64,
    pub missed: i64,
    pub real: i64,
    pub rtp: i64,
    pub r#type: Option<String>,
    pub status: i64,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRecord {
    pub code: String,
    pub name: String,
    pub status: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameRecord {
    pub id: Option<i64>,
    pub game_code: Option<String>,
    pub game_name: Option<String>,
    pub banner: Option<String>,
    pub status: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameRecordV2 {
    pub id: Option<i64>,
    pub game_code: Option<String>,
    pub game_name: Option<BTreeMap<String, String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum NexusggrError {
    #[error("NexusGGR client config invalid: {0}")]
    InvalidConfig(String),

    #[error("NexusGGR transport error")]
    Transport(reqwest::Error),

    #[error("NexusGGR response invalid: {0}")]
    InvalidResponse(String),

    #[error("NexusGGR upstream {method} failed")]
    UpstreamFailure {
        method: &'static str,
        status: i64,
        message: Option<String>,
    },
}

impl NexusggrError {
    pub fn upstream_message(&self) -> Option<&str> {
        match self {
            Self::UpstreamFailure {
                message: Some(message),
                ..
            } => Some(message.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawProviderListResponse {
    status: i64,
    msg: Option<String>,
    providers: Option<Vec<ProviderRecord>>,
}

#[derive(Debug, Deserialize)]
struct RawGameListResponse {
    status: i64,
    msg: Option<String>,
    games: Option<Vec<GameRecord>>,
}

#[derive(Debug, Deserialize)]
struct RawGameListV2Response {
    status: i64,
    msg: Option<String>,
    games: Option<Vec<RawGameRecordV2>>,
}

#[derive(Debug, Deserialize)]
struct RawGameLaunchResponse {
    status: i64,
    msg: Option<String>,
    launch_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMoneyInfoResponse {
    status: i64,
    msg: Option<String>,
    user: Option<RawMoneyInfoUser>,
    user_list: Option<Vec<RawMoneyInfoUser>>,
}

#[derive(Debug, Deserialize)]
struct RawUserCreateResponse {
    status: i64,
    msg: Option<String>,
    user_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawUserDepositResponse {
    status: i64,
    msg: Option<String>,
    agent_balance: Option<Value>,
    user_balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawUserWithdrawResponse {
    status: i64,
    msg: Option<String>,
    agent_balance: Option<Value>,
    user_balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawUserWithdrawResetResponse {
    status: i64,
    msg: Option<String>,
    agent: Option<RawUserWithdrawResetAgent>,
    user: Option<RawUserWithdrawResetUser>,
    user_list: Option<Vec<RawUserWithdrawResetUser>>,
}

#[derive(Debug, Deserialize)]
struct RawTransferStatusResponse {
    status: i64,
    msg: Option<String>,
    amount: Option<Value>,
    r#type: Option<String>,
    agent_balance: Option<Value>,
    user_balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawCallPlayersResponse {
    status: i64,
    msg: Option<String>,
    data: Option<Vec<RawCallPlayerRecord>>,
}

#[derive(Debug, Deserialize)]
struct RawCallListResponse {
    status: i64,
    msg: Option<String>,
    calls: Option<Vec<CallListRecord>>,
}

#[derive(Debug, Deserialize)]
struct RawCallApplyResponse {
    status: i64,
    msg: Option<String>,
    called_money: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawCallHistoryResponse {
    status: i64,
    msg: Option<String>,
    data: Option<Vec<RawCallHistoryRecord>>,
}

#[derive(Debug, Deserialize)]
struct RawCallCancelResponse {
    status: i64,
    msg: Option<String>,
    canceled_money: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawControlRtpResponse {
    status: i64,
    msg: Option<String>,
    changed_rtp: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawControlUsersRtpResponse {
    status: i64,
    msg: Option<String>,
    changed_rtp: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawMoneyInfoUser {
    user_code: Option<String>,
    balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawUserWithdrawResetAgent {
    balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawUserWithdrawResetUser {
    user_code: Option<String>,
    withdraw_amount: Option<Value>,
    balance: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawCallPlayerRecord {
    user_code: Option<String>,
    provider_code: Option<String>,
    game_code: Option<String>,
    bet: Option<Value>,
    balance: Option<Value>,
    total_debit: Option<Value>,
    total_credit: Option<Value>,
    target_rtp: Option<Value>,
    real_rtp: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawCallHistoryRecord {
    id: Option<Value>,
    user_code: Option<String>,
    provider_code: Option<String>,
    game_code: Option<String>,
    bet: Option<Value>,
    user_prev: Option<Value>,
    user_after: Option<Value>,
    agent_prev: Option<Value>,
    agent_after: Option<Value>,
    expect: Option<Value>,
    missed: Option<Value>,
    real: Option<Value>,
    rtp: Option<Value>,
    r#type: Option<String>,
    status: Option<Value>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawGameRecordV2 {
    id: Option<i64>,
    game_code: Option<String>,
    game_name: Option<RawLocalizedGameName>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawLocalizedGameName {
    Localized(BTreeMap<String, String>),
    Plain(String),
}

fn normalize_required(value: String, key: &str) -> Result<String, NexusggrError> {
    let normalized = value.trim().trim_end_matches('/').to_string();

    if normalized.is_empty() {
        return Err(NexusggrError::InvalidConfig(format!(
            "{key} must not be empty"
        )));
    }

    Ok(normalized)
}

fn provider_code_param(provider_code: &str) -> Result<Map<String, Value>, NexusggrError> {
    required_string_param("provider_code", provider_code)
}

fn required_string_param(
    key: &'static str,
    value: &str,
) -> Result<Map<String, Value>, NexusggrError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NexusggrError::InvalidConfig(format!(
            "{key} must not be empty"
        )));
    }

    let mut params = Map::new();
    params.insert(key.to_string(), Value::String(value.to_string()));

    Ok(params)
}

impl From<RawGameRecordV2> for GameRecordV2 {
    fn from(value: RawGameRecordV2) -> Self {
        Self {
            id: value.id,
            game_code: value.game_code,
            game_name: value.game_name.map(normalize_localized_game_name),
        }
    }
}

fn normalize_localized_game_name(value: RawLocalizedGameName) -> BTreeMap<String, String> {
    match value {
        RawLocalizedGameName::Localized(map) => map,
        RawLocalizedGameName::Plain(name) => {
            let mut map = BTreeMap::new();
            map.insert("default".to_string(), name);
            map
        }
    }
}

impl TryFrom<RawMoneyInfoUser> for TypedMoneyInfoUser {
    type Error = NexusggrError;

    fn try_from(value: RawMoneyInfoUser) -> Result<Self, Self::Error> {
        Ok(Self {
            user_code: value.user_code,
            balance: normalize_money_value(value.balance)?,
        })
    }
}

impl TryFrom<RawUserDepositResponse> for UserDepositResponse {
    type Error = NexusggrError;

    fn try_from(value: RawUserDepositResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            agent_balance: normalize_money_value(value.agent_balance)?,
            user_balance: normalize_money_value(value.user_balance)?,
        })
    }
}

impl TryFrom<RawUserWithdrawResponse> for UserWithdrawResponse {
    type Error = NexusggrError;

    fn try_from(value: RawUserWithdrawResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            agent_balance: normalize_money_value(value.agent_balance)?,
            user_balance: normalize_money_value(value.user_balance)?,
        })
    }
}

impl TryFrom<RawUserWithdrawResetResponse> for UserWithdrawResetResponse {
    type Error = NexusggrError;

    fn try_from(value: RawUserWithdrawResetResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            agent: value
                .agent
                .map(UserWithdrawResetAgent::try_from)
                .transpose()?,
            user: value
                .user
                .map(UserWithdrawResetUser::try_from)
                .transpose()?,
            user_list: value
                .user_list
                .unwrap_or_default()
                .into_iter()
                .map(UserWithdrawResetUser::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<RawUserWithdrawResetAgent> for UserWithdrawResetAgent {
    type Error = NexusggrError;

    fn try_from(value: RawUserWithdrawResetAgent) -> Result<Self, Self::Error> {
        Ok(Self {
            balance: normalize_money_value(value.balance)?,
        })
    }
}

impl TryFrom<RawUserWithdrawResetUser> for UserWithdrawResetUser {
    type Error = NexusggrError;

    fn try_from(value: RawUserWithdrawResetUser) -> Result<Self, Self::Error> {
        Ok(Self {
            user_code: value.user_code,
            withdraw_amount: normalize_money_value(value.withdraw_amount)?,
            balance: normalize_money_value(value.balance)?,
        })
    }
}

impl TryFrom<RawTransferStatusResponse> for TransferStatusResponse {
    type Error = NexusggrError;

    fn try_from(value: RawTransferStatusResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            amount: normalize_money_value(value.amount)?,
            r#type: value.r#type,
            agent_balance: normalize_money_value(value.agent_balance)?,
            user_balance: normalize_money_value(value.user_balance)?,
        })
    }
}

impl TryFrom<RawCallPlayerRecord> for CallPlayerRecord {
    type Error = NexusggrError;

    fn try_from(value: RawCallPlayerRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            user_code: value.user_code,
            provider_code: value.provider_code,
            game_code: value.game_code,
            bet: normalize_money_value(value.bet)?,
            balance: normalize_money_value(value.balance)?,
            total_debit: normalize_money_value(value.total_debit)?,
            total_credit: normalize_money_value(value.total_credit)?,
            target_rtp: normalize_money_value(value.target_rtp)?,
            real_rtp: normalize_money_value(value.real_rtp)?,
        })
    }
}

impl TryFrom<RawCallApplyResponse> for CallApplyResponse {
    type Error = NexusggrError;

    fn try_from(value: RawCallApplyResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            called_money: normalize_money_value(value.called_money)?,
        })
    }
}

impl TryFrom<RawCallHistoryRecord> for CallHistoryRecord {
    type Error = NexusggrError;

    fn try_from(value: RawCallHistoryRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: normalize_money_value(value.id)?,
            user_code: value.user_code,
            provider_code: value.provider_code,
            game_code: value.game_code,
            bet: normalize_money_value(value.bet)?,
            user_prev: normalize_money_value(value.user_prev)?,
            user_after: normalize_money_value(value.user_after)?,
            agent_prev: normalize_money_value(value.agent_prev)?,
            agent_after: normalize_money_value(value.agent_after)?,
            expect: normalize_money_value(value.expect)?,
            missed: normalize_money_value(value.missed)?,
            real: normalize_money_value(value.real)?,
            rtp: normalize_money_value(value.rtp)?,
            r#type: value.r#type,
            status: normalize_money_value(value.status)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

impl TryFrom<RawCallCancelResponse> for CallCancelResponse {
    type Error = NexusggrError;

    fn try_from(value: RawCallCancelResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            canceled_money: normalize_money_value(value.canceled_money)?,
        })
    }
}

impl TryFrom<RawControlRtpResponse> for ControlRtpResponse {
    type Error = NexusggrError;

    fn try_from(value: RawControlRtpResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            changed_rtp: normalize_decimal_value(value.changed_rtp, "changed_rtp")?,
        })
    }
}

impl TryFrom<RawControlUsersRtpResponse> for ControlUsersRtpResponse {
    type Error = NexusggrError;

    fn try_from(value: RawControlUsersRtpResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            changed_rtp: normalize_decimal_value(value.changed_rtp, "changed_rtp")?,
        })
    }
}

fn normalize_decimal_value(
    value: Option<Value>,
    field: &'static str,
) -> Result<f64, NexusggrError> {
    let Some(value) = value else {
        return Ok(0.0);
    };

    match value {
        Value::Null => Ok(0.0),
        Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| NexusggrError::InvalidResponse(format!("{field} must be numeric"))),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .map_err(|_| NexusggrError::InvalidResponse(format!("{field} is invalid: {text}"))),
        other => Err(NexusggrError::InvalidResponse(format!(
            "{field} must be numeric, got {other}"
        ))),
    }
}

fn normalize_money_value(value: Option<Value>) -> Result<i64, NexusggrError> {
    let Some(value) = value else {
        return Ok(0);
    };

    let text = match value {
        Value::Null => return Ok(0),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text,
        other => {
            return Err(NexusggrError::InvalidResponse(format!(
                "money_info balance must be number-like, got {other}"
            )))
        }
    };

    parse_money_text(&text).ok_or_else(|| {
        NexusggrError::InvalidResponse(format!("money_info balance is invalid: {text}"))
    })
}

fn parse_money_text(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let negative = trimmed.starts_with('-');
    let unsigned = if negative { &trimmed[1..] } else { trimmed };
    let integer_part = unsigned.split('.').next()?;

    if integer_part.is_empty() || !integer_part.chars().all(|char| char.is_ascii_digit()) {
        return None;
    }

    let parsed = integer_part.parse::<i64>().ok()?;
    Some(if negative { -parsed } else { parsed })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        normalize_decimal_value, normalize_localized_game_name, normalize_money_value,
        parse_money_text, provider_code_param, required_string_param, CallApplyResponse,
        CallCancelResponse, CallHistoryRecord, CallListRecord, CallPlayerRecord,
        ControlRtpResponse, ControlUsersRtpResponse, GameRecord, GameRecordV2, NexusggrClient,
        NexusggrError, ProviderRecord, RawCallApplyResponse, RawCallCancelResponse,
        RawCallHistoryRecord, RawCallListResponse, RawCallPlayerRecord, RawControlRtpResponse,
        RawControlUsersRtpResponse, RawGameLaunchResponse, RawGameListResponse,
        RawGameListV2Response, RawGameRecordV2, RawLocalizedGameName, RawMoneyInfoUser,
        RawProviderListResponse, RawTransferStatusResponse, RawUserCreateResponse,
        RawUserDepositResponse, RawUserWithdrawResetAgent, RawUserWithdrawResetResponse,
        RawUserWithdrawResetUser, RawUserWithdrawResponse, TransferStatusResponse,
        TypedMoneyInfoUser, UserDepositRequest, UserDepositResponse, UserWithdrawRequest,
        UserWithdrawResetAgent, UserWithdrawResetResponse, UserWithdrawResetUser,
        UserWithdrawResponse, Value,
    };

    #[test]
    fn normalizes_endpoint_url() {
        let client =
            NexusggrClient::new("https://api.nexusggr.com/", "agent-code", "agent-token").unwrap();

        assert_eq!(client.endpoint_url(), "https://api.nexusggr.com/");
    }

    #[test]
    fn rejects_empty_agent_code() {
        let error = NexusggrClient::new("https://api.nexusggr.com", "", "agent-token").unwrap_err();

        assert!(matches!(error, NexusggrError::InvalidConfig(_)));
    }

    #[test]
    fn maps_provider_list_upstream_failure() {
        let error = NexusggrError::UpstreamFailure {
            method: "provider_list",
            status: 0,
            message: Some("INVALID_AGENT".to_string()),
        };

        assert_eq!(error.upstream_message(), Some("INVALID_AGENT"));
    }

    #[test]
    fn provider_list_raw_response_allows_empty_list() {
        let raw = RawProviderListResponse {
            status: 1,
            msg: None,
            providers: Some(vec![ProviderRecord {
                code: "PG".to_string(),
                name: "Pocket Games".to_string(),
                status: 1,
            }]),
        };

        assert_eq!(raw.providers.unwrap().len(), 1);
    }

    #[test]
    fn maps_game_list_upstream_failure() {
        let error = NexusggrError::UpstreamFailure {
            method: "game_list",
            status: 0,
            message: Some("INVALID_PROVIDER".to_string()),
        };

        assert_eq!(error.upstream_message(), Some("INVALID_PROVIDER"));
    }

    #[test]
    fn game_list_raw_response_allows_nullable_fields() {
        let raw = RawGameListResponse {
            status: 1,
            msg: None,
            games: Some(vec![GameRecord {
                id: None,
                game_code: Some("mahjong".to_string()),
                game_name: Some("Mahjong Ways".to_string()),
                banner: None,
                status: Some(1),
            }]),
        };

        assert_eq!(raw.games.unwrap().len(), 1);
    }

    #[test]
    fn game_list_v2_raw_response_allows_localized_names() {
        let mut localized = BTreeMap::new();
        localized.insert("en".to_string(), "Mahjong Ways".to_string());
        localized.insert("id".to_string(), "Mahjong Ways ID".to_string());

        let raw = RawGameListV2Response {
            status: 1,
            msg: None,
            games: Some(vec![RawGameRecordV2 {
                id: Some(10),
                game_code: Some("mahjong".to_string()),
                game_name: Some(RawLocalizedGameName::Localized(localized)),
            }]),
        };

        assert_eq!(raw.games.unwrap().len(), 1);
    }

    #[test]
    fn provider_code_param_rejects_empty_value() {
        let error = provider_code_param("   ").unwrap_err();

        assert!(matches!(error, NexusggrError::InvalidConfig(_)));
    }

    #[test]
    fn required_string_param_rejects_empty_value() {
        let error = required_string_param("user_code", "   ").unwrap_err();

        assert!(matches!(error, NexusggrError::InvalidConfig(_)));
    }

    #[test]
    fn normalizes_plain_localized_game_name_to_default_key() {
        let normalized =
            normalize_localized_game_name(RawLocalizedGameName::Plain("Book of Dead".to_string()));

        assert_eq!(normalized.get("default"), Some(&"Book of Dead".to_string()));
    }

    #[test]
    fn converts_raw_game_record_v2_into_public_shape() {
        let record = GameRecordV2::from(RawGameRecordV2 {
            id: Some(1),
            game_code: Some("bookofdead".to_string()),
            game_name: Some(RawLocalizedGameName::Plain("Book of Dead".to_string())),
        });

        assert_eq!(record.game_code.as_deref(), Some("bookofdead"));
        assert_eq!(
            record
                .game_name
                .as_ref()
                .and_then(|value| value.get("default"))
                .map(String::as_str),
            Some("Book of Dead")
        );
    }

    #[test]
    fn game_launch_raw_response_requires_launch_url() {
        let raw = RawGameLaunchResponse {
            status: 1,
            msg: None,
            launch_url: None,
        };

        assert!(raw.launch_url.is_none());
    }

    #[test]
    fn invalid_response_error_exposes_no_upstream_message() {
        let error = NexusggrError::InvalidResponse("missing launch url".to_string());

        assert_eq!(error.upstream_message(), None);
    }

    #[test]
    fn parse_money_text_truncates_fraction_without_float_math() {
        assert_eq!(parse_money_text("1234.99"), Some(1234));
        assert_eq!(parse_money_text("-5.10"), Some(-5));
    }

    #[test]
    fn normalize_money_value_accepts_numeric_string() {
        let normalized = normalize_money_value(Some(Value::String("2500.00".to_string()))).unwrap();

        assert_eq!(normalized, 2500);
    }

    #[test]
    fn typed_money_info_user_converts_balance_to_integer() {
        let record = TypedMoneyInfoUser::try_from(RawMoneyInfoUser {
            user_code: Some("ext-user".to_string()),
            balance: Some(Value::String("1500.80".to_string())),
        })
        .unwrap();

        assert_eq!(record.user_code.as_deref(), Some("ext-user"));
        assert_eq!(record.balance, 1500);
    }

    #[test]
    fn user_create_raw_response_allows_optional_user_code() {
        let raw = RawUserCreateResponse {
            status: 1,
            msg: None,
            user_code: Some("01hxyplayer".to_string()),
        };

        assert_eq!(raw.user_code.as_deref(), Some("01hxyplayer"));
    }

    #[test]
    fn user_deposit_response_normalizes_balance_to_integer() {
        let response = UserDepositResponse::try_from(RawUserDepositResponse {
            status: 1,
            msg: None,
            agent_balance: Some(Value::String("150000.99".to_string())),
            user_balance: Some(Value::String("2500.80".to_string())),
        })
        .unwrap();

        assert_eq!(response.agent_balance, 150000);
        assert_eq!(response.user_balance, 2500);
    }

    #[test]
    fn user_deposit_request_supports_optional_agent_sign() {
        let request = UserDepositRequest {
            user_code: "ext-user".to_string(),
            amount: 10_000,
            agent_sign: Some("agent-sign-1".to_string()),
        };

        assert_eq!(request.agent_sign.as_deref(), Some("agent-sign-1"));
    }

    #[test]
    fn user_withdraw_response_normalizes_balance_to_integer() {
        let response = UserWithdrawResponse::try_from(RawUserWithdrawResponse {
            status: 1,
            msg: None,
            agent_balance: Some(Value::String("160000.55".to_string())),
            user_balance: Some(Value::String("100.40".to_string())),
        })
        .unwrap();

        assert_eq!(response.agent_balance, 160000);
        assert_eq!(response.user_balance, 100);
    }

    #[test]
    fn user_withdraw_request_supports_optional_agent_sign() {
        let request = UserWithdrawRequest {
            user_code: "ext-user".to_string(),
            amount: 10_000,
            agent_sign: Some("agent-sign-2".to_string()),
        };

        assert_eq!(request.agent_sign.as_deref(), Some("agent-sign-2"));
    }

    #[test]
    fn user_withdraw_reset_response_normalizes_money_fields() {
        let response = UserWithdrawResetResponse::try_from(RawUserWithdrawResetResponse {
            status: 1,
            msg: None,
            agent: Some(RawUserWithdrawResetAgent {
                balance: Some(Value::String("50000.70".to_string())),
            }),
            user: Some(RawUserWithdrawResetUser {
                user_code: Some("ext-user".to_string()),
                withdraw_amount: Some(Value::String("10000.40".to_string())),
                balance: Some(Value::String("0.90".to_string())),
            }),
            user_list: Some(vec![RawUserWithdrawResetUser {
                user_code: Some("ext-user-2".to_string()),
                withdraw_amount: Some(Value::String("25000".to_string())),
                balance: Some(Value::String("10".to_string())),
            }]),
        })
        .unwrap();

        assert_eq!(
            response.agent,
            Some(UserWithdrawResetAgent { balance: 50000 })
        );
        assert_eq!(
            response.user,
            Some(UserWithdrawResetUser {
                user_code: Some("ext-user".to_string()),
                withdraw_amount: 10000,
                balance: 0,
            })
        );
        assert_eq!(
            response.user_list,
            vec![UserWithdrawResetUser {
                user_code: Some("ext-user-2".to_string()),
                withdraw_amount: 25000,
                balance: 10,
            }]
        );
    }

    #[test]
    fn transfer_status_response_normalizes_money_fields() {
        let response = TransferStatusResponse::try_from(RawTransferStatusResponse {
            status: 1,
            msg: None,
            amount: Some(Value::String("10000.80".to_string())),
            r#type: Some("withdrawal".to_string()),
            agent_balance: Some(Value::String("75000.55".to_string())),
            user_balance: Some(Value::String("500.25".to_string())),
        })
        .unwrap();

        assert_eq!(response.amount, 10000);
        assert_eq!(response.r#type.as_deref(), Some("withdrawal"));
        assert_eq!(response.agent_balance, 75000);
        assert_eq!(response.user_balance, 500);
    }

    #[test]
    fn call_player_record_normalizes_numeric_fields() {
        let record = CallPlayerRecord::try_from(RawCallPlayerRecord {
            user_code: Some("ext-user".to_string()),
            provider_code: Some("PG".to_string()),
            game_code: Some("mahjong".to_string()),
            bet: Some(Value::String("1000.99".to_string())),
            balance: Some(Value::String("2000".to_string())),
            total_debit: Some(Value::String("5000.40".to_string())),
            total_credit: Some(Value::String("3000.20".to_string())),
            target_rtp: Some(Value::String("80".to_string())),
            real_rtp: Some(Value::String("60".to_string())),
        })
        .unwrap();

        assert_eq!(record.user_code.as_deref(), Some("ext-user"));
        assert_eq!(record.provider_code.as_deref(), Some("PG"));
        assert_eq!(record.game_code.as_deref(), Some("mahjong"));
        assert_eq!(record.bet, 1000);
        assert_eq!(record.balance, 2000);
        assert_eq!(record.total_debit, 5000);
        assert_eq!(record.total_credit, 3000);
        assert_eq!(record.target_rtp, 80);
        assert_eq!(record.real_rtp, 60);
    }

    #[test]
    fn call_list_raw_response_allows_records() {
        let raw = RawCallListResponse {
            status: 1,
            msg: None,
            calls: Some(vec![CallListRecord {
                rtp: Some(92),
                call_type: Some("Free".to_string()),
            }]),
        };

        assert_eq!(raw.calls.unwrap().len(), 1);
    }

    #[test]
    fn call_apply_response_normalizes_called_money() {
        let response = CallApplyResponse::try_from(RawCallApplyResponse {
            status: 1,
            msg: None,
            called_money: Some(Value::String("150000.90".to_string())),
        })
        .unwrap();

        assert_eq!(response.called_money, 150000);
    }

    #[test]
    fn call_history_record_normalizes_numeric_fields() {
        let record = CallHistoryRecord::try_from(RawCallHistoryRecord {
            id: Some(Value::String("10".to_string())),
            user_code: Some("ext-user".to_string()),
            provider_code: Some("PGSOFT".to_string()),
            game_code: Some("mahjong".to_string()),
            bet: Some(Value::String("1000.90".to_string())),
            user_prev: Some(Value::String("10000".to_string())),
            user_after: Some(Value::String("11000".to_string())),
            agent_prev: Some(Value::String("500000".to_string())),
            agent_after: Some(Value::String("499000".to_string())),
            expect: Some(Value::String("1200".to_string())),
            missed: Some(Value::String("200".to_string())),
            real: Some(Value::String("1000".to_string())),
            rtp: Some(Value::String("90".to_string())),
            r#type: Some("common".to_string()),
            status: Some(Value::String("2".to_string())),
            created_at: Some("2026-04-04T10:00:00Z".to_string()),
            updated_at: Some("2026-04-04T10:05:00Z".to_string()),
        })
        .unwrap();

        assert_eq!(record.id, 10);
        assert_eq!(record.user_code.as_deref(), Some("ext-user"));
        assert_eq!(record.bet, 1000);
        assert_eq!(record.user_prev, 10000);
        assert_eq!(record.user_after, 11000);
        assert_eq!(record.agent_prev, 500000);
        assert_eq!(record.agent_after, 499000);
        assert_eq!(record.expect, 1200);
        assert_eq!(record.missed, 200);
        assert_eq!(record.real, 1000);
        assert_eq!(record.rtp, 90);
        assert_eq!(record.r#type.as_deref(), Some("common"));
        assert_eq!(record.status, 2);
        assert_eq!(record.created_at.as_deref(), Some("2026-04-04T10:00:00Z"));
        assert_eq!(record.updated_at.as_deref(), Some("2026-04-04T10:05:00Z"));
    }

    #[test]
    fn call_cancel_response_normalizes_canceled_money() {
        let response = CallCancelResponse::try_from(RawCallCancelResponse {
            status: 1,
            msg: None,
            canceled_money: Some(Value::String("42000.70".to_string())),
        })
        .unwrap();

        assert_eq!(response.canceled_money, 42000);
    }

    #[test]
    fn control_rtp_response_normalizes_changed_rtp() {
        let response = ControlRtpResponse::try_from(RawControlRtpResponse {
            status: 1,
            msg: None,
            changed_rtp: Some(Value::String("91.5".to_string())),
        })
        .unwrap();

        assert_eq!(response.changed_rtp, 91.5);
    }

    #[test]
    fn control_users_rtp_response_normalizes_changed_rtp() {
        let response = ControlUsersRtpResponse::try_from(RawControlUsersRtpResponse {
            status: 1,
            msg: None,
            changed_rtp: Some(Value::String("88.25".to_string())),
        })
        .unwrap();

        assert_eq!(response.changed_rtp, 88.25);
    }

    #[test]
    fn normalize_decimal_value_rejects_non_numeric_value() {
        let error = normalize_decimal_value(Some(Value::Bool(true)), "changed_rtp").unwrap_err();

        assert!(matches!(error, NexusggrError::InvalidResponse(_)));
    }
}
