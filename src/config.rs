use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub addr: String,
    pub internal_token: String,
    pub cloud_base_url: String,
    pub cloud_token: String,
    pub cloud_tls_skip_verify: bool,
    pub user_agent: String,
    pub sse_heartbeat_interval: Duration,
    pub cloud_http_timeout: Duration,
}

impl Config {
    pub fn from_env() -> Self {
        let host = env_string("HMES_HOST", "");
        let port = env_u64("HMES_PORT", 7910);
        let addr = match env_string("HMES_ADDR", "").as_str() {
            "" => {
                let host = if host.is_empty() { "0.0.0.0".to_string() } else { host };
                format!("{host}:{port}")
            }
            other => other.to_string(),
        };

        Self {
            addr,
            internal_token: env_string("HMES_INTERNAL_TOKEN", ""),
            cloud_base_url: env_string("HMES_CLOUD_INTERNAL_BASE_URL", ""),
            cloud_token: env_string("HMES_CLOUD_INTERNAL_TOKEN", ""),
            cloud_tls_skip_verify: env_bool("HMES_CLOUD_TLS_SKIP_VERIFY"),
            user_agent: env_string("HMES_USER_AGENT", "Haruki-HMES"),
            sse_heartbeat_interval: Duration::from_secs(env_u64("HMES_SSE_HEARTBEAT_SECONDS", 15)),
            cloud_http_timeout: Duration::from_secs(env_u64("HMES_CLOUD_TIMEOUT_SECONDS", 5)),
        }
    }
}

fn env_string(key: &str, fallback: &str) -> String {
    match env::var(key) {
        Ok(v) => {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                fallback.to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => fallback.to_string(),
    }
}

fn env_u64(key: &str, fallback: u64) -> u64 {
    match env::var(key) {
        Ok(v) => v.trim().parse::<u64>().ok().filter(|n| *n > 0).unwrap_or(fallback),
        Err(_) => fallback,
    }
}

fn env_bool(key: &str) -> bool {
    matches!(env::var(key).as_deref().map(str::trim), Ok("true") | Ok("1") | Ok("yes"))
}
