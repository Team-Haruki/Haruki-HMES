use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::cloud;
use crate::state::{subscription_key, AppState, Event};

type SharedState = Arc<AppState>;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

#[derive(Debug, Deserialize, Default)]
pub struct SseQuery {
    #[serde(default)]
    pub subscription_id: String,
    #[serde(default)]
    pub subscription_version: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub token: String,
}

pub async fn sse(
    State(state): State<SharedState>,
    Query(query): Query<SseQuery>,
) -> axum::response::Response {
    let subscription_id = query.subscription_id.trim().to_string();
    let subscription_version = query
        .subscription_version
        .as_deref()
        .or(query.version.as_deref())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let token = query.token.trim().to_string();

    if subscription_id.is_empty() || subscription_version.is_empty() || token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "subscription_id, subscription_version and token are required"
            })),
        )
            .into_response();
    }

    let validation =
        match cloud::validate_with_cloud(&state, &subscription_id, &subscription_version, &token)
            .await
        {
            Ok(v) => v,
            Err(err) => {
                tracing::error!(
                    subscription = %subscription_id,
                    version = %subscription_version,
                    error = %err,
                    "cloud validation failed"
                );
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "cloud validation failed" })),
                )
                    .into_response();
            }
        };
    if !validation.valid {
        tracing::warn!(
            subscription = %subscription_id,
            version = %subscription_version,
            "sse validation rejected"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid subscription token" })),
        )
            .into_response();
    }

    let key = subscription_key(&subscription_id, &subscription_version);
    let pending = normalize_events(
        validation.pending_events,
        &subscription_id,
        &subscription_version,
    );
    if !pending.is_empty() {
        state.clear_latest(&key);
    }
    let handle = state.register_sse_client(&key);
    tracing::info!(
        subscription = %subscription_id,
        version = %subscription_version,
        pending_events = pending.len(),
        "sse connected"
    );

    let heartbeat = if state.cfg.sse_heartbeat_interval.is_zero() {
        Duration::from_secs(15)
    } else {
        state.cfg.sse_heartbeat_interval
    };

    let state_for_stream = state.clone();
    let log_subscription = subscription_id.clone();
    let log_version = subscription_version.clone();

    let event_stream = stream! {
        let _guard = ClientGuard {
            state: state_for_stream.clone(),
            handle,
            subscription_id: log_subscription,
            subscription_version: log_version,
        };

        for event in pending {
            yield Ok::<_, Infallible>(make_sse_event(&event));
        }

        let mut rx = _guard.handle.rx.clone();
        // Watch starts with `None`. If publish overwrote it during register,
        // emit the current value first then subscribe to future updates.
        let initial = rx.borrow_and_update().clone();
        if let Some(event) = initial {
            yield Ok::<_, Infallible>(make_sse_event(&event));
        }
        loop {
            if rx.changed().await.is_err() {
                return;
            }
            let event = rx.borrow_and_update().clone();
            match event {
                Some(event) => yield Ok::<_, Infallible>(make_sse_event(&event)),
                None => return,
            }
        }
    };

    Sse::new(event_stream)
        .keep_alive(KeepAlive::new().interval(heartbeat).text("heartbeat"))
        .into_response()
}

struct ClientGuard {
    state: SharedState,
    handle: crate::state::ClientHandle,
    subscription_id: String,
    subscription_version: String,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.state.unregister_sse_client(&self.handle);
        tracing::info!(
            subscription = %self.subscription_id,
            version = %self.subscription_version,
            "sse disconnected"
        );
    }
}

fn make_sse_event(event: &Event) -> SseEvent {
    SseEvent::default()
        .event("birthday_monitor_update")
        .json_data(event)
        .unwrap_or_else(|_| {
            SseEvent::default()
                .event("birthday_monitor_update")
                .data("")
        })
}

fn normalize_events(
    events: Vec<Event>,
    subscription_id: &str,
    subscription_version: &str,
) -> Vec<Event> {
    events
        .into_iter()
        .map(|mut event| {
            event.event_id = event.event_id.trim().to_string();
            if event.subscription_id.is_empty() {
                event.subscription_id = subscription_id.trim().to_string();
            }
            if event.subscription_version.is_empty() {
                event.subscription_version = subscription_version.trim().to_string();
            }
            event.payload_ref = event.payload_ref.trim().to_string();
            event
        })
        .collect()
}

pub async fn internal_event(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if !state.authorized(authorization(&headers)) {
        tracing::warn!("internal event unauthorized");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    let mut event: Event = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "internal event invalid json");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid json" })),
            )
                .into_response();
        }
    };
    event.event_id = event.event_id.trim().to_string();
    event.subscription_id = event.subscription_id.trim().to_string();
    event.subscription_version = event.subscription_version.trim().to_string();
    event.payload_ref = event.payload_ref.trim().to_string();
    if event.event_id.is_empty()
        || event.subscription_id.is_empty()
        || event.subscription_version.is_empty()
    {
        tracing::warn!(
            event = %event.event_id,
            subscription = %event.subscription_id,
            version = %event.subscription_version,
            "internal event rejected"
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "event_id, subscription_id and subscription_version are required"
            })),
        )
            .into_response();
    }
    tracing::info!(
        event = %event.event_id,
        subscription = %event.subscription_id,
        version = %event.subscription_version,
        empty_result = event.empty_result,
        "internal event received"
    );
    state.publish(event);
    (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
}

#[derive(Debug, Deserialize, Default)]
pub struct CloseQuery {
    #[serde(default)]
    pub subscription_version: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CloseBody {
    #[serde(default)]
    subscription_version: String,
}

pub async fn close_subscription(
    State(state): State<SharedState>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<CloseQuery>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if !state.authorized(authorization(&headers)) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    let subscription_id = params
        .get("subscription_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let mut subscription_version = query
        .subscription_version
        .as_deref()
        .or(query.version.as_deref())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if subscription_version.is_empty() && !body.trim().is_empty() {
        match serde_json::from_str::<CloseBody>(&body) {
            Ok(parsed) => subscription_version = parsed.subscription_version.trim().to_string(),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "invalid json" })),
                )
                    .into_response();
            }
        }
    }
    if subscription_id.is_empty() || subscription_version.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "subscription_id and subscription_version are required"
            })),
        )
            .into_response();
    }
    let key = subscription_key(&subscription_id, &subscription_version);
    let closed = state.close_subscription(&key);
    tracing::info!(
        subscription = %subscription_id,
        version = %subscription_version,
        closed_clients = closed,
        "subscription close requested"
    );
    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "closed_clients": closed })),
    )
        .into_response()
}

fn authorization(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
}
