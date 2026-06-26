mod api;
mod exec_registry;
mod jobs;
mod state;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use clap::Parser;
use state::AppState;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};
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
    #[arg(long, env = "BLINK_WEB_ROOT")]
    web_root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let args = Args::parse();
    let web_root = resolve_web_root(args.web_root);
    let state = Arc::new(AppState::new()?);

    let app = Router::new()
        .nest("/api", api::router())
        .fallback_service(
            ServeDir::new(&web_root)
                .not_found_service(ServeFile::new(web_root.join("index.html"))),
        )
        .layer(RequestBodyLimitLayer::new(512 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .context("invalid bind address")?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, web_root = %web_root.display(), "Blink sandbox API listening");
    axum::serve(listener, app).await.context("server exited with error")?;
    Ok(())
}

fn resolve_web_root(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web")
    })
}
