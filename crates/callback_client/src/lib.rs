use std::time::Duration;

use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE},
    Client, Url,
};
use serde::Serialize;
use serde_json::Value;

const CALLBACK_TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, Clone)]
pub struct CallbackClient {
    http: Client,
}

impl CallbackClient {
    pub fn new() -> Result<Self, CallbackError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(CALLBACK_TIMEOUT_SECONDS))
            .build()
            .map_err(CallbackError::Transport)?;

        Ok(Self { http })
    }

    pub async fn send_json(&self, request: &CallbackRequest) -> Result<(), CallbackError> {
        let callback_url = normalize_callback_url(&request.callback_url)?;
        let event_type = normalize_header_value("event_type", &request.event_type)?;
        let reference = normalize_header_value("reference", &request.reference)?;

        let response = self
            .http
            .post(callback_url)
            .headers(callback_headers(&event_type, &reference)?)
            .json(&request.payload)
            .send()
            .await
            .map_err(CallbackError::Transport)?;

        if !response.status().is_success() {
            return Err(CallbackError::UnexpectedStatus(response.status().as_u16()));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CallbackRequest {
    pub callback_url: String,
    pub event_type: String,
    pub reference: String,
    pub payload: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum CallbackError {
    #[error("Callback URL is invalid")]
    InvalidUrl,

    #[error("Callback header `{0}` is invalid")]
    InvalidHeader(&'static str),

    #[error("Callback transport error")]
    Transport(reqwest::Error),

    #[error("Callback returned unexpected status {0}")]
    UnexpectedStatus(u16),
}

fn normalize_callback_url(callback_url: &str) -> Result<Url, CallbackError> {
    let callback_url = callback_url.trim();
    if callback_url.is_empty() {
        return Err(CallbackError::InvalidUrl);
    }

    let callback_url = Url::parse(callback_url).map_err(|_| CallbackError::InvalidUrl)?;
    match callback_url.scheme() {
        "http" | "https" => Ok(callback_url),
        _ => Err(CallbackError::InvalidUrl),
    }
}

fn normalize_header_value(field: &'static str, value: &str) -> Result<HeaderValue, CallbackError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CallbackError::InvalidHeader(field));
    }

    HeaderValue::from_str(value).map_err(|_| CallbackError::InvalidHeader(field))
}

fn callback_headers(
    event_type: &HeaderValue,
    reference: &HeaderValue,
) -> Result<HeaderMap, CallbackError> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("X-Bridge-Event", event_type.clone());
    headers.insert("X-Bridge-Reference", reference.clone());

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::{normalize_callback_url, CallbackError};

    #[test]
    fn rejects_empty_callback_url() {
        let error = normalize_callback_url("   ").unwrap_err();

        assert!(matches!(error, CallbackError::InvalidUrl));
    }

    #[test]
    fn rejects_non_http_callback_url() {
        let error = normalize_callback_url("ftp://example.test/callback").unwrap_err();

        assert!(matches!(error, CallbackError::InvalidUrl));
    }

    #[test]
    fn accepts_https_callback_url() {
        let url = normalize_callback_url("https://example.test/callback").unwrap();

        assert_eq!(url.as_str(), "https://example.test/callback");
    }
}
