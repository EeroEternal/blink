# PTY Interactive Terminal

Blink provides **dual-track execution**:

| Mode | Use Case | Entry |
|------|----------|-------|
| **Pipe (non-TTY)** | AI Agent short tasks, structured JSON results | `POST /api/runs`, `POST /api/sessions/{name}/runs`, `blink-cli session run` |
| **PTY (interactive)** | Human debugging, TUI (vim/top), long-lived shells | `POST /api/sessions/{name}/spawn` + WebSocket attach, `blink-cli session spawn --tty` |

PTY is provided by **BoxLite** underneath (`BoxCommand::tty(true)`, `Execution::stdin()` / `stdout()`, `resize_tty`). Blink wraps spawn + attach bridging inside a Session; the protocol is **compatible** with BoxLite REST attach.

---

## CLI

```bash
# Open a Session
blink-cli session open --name my-agent

# Interactive shell (PTY)
blink-cli session spawn --name my-agent --tty -- sh -i

# Non-TTY one-shot command (stdout/stderr still stream to local terminal in real time, but no PTY semantics)
blink-cli session spawn --name my-agent -- ls -la

# Specify initial terminal size
blink-cli session spawn --name my-agent --tty --rows 24 --cols 80 -- sh -i
```

On completion, outputs JSON: `{"event":"exec_finished","session":"...","execution_id":"...","exit_code":N}`.

---

## blink-server REST + WebSocket

### 1. Spawn

```http
POST /api/sessions/{name}/spawn
Content-Type: application/json

{
  "command": "sh",
  "args": ["-i"],
  "env": { "TERM": "xterm-256color" },
  "tty": true,
  "rows": 24,
  "cols": 80,
  "working_dir": "/tmp"
}
```

Response:

```json
{
  "event": "exec_spawned",
  "session": "my-agent",
  "execution_id": "abc123",
  "tty": true,
  "attach_url": "/api/sessions/my-agent/executions/abc123/attach"
}
```

`execution_id` is stable for the lifetime of the execution. Blink now keeps a server-side
output buffer for each execution so the same execution can be attached multiple times
over its lifetime.

### 2. WebSocket Attach

```http
GET /api/sessions/{name}/executions/{execution_id}/attach?after=42&seq=1
Upgrade: websocket
```

Example connection URL: `ws://127.0.0.1:8787/api/sessions/my-agent/executions/abc123/attach`

Query parameters:

| Param | Meaning |
|-------|---------|
| `after=<seq>` | Resume from a cursor. Only frames with `seq > after` are delivered. |
| `seq=1` | Enable seq framing on binary output. Implied when `after` is present. |

Blink stores recent output in-memory in a bounded ring buffer (default
`BLINK_ATTACH_BUFFER_BYTES=1 MiB`) and keeps the execution record alive for a linger
window after exit (default `BLINK_ATTACH_LINGER_SECS=300`).

#### Client → Server

| Frame Type | Content |
|------------|---------|
| **Binary** | Raw stdin bytes |
| **Text JSON** | Control messages (see table below) |

Control messages:

```json
{"type":"resize","rows":30,"cols":120}
{"type":"signal","signal":2}
{"type":"stdin_eof"}
```

#### Server → Client

| Frame Type | Content |
|------------|---------|
| **Binary** | Legacy: `[channel, payload...]`. `0x01` = stdout, `0x02` = stderr. |
| **Binary** | Seq framing (`seq=1`): `[channel, seq:u64-be, payload...]`. The 8-byte seq is inserted immediately after the channel byte. |
| **Text JSON** | `{"type":"exit","exit_code":0}` process ended (terminal frame) |
| **Text JSON** | `{"type":"error","message":"..."}` non-fatal error information |

#### JavaScript Example

```javascript
const base = 'http://127.0.0.1:8787';

const { execution_id, attach_url } = await fetch(
  `${base}/api/sessions/my-agent/spawn`,
  {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      command: 'sh',
      args: ['-i'],
      tty: true,
      rows: 24,
      cols: 80,
    }),
  },
).then((r) => r.json());

const ws = new WebSocket(`ws://127.0.0.1:8787${attach_url}`);

ws.onmessage = (ev) => {
  if (typeof ev.data === 'string') {
    const msg = JSON.parse(ev.data);
    if (msg.type === 'exit') console.log('exit', msg.exit_code);
    return;
  }
  const buf = new Uint8Array(ev.data);
  const channel = buf[0];
  const text = new TextDecoder().decode(buf.subarray(1)); // legacy mode
  process.stdout.write(text); // channel 0x01 stdout, 0x02 stderr
};

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'resize', rows: 30, cols: 120 }));
};
```

---

## Difference from Short-Task Path

**Pipe mode** (`exec_agent_script` / `session run`):

- No `.tty(true)`
- After process ends, parse `execution_result` JSON from stdout once
- Suitable for Agent SDKs, LLM tool-calls

**PTY mode** (`spawn` + attach):

- `BoxCommand::tty(true)`
- Real-time bidirectional stdin/stdout stream
- Supports `resize_tty`, Unix signals
- Does not parse `execution_result`; exit code is delivered via the `exit` frame at attach end
- Attach can be repeated while the execution is alive or within the linger window after
  exit; use `after=<seq>` to backfill missed output.

---

## XEnsemble Integration

Control-plane `BoxLiteExecAdapter.spawn(cmd, args, env)` should:

1. `POST /api/sessions/{blinkSessionName}/spawn` (`tty: true`)
2. Bridge the returned `attach_url` to the Desktop/Web terminal WebSocket

See [XENSEMBLE.md](XENSEMBLE.md) for details.

---

## V-Hub (vsock, secondary path)

The V-Hub protocol has reserved PTY-related message types (see [VHUB.md](VHUB.md)):

- `StreamData (0x20)` — raw PTY byte stream
- `TtyResize (0x21)` — JSON `{"rows":N,"cols":N}`

The **primary path** today is blink-server WebSocket attach; `blink-cli serve` V-Hub is still mainly used for RPC/Stdout relay. Full PTY over vsock can be connected later according to XEnsemble `attachSession` requirements.

---

## Related Docs

- [STREAMING.md](STREAMING.md) — Dual-track output architecture overview
- [XENSEMBLE.md](XENSEMBLE.md) — Control-plane API mapping
- [PRODUCT.md](PRODUCT.md) — Product levels and capability boundaries
