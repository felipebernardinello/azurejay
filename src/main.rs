mod audio;
mod auth;
mod coa;
mod config;
mod conversations;
mod database;
mod entities;
mod error;
mod state;
mod telemetry;
mod tts;
mod users;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    telemetry::init();

    let config = Config::from_env()?;
    let addr: SocketAddr = format!("{}:{}", config.api_host, config.api_port)
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid API_HOST/API_PORT: {e}"))?;

    let state = AppState::build(config).await?;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "AzureJay listening");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/auth", auth::router(&state))
        .nest("/users", users::router())
        .nest("/conversations", conversations::router())
        .nest("/audio", audio::router());

    api.layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(300),
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
