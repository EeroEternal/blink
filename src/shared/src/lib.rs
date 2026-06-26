//! Blink host/guest shared protocol definitions.

use serde::{Deserialize, Serialize};

/// Default OCI rootfs image for agent sandboxes.
pub const DEFAULT_ROOTFS_IMAGE: &str = "alpine:3.20";

pub const BLINK_MAGIC: u32 = 0x424C_494E;
pub const PROTOCOL_VERSION: u8 = 1;
pub const VHUB_PORT: u32 = 10000;
pub const VMADDR_CID_HOST: u32 = 2;

/// Guest path for agent memory and artifacts (persisted on session disk).
pub const AGENT_MEMORY_DIR: &str = "/var/blink/memory";

pub const HEADER_FORMAT: &str = "<I B B H I Q";
pub const HEADER_SIZE: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    Handshake = 0x01,
    Heartbeat = 0x02,
    Error = 0x03,
    RpcRequest = 0x10,
    RpcResponse = 0x11,
    RpcError = 0x12,
    /// Raw PTY byte stream (interactive terminal output/input).
    StreamData = 0x20,
    /// Resize PTY window: JSON `{"rows":N,"cols":N}`.
    TtyResize = 0x21,
    Stdout = 0x30,
    Stderr = 0x31,
}

impl MessageType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Handshake),
            0x02 => Some(Self::Heartbeat),
            0x03 => Some(Self::Error),
            0x10 => Some(Self::RpcRequest),
            0x11 => Some(Self::RpcResponse),
            0x12 => Some(Self::RpcError),
            0x20 => Some(Self::StreamData),
            0x21 => Some(Self::TtyResize),
            0x30 => Some(Self::Stdout),
            0x31 => Some(Self::Stderr),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionPayload {
    pub event: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
