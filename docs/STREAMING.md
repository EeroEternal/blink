# Output and Dual-Track I/O

Blink distinguishes two primary paths: **Pipe (structured results)** and **PTY (interactive terminal)**; V-Hub vsock is an optional third path (see [VHUB.md](VHUB.md)).

**PTY practical guide:** [PTY.md](PTY.md)

---

## 1. Pipe Mode (Default — Agent Short Tasks)

Parsing terminal ANSI escapes is unreliable for AI Agents. The default path uses **non-TTY pipe**; after the process ends, structured JSON is returned.

### Mechanism

- Guest process runs **without a PTY** (`BoxCommand` default `tty: false`)
- The host copies the Agent binary or script to Guest `/tmp/blink_agent`, then executes via `sh -c "chmod +x … && exec …"` (see `src/core/src/exec.rs`)
- The Agent may emit a `{"event":"execution_result",…}` JSON line at the end of stdout; otherwise Blink returns raw stdout / stderr / exit_code
- **Does not depend on vsock**; BoxLite portal exec is sufficient

### Entry Points

| Layer | API |
|-------|-----|
| CLI | `blink-cli run`, `blink-cli session run` |
| Server | `POST /api/runs`, `POST /api/sessions/{name}/runs` |
| Core | `exec_agent_script()`, `run_in_session()` |

---

## 2. PTY Mode (Interactive)

Use **PTY** for debugging, TUIs (`vim` / `top`), and long-lived shells; bridged via BoxLite `BoxCommand::tty(true)` and attach.

### Mechanism

- When `SpawnSpec.tty = true`, the host spawns with TTY
- `Execution` provides `stdin()` / `stdout()` / optional `stderr()` / `resize_tty(rows, cols)`
- `start_exec_pump()` bridges I/O to attach consumers (WebSocket or local terminal)
- Output frame prefix: `0x01` stdout, `0x02` stderr (non-TTY pipe mode only)

### Entry Points

| Layer | API |
|-------|-----|
| CLI | `blink-cli session spawn --name X --tty -- sh -i` |
| Server | `POST /api/sessions/{name}/spawn` → WebSocket `GET …/executions/{id}/attach` |
| Core | `spawn_exec()`, `start_exec_pump()`, `handle_control_message()` |

WebSocket control frames and wire format are detailed in [PTY.md](PTY.md).

---

## 3. V-Hub (Optional — vsock Relay)

The Guest connects to Host `:10000` via vsock; the BLIN protocol multiplexes RPC and `Stdout` / `Stderr` / `StreamData`. See [VHUB.md](VHUB.md).

**Current status:** The PTY **production path** is blink-server WebSocket attach. V-Hub has implemented Handshake, RPC echo, and `Stdout`/`Stderr` relay; full PTY-over-vsock can be added on demand.

---

## Selection Guide

```
Agent short task, want JSON result        →  Pipe (run / session run)
Human terminal, TUI, interactive shell    →  PTY (spawn + attach)
Control-plane (XEnsemble) terminal UI     →  spawn REST + WSS attach
Guest vsock RPC / stream relay (optional) →  V-Hub (VHUB.md)
```

---

## Related Docs

- [PTY.md](PTY.md) — CLI, REST, WebSocket examples
- [VHUB.md](VHUB.md) — BLIN header and message types
- [XENSEMBLE.md](XENSEMBLE.md) — Control-plane mapping
