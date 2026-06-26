use std::path::Path;

use anyhow::{Context, Result, bail};
use blink_shared::{BLINK_MAGIC, HEADER_SIZE, MessageType, PROTOCOL_VERSION};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug)]
pub struct VhubSession {
    pub request_id: u64,
    pub payload: Vec<u8>,
}

/// Accept one agent connection and return the first RPC request payload.
pub async fn serve_vhub(socket_path: &Path) -> Result<VhubSession> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)
            .with_context(|| format!("failed to remove stale socket {}", socket_path.display()))?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("failed to bind V-Hub socket {}", socket_path.display()))?;

    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept agent connection")?;

    handle_session(&mut stream).await
}

async fn handle_session(stream: &mut UnixStream) -> Result<VhubSession> {
    let (msg_type, request_id, _payload) = read_packet(stream).await?;

    if msg_type != MessageType::Handshake {
        bail!("expected handshake as first packet, got {msg_type:?}");
    }

    write_packet(
        stream,
        MessageType::Handshake,
        request_id,
        b"blink-vhub-ready",
    )
    .await?;

    loop {
        let (msg_type, request_id, payload) = read_packet(stream).await?;
        match msg_type {
            MessageType::RpcRequest => {
                write_packet(stream, MessageType::RpcResponse, request_id, b"{}").await?;
                return Ok(VhubSession {
                    request_id,
                    payload,
                });
            }
            MessageType::Stdout | MessageType::Stderr | MessageType::StreamData => {
                use std::io::Write;
                let sink: &mut dyn Write = match msg_type {
                    MessageType::Stderr => &mut std::io::stderr(),
                    _ => &mut std::io::stdout(),
                };
                let _ = sink.write_all(&payload);
            }
            MessageType::TtyResize => {}
            MessageType::Heartbeat => {}
            other => bail!("unexpected message type while waiting for RPC: {other:?}"),
        }
    }
}

async fn read_packet(stream: &mut UnixStream) -> Result<(MessageType, u64, Vec<u8>)> {
    let mut header = [0u8; HEADER_SIZE];
    stream
        .read_exact(&mut header)
        .await
        .context("failed to read packet header")?;

    let magic = u32::from_le_bytes(header[0..4].try_into()?);
    if magic != BLINK_MAGIC {
        bail!("invalid BLIN magic: {magic:#x}");
    }

    let version = header[4];
    if version != PROTOCOL_VERSION {
        bail!("unsupported protocol version: {version}");
    }

    let msg_type = MessageType::from_u8(header[5]).context("unknown message type")?;
    let payload_len = u32::from_le_bytes(header[8..12].try_into()?) as usize;
    let request_id = u64::from_le_bytes(header[12..20].try_into()?);

    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream
            .read_exact(&mut payload)
            .await
            .context("failed to read packet payload")?;
    }

    Ok((msg_type, request_id, payload))
}

async fn write_packet(
    stream: &mut UnixStream,
    msg_type: MessageType,
    request_id: u64,
    payload: &[u8],
) -> Result<()> {
    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&BLINK_MAGIC.to_le_bytes());
    header[4] = PROTOCOL_VERSION;
    header[5] = msg_type as u8;
    header[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[12..20].copy_from_slice(&request_id.to_le_bytes());

    stream.write_all(&header).await?;
    stream.write_all(payload).await?;
    Ok(())
}
