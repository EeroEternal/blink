mod api;
mod exec_registry;
mod jobs;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::routing::get;
use clap::Parser;
use state::AppState;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "blink-server", about = "Blink sandbox API (execution plane for control-plane consumers)")]
struct Args {
    #[arg(long, default_value = "8787")]
    port: u16,
    /// Listen address. Default localhost — Blink has no API auth; expose only on a trusted network.
    #[arg(long, env = "BLINK_BIND", default_value = "127.0.0.1")]
    bind: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let state = Arc::new(AppState::new()?);

    let app = Router::new()
        .route("/", get(root))
        .nest("/api", api::router())
        .layer(RequestBodyLimitLayer::new(512 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .context("invalid bind address")?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "Blink sandbox API listening");
    axum::serve(listener, app).await.context("server exited with error")?;
    Ok(())
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "service": "blink-server",
        "role": "sandbox_execution_plane",
        "health": "/api/health",
        "product": "/api/product",
    }))
}
