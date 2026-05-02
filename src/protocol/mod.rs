use serde::{Serialize, Deserialize};

pub const BLINK_MAGIC: u32 = 0x424C494E;

#[repr(C, packed)]
pub struct VsockPacketHeader {
    pub magic: u32,
    pub version: u8,
    pub msg_type: u8,
    pub flags: u16,
    pub payload_len: u32,
    pub request_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageType {
    Handshake = 0x01,
    Heartbeat = 0x02,
    Error = 0x03,
    RpcRequest = 0x10,
    RpcResponse = 0x11,
    RpcError = 0x12,
    Stdout = 0x30,
    Stderr = 0x31,
}
