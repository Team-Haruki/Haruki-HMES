use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;
use futures_util::StreamExt;
use haruki_hmes::{handlers, state::AppState, state::Event, state::subscription_key, config::Config};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;

struct TestApp {
    addr: SocketAddr,
    state: Arc<AppState>,
    _server: JoinHandle<()>,
}

impl TestApp {
    async fn start(cfg: Config) -> Self {
        let state = Arc::new(AppState::new(cfg));
        let app = Router::new()
            .route("/healthz", get(handlers::healthz))
            .route("/sse", get(handlers::sse))
            .route("/internal/events", post(handlers::internal_event))
            .route(
                "/internal/subscriptions/{subscription_id}/close",
                post(handlers::close_subscription),
            )
            .with_state(state.clone());

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            addr,
            state,
            _server: handle,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

async fn start_cloud<F>(handler: F) -> (SocketAddr, JoinHandle<()>)
where
    F: Fn(axum::http::HeaderMap, axum::extract::Query<std::collections::HashMap<String, String>>) -> axum::response::Response
        + Clone
        + Send
        + Sync
        + 'static,
{
    let app = Router::new().route(
        "/internal/subscriptions/mysekai-birthday/validate",
        get(move |headers, query| {
            let h = handler.clone();
            async move { h(headers, query) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, handle)
}

async fn read_sse_until<F: Fn(&str) -> bool>(
    body: reqwest::Response,
    predicate: F,
    deadline: Duration,
) -> String {
    let mut acc = String::new();
    let mut stream = body.bytes_stream();
    let _ = timeout(deadline, async {
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            acc.push_str(&String::from_utf8_lossy(&chunk));
            if predicate(&acc) {
                return;
            }
        }
    })
    .await;
    acc
}

#[tokio::test]
async fn sse_returns_pending_events_from_cloud() {
    let (cloud_addr, _cloud) = start_cloud(|headers, query| {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            "Bearer cloud-token"
        );
        assert_eq!(query.get("subscription_id").map(String::as_str), Some("1"));
        assert_eq!(query.get("token").map(String::as_str), Some("sse-token"));
        assert_eq!(
            query.get("subscription_version").map(String::as_str),
            Some("v1")
        );
        let body = json!({
            "valid": true,
            "subscription_id": "1",
            "subscription_version": "v1",
            "pending_events": [{
                "event_id": "9",
                "subscription_id": "1",
                "subscription_version": "v1",
                "empty_result": true,
            }]
        });
        axum::response::Json(body).into_response()
    })
    .await;

    let mut cfg = Config::from_env();
    cfg.cloud_base_url = format!("http://{}", cloud_addr);
    cfg.cloud_token = "cloud-token".to_string();
    cfg.user_agent = "test".to_string();
    cfg.sse_heartbeat_interval = Duration::from_secs(3600);
    cfg.cloud_http_timeout = Duration::from_secs(1);
    cfg.internal_token = String::new();

    let app = TestApp::start(cfg).await;
    app.state.publish(Event {
        event_id: "stale-local".to_string(),
        subscription_id: "1".to_string(),
        subscription_version: "v1".to_string(),
        ..Default::default()
    });

    let resp = reqwest::Client::new()
        .get(app.url("/sse?subscription_id=1&subscription_version=v1&token=sse-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = read_sse_until(
        resp,
        |s| s.contains("\"event_id\":\"9\""),
        Duration::from_secs(2),
    )
    .await;
    assert!(body.contains("event: birthday_monitor_update"), "body: {body}");
    assert!(body.contains("\"event_id\":\"9\""), "body: {body}");
    assert!(body.contains("\"empty_result\":true"), "body: {body}");
    assert!(
        app.state.pop_latest(&subscription_key("1", "v1")).is_none(),
        "expected local latest to be cleared"
    );
}

#[tokio::test]
async fn sse_returns_latest_local_event() {
    let (cloud_addr, _cloud) = start_cloud(|_headers, query| {
        assert_eq!(query.get("subscription_id").map(String::as_str), Some("1"));
        assert_eq!(
            query.get("subscription_version").map(String::as_str),
            Some("v1")
        );
        assert_eq!(query.get("token").map(String::as_str), Some("sse-token"));
        let body = json!({
            "valid": true,
            "subscription_id": "1",
            "subscription_version": "v1",
        });
        axum::response::Json(body).into_response()
    })
    .await;

    let mut cfg = Config::from_env();
    cfg.cloud_base_url = format!("http://{}", cloud_addr);
    cfg.user_agent = "test".to_string();
    cfg.sse_heartbeat_interval = Duration::from_secs(3600);
    cfg.cloud_http_timeout = Duration::from_secs(1);
    cfg.internal_token = String::new();

    let app = TestApp::start(cfg).await;
    app.state.publish(Event {
        event_id: "first".to_string(),
        subscription_id: "1".to_string(),
        subscription_version: "v1".to_string(),
        ..Default::default()
    });
    app.state.publish(Event {
        event_id: "latest".to_string(),
        subscription_id: "1".to_string(),
        subscription_version: "v1".to_string(),
        payload_ref: "redis-key".to_string(),
        ..Default::default()
    });

    let resp = reqwest::Client::new()
        .get(app.url("/sse?subscription_id=1&subscription_version=v1&token=sse-token"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = read_sse_until(
        resp,
        |s| s.contains("\"event_id\":\"latest\""),
        Duration::from_secs(2),
    )
    .await;
    assert!(body.contains("event: birthday_monitor_update"), "body: {body}");
    assert!(body.contains("\"event_id\":\"latest\""), "body: {body}");
    assert!(body.contains("\"payload_ref\":\"redis-key\""), "body: {body}");
    assert!(
        !body.contains("\"event_id\":\"first\""),
        "should only emit latest, body: {body}"
    );
}

use axum::response::IntoResponse;

#[tokio::test]
async fn internal_event_requires_configured_token() {
    let mut cfg = Config::from_env();
    cfg.internal_token = "secret".to_string();
    cfg.cloud_base_url = String::new();
    let app = TestApp::start(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(app.url("/internal/events"))
        .body("")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(app.url("/internal/events"))
        .header("Authorization", "Bearer secret")
        .body(r#"{"event_id":"11","subscription_id":"2","subscription_version":"v2","payload_ref":"ref"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let event = app
        .state
        .pop_latest(&subscription_key("2", "v2"))
        .expect("latest event");
    assert_eq!(event.event_id, "11");
    assert_eq!(event.payload_ref, "ref");
}

#[tokio::test]
async fn close_subscription_requires_token_and_closes_clients() {
    let mut cfg = Config::from_env();
    cfg.internal_token = "secret".to_string();
    cfg.cloud_base_url = String::new();
    let app = TestApp::start(cfg).await;
    let client = reqwest::Client::new();
    let key = subscription_key("2", "v2");
    let handle = app.state.register_sse_client(&key);
    app.state.publish(Event {
        event_id: "pending".to_string(),
        subscription_id: "2".to_string(),
        subscription_version: "v2".to_string(),
        ..Default::default()
    });

    let resp = client
        .post(app.url("/internal/subscriptions/2/close?subscription_version=v2"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = client
        .post(app.url("/internal/subscriptions/2/close?subscription_version=v2"))
        .header("Authorization", "Bearer secret")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["closed_clients"], 1);

    // Receiver should observe close: either sender dropped, or sentinel None.
    let mut rx = handle.rx;
    let result = timeout(Duration::from_secs(1), rx.changed()).await.expect("rx.changed should resolve");
    if result.is_ok() {
        assert!(rx.borrow().is_none(), "should not deliver buffered pending event");
        let next = timeout(Duration::from_secs(1), rx.changed()).await.expect("rx.changed should resolve");
        assert!(next.is_err(), "channel should be closed after sentinel");
    }
    assert!(app.state.pop_latest(&key).is_none());
}
