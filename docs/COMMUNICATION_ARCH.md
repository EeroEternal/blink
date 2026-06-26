# Blink V-Hub & I/O Architecture

Blink separates **Agent structured results**, **real-time pipe streams**, and **interactive PTY** into distinct paths. They can share transport (vsock V-Hub or HTTP WebSocket) but serve different workloads.

**PTY usage guide:** [PTY.md](PTY.md)

---

## 1. Three I/O Tracks

| Track | Transport (today) | Message / API | Use case |
|-------|-------------------|---------------|----------|
| **Structured result** | REST, CLI stdout | `execution_result` JSON | AI Agent one-shot runs |
| **Pipe streaming** | V-Hub vsock | `Stdout (0x30)`, `Stderr (0x31)` | Live logs without PTY |
| **PTY interactive** | REST + WebSocket | spawn + attach (BoxLite wire) | Shell, TUI, XEnsemble terminal |

---

## 2. V-Hub (virtio-vsock)

Host: `blink-cli serve --socket <path>` (bridge to guest CID `2`, port `10000`).

### Packet header (20 bytes)

| Field | Size | Notes |
|-------|------|-------|
| Magic | 4 | `0x424C494E` (`BLIN`) |
| Version | 1 | `1` |
| MessageType | 1 | See table below |
| Flags | 2 | `0` |
| Payload length | 4 | LE u32 |
| Request ID | 8 | LE u64 |

### Message types (`blink-shared`)

| Type | Value | Direction | Purpose |
|------|-------|-----------|---------|
| `Handshake` | `0x01` | both | Session open |
| `Heartbeat` | `0x02` | both | Keepalive |
| `Error` | `0x03` | both | Protocol error |
| `RpcRequest` | `0x10` | guest → host | Structured JSON RPC |
| `RpcResponse` | `0x11` | host → guest | RPC reply |
| `RpcError` | `0x12` | host → guest | RPC failure |
| `StreamData` | `0x20` | guest → host | Raw PTY byte stream |
| `TtyResize` | `0x21` | host → guest | JSON `{"rows":N,"cols":N}` |
| `Stdout` | `0x30` | guest → host | Pipe-mode stdout chunk |
| `Stderr` | `0x31` | guest → host | Pipe-mode stderr chunk |

### Host dispatcher (`serve_vhub`)

- `Handshake` → reply `blink-vhub-ready`
- `RpcRequest` → echo `{}` response, return payload to caller
- `Stdout` / `Stderr` / `StreamData` → write to host terminal stdout
- `TtyResize` → acknowledged (resize applied on PTY bridge when wired)

---

## 3. PTY attach (WebSocket — primary interactive path)

Implemented in `blink-server` (`/api/sessions/{name}/spawn` + `/executions/{id}/attach`).

Wire contract mirrors **BoxLite REST attach**:

**Client → server**

- Binary frames: stdin bytes
- Text JSON: `resize`, `signal`, `stdin_eof`

**Server → client**

- Binary frames: `[0x01, …]` stdout, `[0x02, …]` stderr (non-TTY)
- Text JSON: `{"type":"exit","exit_code":N}`

Core implementation: `blink-core` `pty.rs` — `spawn_exec()`, `start_exec_pump()`, `handle_control_message()`.

---

## 4. Pipe-mode agent lifecycle (non-PTY)

1. Host copies agent script into guest `/tmp/blink_agent.py`.
2. Host runs `python3` via `BoxCommand` (no TTY).
3. Host drains stdout/stderr streams, waits for exit.
4. Host parses `execution_result` JSON from stdout if present.

No vsock required for this path; BoxLite portal exec is sufficient.

---

## 5. Lifecycle comparison

```
Pipe (Agent run):
  Host exec (no TTY) → collect streams → execution_result JSON

PTY (Interactive):
  Host spawn (tty=true) → WS attach → bidirectional bytes → exit frame

V-Hub (optional):
  Guest connect :10000 → Handshake → StreamData/Rpc multiplexed
```

---

## Related

- [OUTPUT_STREAMING.md](OUTPUT_STREAMING.md) — when to use each track
- [PTY.md](PTY.md) — CLI, REST, WebSocket examples
- [XENSEMBLE.md](XENSEMBLE.md) — control-plane mapping
