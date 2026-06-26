use std::path::PathBuf;

use anyhow::{Context, Result};
use blink_sdk::{
    checkpoint_session, export_session, import_session, list_checkpoints, list_sessions,
    open_session, open_warm_session, remove_session, restore_session, run_agent_script,
    run_in_session, serve_vhub, spawn_in_session, stop_session, SpawnSpec, start_exec_pump,
};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "blink-cli", about = "Blink — AI agent sandbox runtime (BoxLite core)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a guest agent binary in a one-shot sandbox (ephemeral).
    Run {
        script: PathBuf,
        #[arg(long, default_value = "alpine:3.20")]
        image: String,
    },
    /// Start the V-Hub listener on a Unix socket (for guest vsock bridge wiring).
    Serve {
        #[arg(long)]
        socket: PathBuf,
    },
    /// Persistent agent session backed by a named BoxLite box + snapshots.
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Create or reuse a named session (persistent disk, survives restarts).
    Open {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "alpine:3.20")]
        image: String,
        /// Keep VM alive after host disconnects (warm session).
        #[arg(long, default_value_t = false)]
        warm: bool,
    },
    /// Run an agent script inside an existing session.
    Run {
        #[arg(long)]
        name: String,
        script: PathBuf,
    },
    /// Spawn an interactive command (PTY when --tty).
    Spawn {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        tty: bool,
        #[arg(long)]
        rows: Option<u32>,
        #[arg(long)]
        cols: Option<u32>,
        /// Command and arguments (e.g. `-- sh -i`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Snapshot session disk state (agent memory, files, installed packages).
    Checkpoint {
        #[arg(long)]
        name: String,
        #[arg(long)]
        snapshot: String,
    },
    /// Restore session from a snapshot (session VM must be stopped).
    Restore {
        #[arg(long)]
        name: String,
        #[arg(long)]
        snapshot: String,
    },
    /// List snapshots for a session.
    List {
        #[arg(long)]
        name: String,
    },
    /// List all sessions.
    Ls,
    /// Export session to .boxlite archive.
    Export {
        #[arg(long)]
        name: String,
    },
    /// Import session from .boxlite archive.
    Import {
        archive: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop the session VM (disk retained).
    Stop {
        #[arg(long)]
        name: String,
    },
    /// Delete a session and its disks.
    Remove {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Commands::Run { script, image } => run_ephemeral(&script, &image).await,
        Commands::Serve { socket } => serve_command(&socket).await,
        Commands::Session { command } => session_command(command).await,
    }
}

async fn run_ephemeral(script: &PathBuf, image: &str) -> Result<()> {
    let result = run_agent_script(script, image)
        .await
        .with_context(|| format!("blink run failed for {}", script.display()))?;
    print_execution_result(&result)
}

async fn session_command(command: SessionCommands) -> Result<()> {
    match command {
        SessionCommands::Open { name, image, warm } => {
            let (box_id, created) = if warm {
                open_warm_session(&name, &image).await?
            } else {
                open_session(&name, &image).await?
            };
            let payload = serde_json::json!({
                "event": "session_opened",
                "name": name,
                "box_id": box_id,
                "created": created,
                "warm": warm,
                "memory_dir": blink_shared::AGENT_MEMORY_DIR,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Run { name, script } => {
            let result = run_in_session(&name, script.as_path())
                .await
                .with_context(|| format!("session run failed for {}", script.display()))?;
            print_execution_result(&result)
        }
        SessionCommands::Spawn {
            name,
            tty,
            rows,
            cols,
            command,
        } => spawn_session_command(&name, tty, rows, cols, &command).await,
        SessionCommands::Checkpoint { name, snapshot } => {
            let info = checkpoint_session(&name, &snapshot).await?;
            let payload = serde_json::json!({
                "event": "session_checkpoint",
                "name": name,
                "snapshot": info.name,
                "snapshot_id": info.id,
                "created_at": info.created_at,
                "size_bytes": info.disk_info.size_bytes,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Restore { name, snapshot } => {
            restore_session(&name, &snapshot).await?;
            let payload = serde_json::json!({
                "event": "session_restored",
                "name": name,
                "snapshot": snapshot,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::List { name } => {
            let snapshots = list_checkpoints(&name).await?;
            let payload = serde_json::json!({
                "event": "session_checkpoints",
                "name": name,
                "snapshots": snapshots.iter().map(|s| serde_json::json!({
                    "name": s.name,
                    "id": s.id,
                    "created_at": s.created_at,
                    "size_bytes": s.disk_info.size_bytes,
                })).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Ls => {
            let sessions = list_sessions().await?;
            println!("{}", serde_json::to_string(&sessions)?);
            Ok(())
        }
        SessionCommands::Export { name } => {
            let path = export_session(&name).await?;
            let payload = serde_json::json!({
                "event": "session_exported",
                "name": name,
                "path": path.display().to_string(),
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Import { archive, name } => {
            let box_id = import_session(&archive, name.as_deref()).await?;
            let payload = serde_json::json!({
                "event": "session_imported",
                "box_id": box_id,
                "name": name,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Stop { name } => {
            stop_session(&name).await?;
            let payload = serde_json::json!({
                "event": "session_stopped",
                "name": name,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
        SessionCommands::Remove { name, force } => {
            remove_session(&name, force).await?;
            let payload = serde_json::json!({
                "event": "session_removed",
                "name": name,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(())
        }
    }
}

fn print_execution_result(result: &blink_sdk::AgentResult) -> Result<()> {
    let payload = serde_json::json!({
        "event": "execution_result",
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code,
    });
    println!("{}", serde_json::to_string(&payload)?);
    Ok(())
}

async fn serve_command(socket: &PathBuf) -> Result<()> {
    tracing::info!(socket = %socket.display(), port = blink_shared::VHUB_PORT, "V-Hub listening");
    let session = serve_vhub(socket).await?;
    let payload = String::from_utf8(session.payload).context("RPC payload is not valid UTF-8")?;
    println!("{payload}");
    Ok(())
}

async fn spawn_session_command(
    name: &str,
    tty: bool,
    rows: Option<u32>,
    cols: Option<u32>,
    command: &[String],
) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("command required after `--`");
    }
    let (cmd, args) = command.split_first().unwrap();
    let spec = SpawnSpec {
        command: cmd.clone(),
        args: args.to_vec(),
        env: Default::default(),
        tty,
        rows,
        cols,
        working_dir: None,
    };
    let execution = spawn_in_session(name, spec).await?;
    let exec_id = execution.id().to_string();
    tracing::info!(session = name, exec_id, tty, "spawned; attaching local terminal");

    let pump = start_exec_pump(execution, tty);
    let exit_code = attach_local_terminal(pump, tty).await?;

    let payload = serde_json::json!({
        "event": "exec_finished",
        "session": name,
        "execution_id": exec_id,
        "exit_code": exit_code,
    });
    println!("{}", serde_json::to_string(&payload)?);
    Ok(())
}

async fn attach_local_terminal(pump: blink_sdk::ExecPump, tty: bool) -> Result<i32> {
    use blink_sdk::ExecPump;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ExecPump {
        stdin_tx,
        mut output_rx,
        done,
    } = pump;

    let stdin_handle = {
        let tx = stdin_tx.clone();
        tokio::spawn(async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 8192];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        })
    };

    let stdout_handle = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(frame) = output_rx.recv().await {
            let payload = if frame.first() == Some(&0x02) && !tty {
                &frame[1..]
            } else if frame.first() == Some(&0x01) {
                &frame[1..]
            } else {
                &frame[..]
            };
            if stdout.write_all(payload).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });

    let mut done = done;
    let exit_code = tokio::select! {
        result = &mut done => {
            match result {
                Ok(Ok(status)) => status.exit_code,
                Ok(Err(e)) => return Err(e),
                Err(_) => -1,
            }
        }
        _ = stdout_handle => -1,
    };

    stdin_handle.abort();
    Ok(exit_code)
}
