# V-Hub (vsock / BLIN Protocol)

A Guest Agent connects to Host port `10000` (CID `2`, constants in `blink-shared`) via **virtio-vsock**, using a 20-byte **BLIN** header to multiplex RPC and streaming output.

Host-side entry: `blink-cli serve --socket <path>` (`blink-sdk::serve_vhub`), which requires the libkrun vsock bridge to forward the Guest connection to that Unix socket.

**The primary path for interactive terminals is WebSocket attach, not V-Hub** — see [PTY.md](PTY.md). **Pipe-mode Agent execution defaults to BoxLite portal exec and does not depend on vsock** — see [STREAMING.md](STREAMING.md).

---

## Header (20 bytes)

| Field | Size | Description |
|-------|------|-------------|
| Magic | 4 | `0x424C494E` (`BLIN`) |
| Version | 1 | `1` |
| MessageType | 1 | See table below |
| Flags | 2 | `0` |
| Payload length | 4 | LE u32 |
| Request ID | 8 | LE u64 |
| Payload | variable | Usually JSON for RPC |

Constants are defined in `blink-shared` (`BLINK_MAGIC`, `PROTOCOL_VERSION`, `VHUB_PORT`, `VMADDR_CID_HOST`).

---

## Message Types

| Type | Value | Direction | Purpose |
|------|-------|-----------|---------|
| `Handshake` | `0x01` | bidirectional | Session establishment |
| `Heartbeat` | `0x02` | bidirectional | Keep-alive |
| `Error` | `0x03` | bidirectional | Protocol error |
| `RpcRequest` | `0x10` | guest → host | Structured JSON RPC |
| `RpcResponse` | `0x11` | host → guest | RPC reply |
| `RpcError` | `0x12` | host → guest | RPC failure |
| `StreamData` | `0x20` | guest → host | Raw PTY byte stream (reserved) |
| `TtyResize` | `0x21` | host → guest | JSON `{"rows":N,"cols":N}` |
| `Stdout` | `0x30` | guest → host | Pipe-mode stdout chunk |
| `Stderr` | `0x31` | guest → host | Pipe-mode stderr chunk |

---

## Host Dispatcher (`serve_vhub`)

Implementation: `src/core/src/vhub.rs`

1. The first packet must be `Handshake` → Host replies with payload `blink-vhub-ready`
2. Loop reading packets until an `RpcRequest` arrives → reply with an empty JSON `{}` `RpcResponse`, and return the request payload to the CLI caller
3. While waiting for RPC: write `Stdout` / `Stderr` / `StreamData` to the Host terminal stdout/stderr; silently ignore `TtyResize` / `Heartbeat`

`blink-cli serve` prints the RPC payload as JSON to stdout when received.

---

## Relationship to REST / PTY

```
Agent short task (default)     BoxLite exec, no vsock     →  STREAMING.md
Interactive terminal (prod path)  spawn + WebSocket attach   →  PTY.md
Guest vsock RPC/stream relay   BLIN @ :10000              →  this doc
```

`StreamData` / full PTY-over-vsock on V-Hub has not yet been wired as a production path; control planes can bridge vsock on demand.

---

## Related Docs

- [STREAMING.md](STREAMING.md) — Pipe / PTY dual-track I/O selection
- [PTY.md](PTY.md) — WebSocket attach protocol and examples
- [XENSEMBLE.md](XENSEMBLE.md) — Control-plane API mapping
