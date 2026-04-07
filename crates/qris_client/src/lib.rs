use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Typed HTTP client for QRIS upstream.
/// Current scope: foundation for `/api/generate` and `/api/checkstatus/v2/{trx_id}`.
#[derive(Debug, Clone)]
pub struct QrisClient {
    http: Client,
    base_url: String,
    merchant_uuid: String,
}

impl QrisClient {
    pub fn new(
        base_url: impl Into<String>,
        merchant_uuid: impl Into<String>,
    ) -> Result<Self, QrisError> {
        Self::with_http(Client::new(), base_url, merchant_uuid)
    }

    pub fn with_http(
        http: Client,
        base_url: impl Into<String>,
        merchant_uuid: impl Into<String>,
    ) -> Result<Self, QrisError> {
        let base_url = normalize_base_url(base_url.into())?;
        let merchant_uuid = normalize_merchant_uuid(merchant_uuid.into())?;

        Ok(Self {
            http,
            base_url,
            merchant_uuid,
        })
    }

    pub async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResponse, QrisError> {
        let payload = GeneratePayload {
            username: &request.username,
            amount: request.amount,
            uuid: &self.merchant_uuid,
            expire: request.expire,
            custom_ref: request.custom_ref.as_deref(),
        };

        let response = self
            .http
            .post(self.endpoint_url("generate"))
            .json(&payload)
            .send()
            .await
            .map_err(QrisError::Transport)?
            .error_for_status()
            .map_err(QrisError::Transport)?;

        let raw = response
            .json::<RawGenerateResponse>()
            .await
            .map_err(QrisError::Transport)?;

        parse_generate_response(raw)
    }

    pub async fn check_status(
        &self,
        trx_id: &str,
        client_code: &str,
        client_key: &str,
    ) -> Result<CheckStatusResponse, QrisError> {
        let trx_id = trx_id.trim();
        if trx_id.is_empty() {
            return Err(QrisError::InvalidConfig(
                "trx_id must not be empty".to_string(),
            ));
        }

        let client_code = client_code
            .trim()
            .strip_prefix("")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| QrisError::InvalidConfig("QRIS_CLIENT must not be empty".to_string()))?;
        let client_key = client_key
            .trim()
            .strip_prefix("")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                QrisError::InvalidConfig("QRIS_CLIENT_KEY must not be empty".to_string())
            })?;

        let response = self
            .http
            .post(self.endpoint_url(&format!("checkstatus/v2/{trx_id}")))
            .json(&CheckStatusPayload {
                uuid: &self.merchant_uuid,
                client: client_code,
                client_key,
            })
            .send()
            .await
            .map_err(QrisError::Transport)?
            .error_for_status()
            .map_err(QrisError::Transport)?;

        let raw = response
            .json::<RawCheckStatusResponse>()
            .await
            .map_err(QrisError::Transport)?;

        parse_check_status_response(raw)
    }

    fn endpoint_url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerateRequest {
    pub username: String,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_ref: Option<String>,
}

impl GenerateRequest {
    pub fn new(username: impl Into<String>, amount: i64) -> Self {
        Self {
            username: username.into(),
            amount,
            expire: None,
            custom_ref: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerateResponse {
    pub data: String,
    pub trx_id: String,
    pub expired_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckStatusResponse {
    pub status: String,
    pub amount: Option<i64>,
    pub merchant_id: Option<String>,
    pub trx_id: Option<String>,
    pub rrn: Option<String>,
    pub created_at: Option<String>,
    pub finish_at: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum QrisError {
    #[error("QRIS client config invalid: {0}")]
    InvalidConfig(String),

    #[error("QRIS transport error")]
    Transport(reqwest::Error),

    #[error("QRIS upstream {operation} failed")]
    UpstreamFailure {
        operation: &'static str,
        message: Option<String>,
    },

    #[error("QRIS upstream response invalid: {0}")]
    InvalidResponse(String),
}

impl QrisError {
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

#[derive(Debug, Serialize)]
struct GeneratePayload<'a> {
    username: &'a str,
    amount: i64,
    uuid: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expire: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_ref: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct CheckStatusPayload<'a> {
    uuid: &'a str,
    client: &'a str,
    client_key: &'a str,
}

#[derive(Debug, Deserialize)]
struct RawGenerateResponse {
    status: bool,
    data: Option<String>,
    trx_id: Option<String>,
    expired_at: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCheckStatusResponse {
    status: Option<String>,
    amount: Option<i64>,
    merchant_id: Option<String>,
    trx_id: Option<String>,
    rrn: Option<String>,
    created_at: Option<String>,
    finish_at: Option<String>,
    error: Option<String>,
}

fn parse_generate_response(raw: RawGenerateResponse) -> Result<GenerateResponse, QrisError> {
    if !raw.status {
        return Err(QrisError::UpstreamFailure {
            operation: "generate",
            message: raw.error,
        });
    }

    let data = require_non_empty(raw.data, "data")?;
    let trx_id = require_non_empty(raw.trx_id, "trx_id")?;

    Ok(GenerateResponse {
        data,
        trx_id,
        expired_at: raw.expired_at,
    })
}

fn parse_check_status_response(
    raw: RawCheckStatusResponse,
) -> Result<CheckStatusResponse, QrisError> {
    let status = raw
        .status
        .map(|status| status.trim().to_string())
        .filter(|status| !status.is_empty())
        .ok_or_else(|| QrisError::UpstreamFailure {
            operation: "check_status",
            message: raw.error,
        })?;

    Ok(CheckStatusResponse {
        status,
        amount: raw.amount,
        merchant_id: raw.merchant_id,
        trx_id: raw.trx_id,
        rrn: raw.rrn,
        created_at: raw.created_at,
        finish_at: raw.finish_at,
    })
}

fn require_non_empty(value: Option<String>, field: &str) -> Result<String, QrisError> {
    let value = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| QrisError::InvalidResponse(format!("missing {field}")))?;

    Ok(value)
}

fn normalize_base_url(base_url: String) -> Result<String, QrisError> {
    let normalized = base_url.trim().trim_end_matches('/').to_string();

    if normalized.is_empty() {
        return Err(QrisError::InvalidConfig(
            "QRIS_API_URL must not be empty".to_string(),
        ));
    }

    Ok(normalized)
}

fn normalize_merchant_uuid(merchant_uuid: String) -> Result<String, QrisError> {
    let normalized = merchant_uuid.trim().to_string();

    if normalized.is_empty() {
        return Err(QrisError::InvalidConfig(
            "QRIS_MERCHANT_UUID must not be empty".to_string(),
        ));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_check_status_response, parse_generate_response, GenerateRequest, QrisClient,
        QrisError, RawCheckStatusResponse, RawGenerateResponse,
    };

    #[test]
    fn normalizes_generate_endpoint_url() {
        let client = QrisClient::new("https://rest.otomatis.vip/api/", "merchant-uuid").unwrap();

        assert_eq!(
            client.endpoint_url("/generate"),
            "https://rest.otomatis.vip/api/generate"
        );
    }

    #[test]
    fn rejects_missing_generate_response_fields() {
        let error = parse_generate_response(RawGenerateResponse {
            status: true,
            data: Some("".to_string()),
            trx_id: Some("trx-123".to_string()),
            expired_at: None,
            error: None,
        })
        .unwrap_err();

        assert!(matches!(error, QrisError::InvalidResponse(_)));
    }

    #[test]
    fn maps_failed_generate_response_to_upstream_failure() {
        let error = parse_generate_response(RawGenerateResponse {
            status: false,
            data: None,
            trx_id: None,
            expired_at: None,
            error: Some("Toko not valid".to_string()),
        })
        .unwrap_err();

        assert!(matches!(
            error,
            QrisError::UpstreamFailure {
                operation: "generate",
                ..
            }
        ));
        assert_eq!(error.upstream_message(), Some("Toko not valid"));
    }

    #[test]
    fn builds_generate_request_with_optional_fields_empty_by_default() {
        let request = GenerateRequest::new("terminal-01", 10_000);

        assert_eq!(request.username, "terminal-01");
        assert_eq!(request.amount, 10_000);
        assert_eq!(request.expire, None);
        assert_eq!(request.custom_ref, None);
    }

    #[test]
    fn parses_upstream_check_status_response() {
        let response = parse_check_status_response(RawCheckStatusResponse {
            status: Some("paid".to_string()),
            amount: Some(10_000),
            merchant_id: Some("merchant-01".to_string()),
            trx_id: Some("trx-123".to_string()),
            rrn: Some("rrn-123".to_string()),
            created_at: Some("2026-04-07T10:00:00Z".to_string()),
            finish_at: None,
            error: None,
        })
        .unwrap();

        assert_eq!(response.status, "paid");
        assert_eq!(response.trx_id.as_deref(), Some("trx-123"));
    }

    #[test]
    fn maps_failed_check_status_to_upstream_failure() {
        let error = parse_check_status_response(RawCheckStatusResponse {
            status: None,
            amount: None,
            merchant_id: None,
            trx_id: None,
            rrn: None,
            created_at: None,
            finish_at: None,
            error: Some("Toko not valid".to_string()),
        })
        .unwrap_err();

        assert!(matches!(
            error,
            QrisError::UpstreamFailure {
                operation: "check_status",
                ..
            }
        ));
        assert_eq!(error.upstream_message(), Some("Toko not valid"));
    }
}
