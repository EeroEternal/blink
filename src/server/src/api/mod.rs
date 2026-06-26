mod error;
mod exec;
mod runs;
mod sessions;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/product", get(product))
        .merge(runs::router())
        .merge(sessions::router())
        .merge(exec::router())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "blink-server",
        "role": "sandbox_execution_plane",
    }))
}

async fn product() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "ephemeral_enabled": true,
        "session_enabled": true,
        "snapshot_enabled": true,
        "export_enabled": true,
        "import_enabled": true,
        "warm_enabled": true,
        "pty_spawn_enabled": true,
        "consumer": "xensemble",
        "security": "network_isolation — no API auth; bind to localhost or private network",
    }))
}
