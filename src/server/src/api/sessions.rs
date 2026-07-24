use std::path::{Component, Path as StdPath, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxPath, State};
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use blink_shared::DEFAULT_ROOTFS_IMAGE;
use serde::Deserialize;

use crate::api::error::{valid_session_name, ApiError};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/sessions", get(list_sessions).post(open_session))
        .route("/sessions/{name}", get(get_session).delete(remove_session))
        .route("/sessions/{name}/stop", post(stop_session))
        .route("/sessions/{name}/runs", post(run_in_session))
        .route(
            "/sessions/{name}/checkpoints",
            get(list_checkpoints).post(create_checkpoint),
        )
        .route(
            "/sessions/{name}/checkpoints/{snapshot}/restore",
            post(restore_checkpoint),
        )
        .route("/sessions/{name}/export", post(export_session))
        .route("/import", post(import_session))
        .route("/exports/{filename}", get(download_export))
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sessions = state
        .ctx
        .list_sessions()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "event": "session_list",
        "sessions": sessions,
    })))
}

#[derive(Deserialize)]
struct OpenSessionRequest {
    name: String,
    #[serde(default = "default_image")]
    image: String,
    #[serde(default)]
    warm: bool,
    #[serde(default)]
    volumes: Vec<blink_sdk::SessionVolume>,
    /// Outbound network for the sandbox. Defaults to enabled (full egress).
    #[serde(default)]
    network: Option<blink_sdk::NetworkConfig>,
    /// VM resource limits (cpus / memory_mib / disk_size_gb). Unset = BoxLite defaults.
    #[serde(default)]
    resources: blink_sdk::SandboxResources,
}

fn default_image() -> String {
    DEFAULT_ROOTFS_IMAGE.into()
}

async fn open_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OpenSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !valid_session_name(&body.name) {
        return Err(ApiError::bad_request("invalid session name"));
    }
    let options = blink_sdk::OpenSessionOptions {
        volumes: body.volumes,
        network: body.network,
        resources: body.resources.clone(),
    };
    let (box_id, created) = state
        .ctx
        .open_session(&body.name, &body.image, body.warm, options)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("invalid network")
                || msg.contains("incompatible")
                || msg.contains("cpus must")
                || msg.contains("memory_mib must")
                || msg.contains("disk_size_gb must")
            {
                ApiError::bad_request(msg)
            } else {
                ApiError::internal(msg)
            }
        })?;
    Ok(Json(serde_json::json!({
        "event": "session_opened",
        "name": body.name,
        "box_id": box_id,
        "created": created,
        "warm": body.warm,
        "resources": body.resources,
    })))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sessions = state.ctx.list_sessions().await.map_err(internal)?;
    let session = sessions
        .into_iter()
        .find(|s| s.name.as_deref() == Some(name.as_str()))
        .ok_or_else(|| ApiError::not_found("session not found"))?;
    Ok(Json(serde_json::json!({
        "event": "session_info",
        "session": session,
    })))
}

#[derive(Deserialize)]
struct RunInSessionRequest {
    /// Host path to the agent binary or script to copy into the sandbox.
    script: String,
}

async fn run_in_session(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
    Json(body): Json<RunInSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let script = body.script.trim();
    if script.is_empty() {
        return Err(ApiError::bad_request("script required"));
    }
    if !std::path::Path::new(script).exists() {
        return Err(ApiError::bad_request("script not found on host"));
    }
    let result = state
        .ctx
        .run_in_session(&name, std::path::Path::new(script))
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "event": "execution_result",
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code,
        "memory_keys": result.memory_keys,
    })))
}

async fn list_checkpoints(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let snaps = state
        .ctx
        .list_checkpoints(&name)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "event": "checkpoint_list",
        "snapshots": snaps.iter().map(|s| serde_json::json!({
            "name": s.name,
            "id": s.id,
            "created_at": s.created_at,
            "size_bytes": s.disk_info.size_bytes,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
struct CheckpointRequest {
    snapshot: String,
}

async fn create_checkpoint(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
    Json(body): Json<CheckpointRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let info = state
        .ctx
        .checkpoint_session(&name, &body.snapshot)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "event": "session_checkpoint",
        "snapshot": info.name,
        "snapshot_id": info.id,
        "created_at": info.created_at,
        "size_bytes": info.disk_info.size_bytes,
    })))
}

async fn restore_checkpoint(
    State(state): State<Arc<AppState>>,
    AxPath((name, snapshot)): AxPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .ctx
        .restore_session(&name, &snapshot)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "event": "session_restored",
        "name": name,
        "snapshot": snapshot,
    })))
}

async fn export_session(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = state
        .ctx
        .export_session(&name)
        .await
        .map_err(internal)?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ApiError::internal("export path invalid"))?;
    Ok(Json(serde_json::json!({
        "event": "session_exported",
        "filename": filename,
        "download_url": format!("/api/exports/{filename}"),
        "path": path.display().to_string(),
    })))
}

async fn import_session(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut archive_path: Option<PathBuf> = None;
    let mut session_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        match field.name() {
            Some("archive") => {
                let name = field.file_name().unwrap_or("import.boxlite").to_string();
                let tmp = tempfile::Builder::new()
                    .prefix("blink-import-")
                    .suffix(".boxlite")
                    .tempfile()
                    .map_err(internal)?;
                let path = tmp.path().to_path_buf();
                let data = field.bytes().await.map_err(internal)?;
                tokio::fs::write(&path, &data).await.map_err(internal)?;
                archive_path = Some(path);
                if session_name.is_none() {
                    session_name = Some(
                        name.strip_suffix(".boxlite")
                            .unwrap_or(&name)
                            .chars()
                            .take(32)
                            .collect(),
                    );
                }
            }
            Some("name") => {
                let text = field.text().await.map_err(internal)?;
                if !text.trim().is_empty() {
                    session_name = Some(text.trim().to_string());
                }
            }
            _ => {}
        }
    }

    let archive_path = archive_path.ok_or_else(|| ApiError::bad_request("archive file required"))?;
    let name = session_name.as_deref();
    if let Some(n) = name {
        if !valid_session_name(n) {
            return Err(ApiError::bad_request("invalid session name"));
        }
    }

    let box_id = state
        .ctx
        .import_session(&archive_path, name)
        .await
        .map_err(internal)?;

    Ok(Json(serde_json::json!({
        "event": "session_imported",
        "box_id": box_id,
        "name": name,
    })))
}

async fn download_export(
    State(state): State<Arc<AppState>>,
    AxPath(filename): AxPath<String>,
) -> Result<Response, ApiError> {
    if filename.contains('/') || filename.contains("..") {
        return Err(ApiError::bad_request("invalid filename"));
    }
    let path = safe_export_path(state.ctx.export_dir(), &filename)?;
    if !path.exists() {
        return Err(ApiError::not_found("export not found"));
    }
    let data = tokio::fs::read(&path).await.map_err(internal)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(data))
        .unwrap())
}

async fn stop_session(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.ctx.stop_session(&name).await.map_err(internal)?;
    Ok(Json(serde_json::json!({ "event": "session_stopped", "name": name })))
}

async fn remove_session(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .ctx
        .remove_session(&name, false)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({ "event": "session_removed", "name": name })))
}

fn safe_export_path(export_dir: &StdPath, filename: &str) -> Result<PathBuf, ApiError> {
    let path = export_dir.join(filename);
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(ApiError::bad_request("invalid path"));
        }
    }
    if !path.starts_with(export_dir) {
        return Err(ApiError::bad_request("invalid path"));
    }
    Ok(path)
}

fn internal(err: impl ToString) -> ApiError {
    ApiError::internal(err.to_string())
}
