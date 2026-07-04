use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxPath, Query, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use blink_sdk::{SpawnSpec, handle_control_message};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

use crate::api::error::{ApiError, valid_session_name};
use crate::exec_registry::{BufferedFrame, ExecSession, encode_frame};
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

    let session = ExecSession::start(name.clone(), execution, spec.tty, state.execs.clone());
    let exec_id = state.execs.insert(session).await;

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
    Query(query): Query<AttachQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let broker = state
        .execs
        .get(&exec_id)
        .await
        .ok_or_else(|| ApiError::not_found("execution not found"))?;

    if broker.session_name() != name {
        return Err(ApiError::not_found("execution not found in session"));
    }

    let query = query.finalize();
    let after = query.after.unwrap_or(0);
    let seq_framing = query.seq_framing;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(err) = run_ws_attach(socket, broker, after, seq_framing).await {
            tracing::warn!(%exec_id, error = %err, "WS attach ended with error");
        }
    }))
}

async fn run_ws_attach(
    socket: WebSocket,
    broker: Arc<ExecSession>,
    after: u64,
    seq_framing: bool,
) -> anyhow::Result<()> {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let mut output_rx = broker.subscribe();
    let mut exit_rx = broker.exit_rx();

    let snapshot = broker.snapshot_after(after);
    let mut last_sent_seq = after;
    for frame in snapshot.frames {
        send_frame(&mut ws_sink, &frame, seq_framing).await?;
        last_sent_seq = frame.seq;
    }

    if let Some(code) = snapshot.exit_code.or(broker.exit_code()) {
        let _ = flush_buffered_frames(&mut ws_sink, broker.as_ref(), last_sent_seq, seq_framing)
            .await?;
        send_exit(&mut ws_sink, code).await?;
        let _ = ws_sink.close().await;
        return Ok(());
    }

    loop {
        tokio::select! {
            frame = output_rx.recv() => {
                match frame {
                    Ok(frame) => {
                        if frame.seq <= last_sent_seq {
                            continue;
                        }
                        send_frame(&mut ws_sink, &frame, seq_framing).await?;
                        last_sent_seq = frame.seq;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(%skipped, "execution attach broadcast lagged");
                        last_sent_seq = flush_buffered_frames(
                            &mut ws_sink,
                            broker.as_ref(),
                            last_sent_seq,
                            seq_framing,
                        )
                        .await?;
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        let _ = flush_buffered_frames(
                            &mut ws_sink,
                            broker.as_ref(),
                            last_sent_seq,
                            seq_framing,
                        )
                        .await?;
                        let exit_code = broker.exit_code().or(*exit_rx.borrow());
                        if let Some(code) = exit_code {
                            send_exit(&mut ws_sink, code).await?;
                        }
                        let _ = ws_sink.close().await;
                        break;
                    }
                }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        broker.send_stdin(bytes.to_vec());
                    }
                    Some(Ok(Message::Text(text))) => {
                        if is_stdin_eof(&text) {
                            broker.close_stdin();
                        } else if let Err(err) = handle_control_message(broker.execution_for_control(), &text).await {
                            tracing::debug!(error = %err, "control message ignored");
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(error = %err, "WS read error");
                        break;
                    }
                }
            }
            changed = exit_rx.changed() => {
                match changed {
                    Ok(()) => {
                        let _ = flush_buffered_frames(
                            &mut ws_sink,
                            broker.as_ref(),
                            last_sent_seq,
                            seq_framing,
                        )
                        .await?;
                        let exit_code = *exit_rx.borrow();
                        if let Some(code) = exit_code {
                            send_exit(&mut ws_sink, code).await?;
                        } else if let Some(code) = broker.exit_code() {
                            send_exit(&mut ws_sink, code).await?;
                        }
                        let _ = ws_sink.close().await;
                        break;
                    }
                    Err(_) => {
                        if let Some(code) = broker.exit_code() {
                            send_exit(&mut ws_sink, code).await?;
                            let _ = ws_sink.close().await;
                        }
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Deserialize, Default)]
struct AttachQuery {
    after: Option<u64>,
    seq: Option<String>,
    #[serde(skip_deserializing, default)]
    seq_framing: bool,
}

impl AttachQuery {
    fn finalize(mut self) -> Self {
        self.seq_framing = self.after.is_some() || parse_boolish(self.seq.as_deref());
        self
    }
}

fn parse_boolish(value: Option<&str>) -> bool {
    matches!(
        value.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn is_stdin_eof(text: &str) -> bool {
    text.contains(r#""type":"stdin_eof""#)
}

async fn flush_buffered_frames(
    ws_sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    broker: &ExecSession,
    last_sent_seq: u64,
    seq_framing: bool,
) -> anyhow::Result<u64> {
    let tail = broker.snapshot_after(last_sent_seq);
    let mut newest_seq = last_sent_seq;
    for frame in tail.frames {
        if frame.seq <= last_sent_seq {
            continue;
        }
        send_frame(ws_sink, &frame, seq_framing).await?;
        newest_seq = frame.seq;
    }
    Ok(newest_seq)
}

async fn send_frame(
    ws_sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    frame: &BufferedFrame,
    seq_framing: bool,
) -> anyhow::Result<()> {
    let bytes = encode_frame(frame, seq_framing);
    ws_sink
        .send(Message::Binary(bytes.into()))
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))
}

async fn send_exit(
    ws_sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    code: i32,
) -> anyhow::Result<()> {
    ws_sink
        .send(Message::Text(
            serde_json::json!({"type":"exit","exit_code": code})
                .to_string()
                .into(),
        ))
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))
}

fn internal(err: impl ToString) -> ApiError {
    ApiError::internal(err.to_string())
}
