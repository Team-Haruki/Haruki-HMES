use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::state::{bearer_auth, AppState, Event};

#[derive(Debug, Default, Deserialize)]
pub struct CloudValidationResponse {
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub subscription_id: String,
    #[serde(default)]
    pub subscription_version: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub pending_events: Vec<Event>,
}

pub async fn validate_with_cloud(
    state: &AppState,
    subscription_id: &str,
    subscription_version: &str,
    token: &str,
) -> Result<CloudValidationResponse> {
    let base = state.cfg.cloud_base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("cloud base url is not configured"));
    }
    let url = format!("{base}/internal/subscriptions/mysekai-birthday/validate");
    let mut query = vec![("subscription_id", subscription_id.to_string())];
    if !subscription_version.is_empty() {
        query.push(("subscription_version", subscription_version.to_string()));
    }
    query.push(("token", token.to_string()));

    let mut request = state.http.get(&url).query(&query);
    let auth = bearer_auth(&state.cfg.cloud_token);
    if !auth.is_empty() {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }

    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("cloud returned status {}", status.as_u16()));
    }
    let body = response.json::<CloudValidationResponse>().await?;
    Ok(body)
}
