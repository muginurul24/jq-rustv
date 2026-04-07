use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub bind_address: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub nexusggr_api_url: String,
    pub nexusggr_agent_code: String,
    pub nexusggr_agent_token: String,
    pub qris_api_url: String,
    pub qris_merchant_uuid: String,
    pub qris_client: String,
    pub qris_client_key: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            database_url: require_env("DATABASE_URL")?,
            redis_url: require_env("REDIS_URL")?,
            bind_address: env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            jwt_secret: require_env("JWT_SECRET")?,
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "8".to_string())
                .parse()
                .map_err(|_| "JWT_EXPIRY_HOURS must be a number".to_string())?,
            nexusggr_api_url: env::var("NEXUSGGR_API_URL")
                .unwrap_or_else(|_| "https://api.nexusggr.com".to_string()),
            nexusggr_agent_code: env::var("NEXUSGGR_AGENT_CODE").unwrap_or_default(),
            nexusggr_agent_token: env::var("NEXUSGGR_AGENT_TOKEN").unwrap_or_default(),
            qris_api_url: env::var("QRIS_API_URL")
                .unwrap_or_else(|_| "https://rest.otomatis.vip/api".to_string()),
            qris_merchant_uuid: require_env("QRIS_MERCHANT_UUID")?,
            qris_client: require_env("QRIS_CLIENT")?,
            qris_client_key: require_env("QRIS_CLIENT_KEY")?,
        })
    }
}

fn require_env(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("missing required env var: {key}"))
}
