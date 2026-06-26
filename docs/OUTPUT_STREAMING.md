# Blink Output Streaming Architecture (Dual-Track)

Blink prioritizes **machine-readable (structured)** output for AI Agents over **human-readable (interactive)** output, while supporting the latter via an opt-in PTY mode.

**Practical guide:** [PTY.md](PTY.md)

---

## 1. Primary: Structured JSON (Pipe Mode)

For AI Agent workloads, parsing raw terminal output (ANSI escape codes, progress bars) is error-prone. The default path uses **non-TTY pipes** and structured JSON reporting.

### Mechanism

- Guest process runs **without** a PTY (`BoxCommand` default `tty: false`).
- Host copies the agent script and runs `python3 /tmp/blink_agent.py` (see `src/core/src/exec.rs`).
- The agent may emit a final `execution_result` JSON line on stdout.
- Blink collects stdout/stderr after the process exits and returns structured JSON to the caller.

### Entry points

| Layer | API |
|-------|-----|
| CLI | `blink-cli run`, `blink-cli session run` |
| Server | `POST /api/runs`, `POST /api/sessions/{name}/runs` |
| Core | `exec_agent_script()`, `run_in_session()` |

### Advantages

- **Deterministic:** dispatchers read `.exit_code` / `.stderr` without regex on terminal output.
- **Metadata-rich:** JSON can carry memory keys, timestamps, etc.
- **Bandwidth-efficient:** no VT100 control sequences in the primary result path.

---

## 2. Secondary: PTY Streaming (Interactive)

For debugging, TUI apps (`vim`, `top`), or long-lived shells, Blink provides **PTY mode** via BoxLite.

### Mechanism

- Host calls `BoxCommand::tty(true)` when spawning (`SpawnSpec.tty = true`).
- `Execution` exposes bidirectional streams: `stdin()`, `stdout()`, optional `stderr()`, and `resize_tty(rows, cols)`.
- Blink **`start_exec_pump()`** bridges I/O to attach consumers (WebSocket or local terminal).
- Output frames use BoxLite attach channel prefixes: `0x01` stdout, `0x02` stderr (non-TTY only).

### Entry points

| Layer | API |
|-------|-----|
| CLI | `blink-cli session spawn --name X --tty -- sh -i` |
| Server | `POST /api/sessions/{name}/spawn` → WebSocket `GET .../executions/{id}/attach` |
| Core | `spawn_exec()`, `start_exec_pump()`, `handle_control_message()` |

### WebSocket control frames (client → server)

```json
{"type":"resize","rows":30,"cols":120}
{"type":"signal","signal":2}
{"type":"stdin_eof"}
```

### Terminal frame (server → client)

```json
{"type":"exit","exit_code":0}
```

### Advantages

- **Immersive debugging:** colors, progress bars (`tqdm`), cursor control.
- **Interactive stdin:** full-duplex terminal sessions.
- **BoxLite-compatible wire format** for XEnsemble `BoxLiteExecAdapter.spawn`.

---

## 3. V-Hub (vsock) — Optional Third Path

The BLIN protocol on vsock port `10000` multiplexes message types (see [COMMUNICATION_ARCH.md](COMMUNICATION_ARCH.md)):

| Type | Value | Role |
|------|-------|------|
| `RpcRequest` / `RpcResponse` | `0x10` / `0x11` | Structured RPC |
| `StreamData` | `0x20` | Raw PTY bytes (reserved for vsock bridge) |
| `TtyResize` | `0x21` | PTY window resize |
| `Stdout` / `Stderr` | `0x30` / `0x31` | Pipe-mode stream relay |

**Current status:** PTY **production path** is blink-server WebSocket attach. V-Hub `StreamData` is defined in `blink-shared` and handled in `serve_vhub`; full PTY-over-vsock integration is planned for control-plane vsock bridges.

---

## Choosing a Track

```
Short Agent task, need JSON result     →  Pipe mode (runs / session run)
Human terminal, TUI, interactive shell →  PTY mode (spawn + attach)
Control plane (XEnsemble) terminal UI  →  spawn REST + WSS attach
Guest vsock bridge (future)            →  V-Hub StreamData
```
