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
                let host = if host.is_empty() {
                    "0.0.0.0".to_string()
                } else {
                    host
                };
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
        Ok(v) => v
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .unwrap_or(fallback),
        Err(_) => fallback,
    }
}

fn env_bool(key: &str) -> bool {
    matches!(
        env::var(key).as_deref().map(str::trim),
        Ok("true") | Ok("1") | Ok("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        values: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn capture(keys: &[&'static str]) -> Self {
            Self {
                values: keys.iter().map(|key| (*key, env::var(key).ok())).collect(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.values {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn reads_and_normalizes_environment_values() {
        const KEYS: &[&str] = &[
            "HMES_ADDR",
            "HMES_HOST",
            "HMES_PORT",
            "HMES_INTERNAL_TOKEN",
            "HMES_CLOUD_INTERNAL_BASE_URL",
            "HMES_CLOUD_INTERNAL_TOKEN",
            "HMES_CLOUD_TLS_SKIP_VERIFY",
            "HMES_USER_AGENT",
            "HMES_SSE_HEARTBEAT_SECONDS",
            "HMES_CLOUD_TIMEOUT_SECONDS",
        ];
        let _guard = EnvGuard::capture(KEYS);

        env::remove_var("HMES_ADDR");
        env::set_var("HMES_HOST", " 127.0.0.1 ");
        env::set_var("HMES_PORT", "9000");
        env::set_var("HMES_INTERNAL_TOKEN", " internal ");
        env::set_var("HMES_CLOUD_INTERNAL_BASE_URL", " https://cloud.example ");
        env::set_var("HMES_CLOUD_INTERNAL_TOKEN", " cloud ");
        env::set_var("HMES_CLOUD_TLS_SKIP_VERIFY", "yes");
        env::set_var("HMES_USER_AGENT", " test-agent ");
        env::set_var("HMES_SSE_HEARTBEAT_SECONDS", "30");
        env::set_var("HMES_CLOUD_TIMEOUT_SECONDS", "8");

        let config = Config::from_env();
        assert_eq!(config.addr, "127.0.0.1:9000");
        assert_eq!(config.internal_token, "internal");
        assert_eq!(config.cloud_base_url, "https://cloud.example");
        assert_eq!(config.cloud_token, "cloud");
        assert!(config.cloud_tls_skip_verify);
        assert_eq!(config.user_agent, "test-agent");
        assert_eq!(config.sse_heartbeat_interval, Duration::from_secs(30));
        assert_eq!(config.cloud_http_timeout, Duration::from_secs(8));

        env::set_var("HMES_ADDR", " 127.0.0.1:9100 ");
        assert_eq!(Config::from_env().addr, "127.0.0.1:9100");
    }

    #[test]
    fn falls_back_for_missing_empty_and_invalid_values() {
        const STRING_KEY: &str = "HMES_TEST_STRING";
        const NUMBER_KEY: &str = "HMES_TEST_NUMBER";
        const BOOL_KEY: &str = "HMES_TEST_BOOL";
        let _guard = EnvGuard::capture(&[STRING_KEY, NUMBER_KEY, BOOL_KEY]);

        env::remove_var(STRING_KEY);
        assert_eq!(env_string(STRING_KEY, "fallback"), "fallback");
        env::set_var(STRING_KEY, "   ");
        assert_eq!(env_string(STRING_KEY, "fallback"), "fallback");
        env::set_var(STRING_KEY, " value ");
        assert_eq!(env_string(STRING_KEY, "fallback"), "value");

        env::remove_var(NUMBER_KEY);
        assert_eq!(env_u64(NUMBER_KEY, 7), 7);
        for invalid in ["0", "invalid", " 0 "] {
            env::set_var(NUMBER_KEY, invalid);
            assert_eq!(env_u64(NUMBER_KEY, 7), 7);
        }
        env::set_var(NUMBER_KEY, " 12 ");
        assert_eq!(env_u64(NUMBER_KEY, 7), 12);

        for truthy in ["true", "1", "yes"] {
            env::set_var(BOOL_KEY, truthy);
            assert!(env_bool(BOOL_KEY));
        }
        env::set_var(BOOL_KEY, "TRUE");
        assert!(!env_bool(BOOL_KEY));
        env::remove_var(BOOL_KEY);
        assert!(!env_bool(BOOL_KEY));
    }
}
