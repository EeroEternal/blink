//! PTY / interactive exec: spawn commands with optional TTY and bridge I/O.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use boxlite::{BoxCommand, ExecResult, Execution, LiteBox};
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};

/// Request to spawn a command inside a sandbox (maps to XEnsemble `ExecAdapter.spawn`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SpawnSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub tty: bool,
    #[serde(default)]
    pub rows: Option<u32>,
    #[serde(default)]
    pub cols: Option<u32>,
    #[serde(default)]
    pub working_dir: Option<String>,
}

impl SpawnSpec {
    pub fn into_box_command(self) -> BoxCommand {
        let mut cmd = BoxCommand::new(self.command).tty(self.tty);
        for arg in self.args {
            cmd = cmd.arg(arg);
        }
        for (k, v) in self.env {
            cmd = cmd.env(k, v);
        }
        if let Some(dir) = self.working_dir {
            cmd = cmd.working_dir(dir);
        }
        cmd
    }
}

/// Spawn a command and return a live `Execution` handle (does not wait for completion).
pub async fn spawn_exec(litebox: &LiteBox, spec: SpawnSpec) -> Result<Execution> {
    let tty = spec.tty;
    let rows = spec.rows;
    let cols = spec.cols;
    let mut execution = litebox
        .exec(spec.into_box_command())
        .await
        .context("failed to spawn command in sandbox")?;

    if tty {
        if let (Some(rows), Some(cols)) = (rows, cols) {
            execution
                .resize_tty(rows, cols)
                .await
                .context("initial PTY resize failed")?;
        }
    }

    Ok(execution)
}

/// Channels for bidirectional attach (WebSocket / V-Hub / local terminal).
pub struct ExecPump {
    /// Send raw stdin bytes into the sandbox process.
    pub stdin_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Binary frames: `[0x01|0x02, payload...]` (stdout / stderr).
    pub output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Fires once when the process exits.
    pub done: oneshot::Receiver<Result<ExecResult>>,
}

/// Start pumping I/O for an execution. Output frames use BoxLite attach channel prefixes.
pub fn start_exec_pump(mut execution: Execution, tty: bool) -> ExecPump {
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let (output_tx, output_rx) = mpsc::unbounded_channel();
    let (done_tx, done_rx) = oneshot::channel();

    tokio::spawn(async move {
        let result = run_pump(&mut execution, stdin_rx, &output_tx, tty).await;
        let _ = done_tx.send(result);
    });

    ExecPump {
        stdin_tx,
        output_rx,
        done: done_rx,
    }
}

async fn run_pump(
    execution: &mut Execution,
    mut stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    output_tx: &mpsc::UnboundedSender<Vec<u8>>,
    tty: bool,
) -> Result<ExecResult> {
    let stdin = execution.stdin();
    let stdout_stream = execution.stdout();
    let stderr_stream = if tty { None } else { execution.stderr() };

    let stdin_handle = if let Some(mut stdin_writer) = stdin {
        Some(tokio::spawn(async move {
            while let Some(bytes) = stdin_rx.recv().await {
                if stdin_writer.write(&bytes).await.is_err() {
                    break;
                }
            }
            stdin_writer.close();
        }))
    } else {
        None
    };

    if let Some(mut stream) = stdout_stream {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            while let Some(chunk) = stream.next().await {
                let mut frame = vec![0x01];
                frame.extend_from_slice(chunk.as_bytes());
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
    }

    if let Some(mut stream) = stderr_stream {
        let tx = output_tx.clone();
        tokio::spawn(async move {
            while let Some(chunk) = stream.next().await {
                let mut frame = vec![0x02];
                frame.extend_from_slice(chunk.as_bytes());
                if tx.send(frame).is_err() {
                    break;
                }
            }
        });
    }

    let result = execution.wait().await.context("wait for execution")?;

    if let Some(h) = stdin_handle {
        h.abort();
    }

    if let Some(message) = &result.error_message {
        bail!("exec did not complete normally: {message}");
    }

    Ok(result)
}

/// Apply a WebSocket-style resize control frame.
pub async fn handle_control_message(execution: &Execution, text: &str) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_str(text).context("invalid control JSON")?;
    match value.get("type").and_then(|v| v.as_str()) {
        Some("resize") => {
            let rows = value
                .get("rows")
                .and_then(|v| v.as_u64())
                .context("resize.rows required")? as u32;
            let cols = value
                .get("cols")
                .and_then(|v| v.as_u64())
                .context("resize.cols required")? as u32;
            execution
                .resize_tty(rows, cols)
                .await
                .context("resize_tty failed")?;
        }
        Some("signal") => {
            let signal = value
                .get("signal")
                .and_then(|v| v.as_i64())
                .context("signal required")? as i32;
            execution.signal(signal).await.context("signal failed")?;
        }
        Some("stdin_eof") => {}
        _ => {}
    }
    Ok(())
}
