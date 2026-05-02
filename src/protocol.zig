const std = @import("std");

pub const BLINK_MAGIC: u32 = 0x424C494E; // "BLIN"

pub const MessageType = enum(u8) {
    Handshake = 0x01,
    Heartbeat = 0x02,
    Error = 0x03,

    RpcRequest = 0x10,
    RpcResponse = 0x11,
    RpcError = 0x12,

    Stdout = 0x30,
    Stderr = 0x31,

    StreamData = 0x20,
    StreamEnd = 0x21,
    
    _,
};

/// 16-byte fixed header, packed to ensure strict memory alignment
pub const VsockPacketHeader = extern struct {
    magic: u32 align(1) = BLINK_MAGIC,
    version: u8 align(1) = 1,
    msg_type: u8 align(1),
    flags: u16 align(1) = 0,
    payload_len: u32 align(1),
    request_id: u64 align(1),
};

pub const ProtocolError = error{
    InvalidMagic,
    UnsupportedVersion,
    IncompleteHeader,
    PayloadTooLarge,
    BufferTooSmall,
};

pub const MAX_PAYLOAD_SIZE: u32 = 1024 * 1024 * 10; // 10MB limit per frame

pub const VsockProtocol = struct {
    /// Reads and validates the header from a byte slice.
    pub fn decodeHeader(buffer: []const u8) !VsockPacketHeader {
        if (buffer.len < @sizeOf(VsockPacketHeader)) {
            return error.IncompleteHeader;
        }

        const header: *align(1) const VsockPacketHeader = @ptrCast(buffer[0..@sizeOf(VsockPacketHeader)]);
        
        if (header.magic != BLINK_MAGIC) {
            return error.InvalidMagic;
        }
        if (header.version != 1) {
            return error.UnsupportedVersion;
        }
        if (header.payload_len > MAX_PAYLOAD_SIZE) {
            return error.PayloadTooLarge;
        }

        return header.*;
    }

    /// Writes a complete packet (header + payload) to an allocated buffer.
    pub fn encodePacketAlloc(allocator: std.mem.Allocator, msg_type: MessageType, request_id: u64, payload: []const u8) ![]u8 {
        const total_len = @sizeOf(VsockPacketHeader) + payload.len;
        var buffer = try allocator.alloc(u8, total_len);
        errdefer allocator.free(buffer);

        const header: *align(1) VsockPacketHeader = @ptrCast(buffer[0..@sizeOf(VsockPacketHeader)]);
        header.* = .{
            .msg_type = @intFromEnum(msg_type),
            .payload_len = @intCast(payload.len),
            .request_id = request_id,
        };

        if (payload.len > 0) {
            @memcpy(buffer[@sizeOf(VsockPacketHeader)..], payload);
        }

        return buffer;
    }
};

test "Protocol basic encode decode" {
    const allocator = std.testing.allocator;
    const payload = "{\"method\":\"test\"}";
    
    const packet = try VsockProtocol.encodePacketAlloc(allocator, .RpcRequest, 42, payload);
    defer allocator.free(packet);

    const decoded_header = try VsockProtocol.decodeHeader(packet);
    try std.testing.expectEqual(BLINK_MAGIC, decoded_header.magic);
    try std.testing.expectEqual(@intFromEnum(MessageType.RpcRequest), decoded_header.msg_type);
    try std.testing.expectEqual(42, decoded_header.request_id);
    try std.testing.expectEqual(payload.len, decoded_header.payload_len);
    
    const decoded_payload = packet[@sizeOf(VsockPacketHeader)..];
    try std.testing.expectEqualStrings(payload, decoded_payload);
}
