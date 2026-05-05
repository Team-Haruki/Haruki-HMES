use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::config::Config;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub subscription_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subscription_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub payload_ref: String,
    #[serde(default)]
    pub empty_result: bool,
}

impl Event {
    pub fn key(&self) -> String {
        subscription_key(&self.subscription_id, &self.subscription_version)
    }
}

pub fn subscription_key(subscription_id: &str, subscription_version: &str) -> String {
    let id = subscription_id.trim();
    let ver = subscription_version.trim();
    if ver.is_empty() {
        id.to_string()
    } else {
        format!("{id}:{ver}")
    }
}

pub type ClientId = u64;

#[derive(Default)]
struct Subscription {
    latest: Option<Event>,
    clients: HashMap<ClientId, watch::Sender<Option<Event>>>,
}

pub struct AppState {
    pub cfg: Config,
    pub http: reqwest::Client,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    subscriptions: HashMap<String, Subscription>,
    next_client_id: ClientId,
}

pub struct ClientHandle {
    pub key: String,
    pub id: ClientId,
    pub rx: watch::Receiver<Option<Event>>,
}

impl AppState {
    pub fn new(cfg: Config) -> Self {
        // Install ring as the default rustls crypto provider.
        // Silently ignore "already installed" errors (e.g. in tests).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(cfg.cloud_http_timeout)
            .user_agent(cfg.user_agent.clone())
            .danger_accept_invalid_certs(cfg.cloud_tls_skip_verify)
            .build()
            .expect("build reqwest client");
        Self {
            cfg,
            http,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn publish(&self, event: Event) {
        let key = event.key();
        let mut inner = self.inner.lock().unwrap();
        let sub = inner.subscriptions.entry(key.clone()).or_default();
        if sub.clients.is_empty() {
            tracing::info!(
                event = %event.event_id,
                subscription = %event.subscription_id,
                version = %event.subscription_version,
                empty_result = event.empty_result,
                "event queued as latest"
            );
            sub.latest = Some(event);
        } else {
            tracing::info!(
                event = %event.event_id,
                subscription = %event.subscription_id,
                version = %event.subscription_version,
                clients = sub.clients.len(),
                empty_result = event.empty_result,
                "event delivering to clients"
            );
            for tx in sub.clients.values() {
                let _ = tx.send(Some(event.clone()));
            }
        }
    }

    pub fn clear_latest(&self, key: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(sub) = inner.subscriptions.get_mut(key) {
            sub.latest = None;
            self.maybe_drop_empty(&mut inner, key);
        }
    }

    pub fn pop_latest(&self, key: &str) -> Option<Event> {
        let mut inner = self.inner.lock().unwrap();
        let event = inner
            .subscriptions
            .get_mut(key)
            .and_then(|sub| sub.latest.take());
        if event.is_some() {
            self.maybe_drop_empty(&mut inner, key);
        }
        event
    }

    pub fn register_sse_client(&self, key: &str) -> ClientHandle {
        let (tx, rx) = watch::channel::<Option<Event>>(None);
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_client_id;
        inner.next_client_id = inner.next_client_id.wrapping_add(1);
        let sub = inner.subscriptions.entry(key.to_string()).or_default();
        if let Some(event) = sub.latest.take() {
            let _ = tx.send(Some(event));
        }
        sub.clients.insert(id, tx);
        ClientHandle {
            key: key.to_string(),
            id,
            rx,
        }
    }

    pub fn unregister_sse_client(&self, handle: &ClientHandle) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(sub) = inner.subscriptions.get_mut(&handle.key) {
            sub.clients.remove(&handle.id);
        }
        self.maybe_drop_empty(&mut inner, &handle.key);
    }

    pub fn close_subscription(&self, key: &str) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let Some(mut sub) = inner.subscriptions.remove(key) else {
            return 0;
        };
        sub.latest = None;
        let count = sub.clients.len();
        // Send a sentinel `None` to wake any blocked receivers and signal
        // graceful close. The SSE loop treats `None` as exit without delivery.
        for (_, tx) in sub.clients.drain() {
            let _ = tx.send(None);
            drop(tx);
        }
        count
    }

    pub fn authorized(&self, header_value: Option<&str>) -> bool {
        let expected = self.cfg.internal_token.trim();
        if expected.is_empty() {
            return true;
        }
        let auth = header_value.map(str::trim).unwrap_or("");
        auth == expected || auth == bearer_auth(expected)
    }

    fn maybe_drop_empty(&self, inner: &mut std::sync::MutexGuard<'_, Inner>, key: &str) {
        if let Some(sub) = inner.subscriptions.get(key) {
            if sub.latest.is_none() && sub.clients.is_empty() {
                inner.subscriptions.remove(key);
            }
        }
    }
}

pub fn bearer_auth(token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        return String::new();
    }
    if token.to_ascii_lowercase().starts_with("bearer ") {
        token.to_string()
    } else {
        format!("Bearer {token}")
    }
}
