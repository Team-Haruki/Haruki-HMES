use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;

use haruki_hmes::{config::Config, handlers, logging, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_ansi();
    logging::init();

    let cfg = Config::from_env();
    let addr = cfg.addr.clone();
    let state = Arc::new(AppState::new(cfg));

    let app = Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/sse", get(handlers::sse))
        .route("/internal/events", post(handlers::internal_event))
        .route(
            "/internal/subscriptions/{subscription_id}/close",
            post(handlers::close_subscription),
        )
        .with_state(state);

    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "HMES listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(windows)]
fn enable_ansi() {
    let _ = enable_ansi_support::enable_ansi_support();
}

#[cfg(not(windows))]
fn enable_ansi() {}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
