use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use blink_sdk::{ExecPump, SpawnSpec, handle_control_message, start_exec_pump};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::api::error::{valid_session_name, ApiError};
use crate::exec_registry::PendingExec;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/sessions/{name}/spawn", post(spawn_exec))
        .route(
            "/sessions/{name}/executions/{exec_id}/attach",
            get(attach_exec),
        )
}

#[derive(Deserialize)]
struct SpawnRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: std::collections::HashMap<String, String>,
    #[serde(default)]
    tty: bool,
    #[serde(default)]
    rows: Option<u32>,
    #[serde(default)]
    cols: Option<u32>,
    #[serde(default)]
    working_dir: Option<String>,
}

async fn spawn_exec(
    State(state): State<Arc<AppState>>,
    AxPath(name): AxPath<String>,
    Json(body): Json<SpawnRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.command.trim().is_empty() {
        return Err(ApiError::bad_request("command required"));
    }
    if !valid_session_name(&name) {
        return Err(ApiError::bad_request("invalid session name"));
    }

    let spec = SpawnSpec {
        command: body.command,
        args: body.args,
        env: body.env,
        tty: body.tty,
        rows: body.rows,
        cols: body.cols,
        working_dir: body.working_dir,
    };

    let execution = state
        .ctx
        .spawn_in_session(&name, spec.clone())
        .await
        .map_err(internal)?;

    let exec_id = state
        .execs
        .insert(PendingExec {
            session_name: name.clone(),
            execution,
            tty: spec.tty,
        })
        .await;

    Ok(Json(serde_json::json!({
        "event": "exec_spawned",
        "session": name,
        "execution_id": exec_id,
        "tty": spec.tty,
        "attach_url": format!("/api/sessions/{name}/executions/{exec_id}/attach"),
    })))
}

async fn attach_exec(
    State(state): State<Arc<AppState>>,
    AxPath((name, exec_id)): AxPath<(String, String)>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let pending = state
        .execs
        .take(&exec_id)
        .await
        .ok_or_else(|| ApiError::not_found("execution not found or already attached"))?;

    if pending.session_name != name {
        return Err(ApiError::not_found("execution not found in session"));
    }

    let tty = pending.tty;
    let execution = pending.execution;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(err) = run_ws_attach(socket, execution, tty).await {
            tracing::warn!(%exec_id, error = %err, "WS attach ended with error");
        }
    }))
}

async fn run_ws_attach(
    socket: WebSocket,
    execution: blink_sdk::Execution,
    tty: bool,
) -> anyhow::Result<()> {
    let execution_for_control = execution.clone();
    let pump = start_exec_pump(execution, tty);
    let ExecPump {
        stdin_tx,
        mut output_rx,
        done,
    } = pump;

    let (mut ws_sink, mut ws_stream) = socket.split();
    let mut stdin_tx = Some(stdin_tx);
    let mut done = done;
    let mut exit_code: Option<i32> = None;

    loop {
        tokio::select! {
            frame = output_rx.recv() => {
                match frame {
                    Some(bytes) => {
                        if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        if let Some(tx) = stdin_tx.as_ref() {
                            let _ = tx.send(bytes.to_vec());
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if text.contains(r#""type":"stdin_eof""#) {
                            stdin_tx = None;
                        } else if let Err(e) = handle_control_message(&execution_for_control, &text).await {
                            tracing::debug!(error = %e, "control message ignored");
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, "WS read error");
                        break;
                    }
                }
            }
            result = &mut done => {
                match result {
                    Ok(Ok(status)) => {
                        exit_code = Some(status.exit_code);
                    }
                    Ok(Err(e)) => {
                        let _ = ws_sink.send(Message::Text(
                            serde_json::json!({"type":"error","message": e.to_string()}).to_string().into()
                        )).await;
                    }
                    Err(_) => {}
                }
                break;
            }
        }
    }

    let code = exit_code.unwrap_or(-1);
    let _ = ws_sink
        .send(Message::Text(
            serde_json::json!({"type":"exit","exit_code": code}).to_string().into(),
        ))
        .await;
    let _ = ws_sink.close().await;

    Ok(())
}

fn internal(err: impl ToString) -> ApiError {
    ApiError::internal(err.to_string())
}
