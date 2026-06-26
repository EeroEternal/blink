# PTY 交互式终端

Blink 提供 **双轨执行**：

| 模式 | 适用场景 | 入口 |
|------|----------|------|
| **Pipe（非 TTY）** | AI Agent 短任务、结构化 JSON 结果 | `POST /api/runs`、`POST /api/sessions/{name}/runs`、`blink-cli session run` |
| **PTY（交互式）** | 人工调试、TUI（vim/top）、长期 shell | `POST /api/sessions/{name}/spawn` + WebSocket attach、`blink-cli session spawn --tty` |

PTY 底层由 **BoxLite** 提供（`BoxCommand::tty(true)`、`Execution::stdin()` / `stdout()`、`resize_tty`）。Blink 在 Session 内封装 spawn + attach 桥接，协议与 BoxLite REST attach **兼容**。

---

## CLI

```bash
# 打开 Session
blink-cli session open --name my-agent

# 交互式 shell（PTY）
blink-cli session spawn --name my-agent --tty -- sh -i

# 非 TTY 一次性命令（stdout/stderr 仍实时打到本地终端，但无 PTY 语义）
blink-cli session spawn --name my-agent -- ls -la

# 指定初始终端尺寸
blink-cli session spawn --name my-agent --tty --rows 24 --cols 80 -- sh -i
```

完成后输出 JSON：`{"event":"exec_finished","session":"...","execution_id":"...","exit_code":N}`。

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

响应：

```json
{
  "event": "exec_spawned",
  "session": "my-agent",
  "execution_id": "abc123",
  "tty": true,
  "attach_url": "/api/sessions/my-agent/executions/abc123/attach"
}
```

**注意：** `execution_id` 在首次 WebSocket attach 前保存在服务端 registry；attach 成功后 registry 条目被消费，**不可重复 attach**。

### 2. WebSocket Attach

```http
GET /api/sessions/{name}/executions/{execution_id}/attach
Upgrade: websocket
```

连接 URL 示例：`ws://127.0.0.1:8787/api/sessions/my-agent/executions/abc123/attach`

#### 客户端 → 服务端

| 帧类型 | 内容 |
|--------|------|
| **Binary** | 原始 stdin 字节 |
| **Text JSON** | 控制消息（见下表） |

控制消息：

```json
{"type":"resize","rows":30,"cols":120}
{"type":"signal","signal":2}
{"type":"stdin_eof"}
```

#### 服务端 → 客户端

| 帧类型 | 内容 |
|--------|------|
| **Binary** | `[channel, payload...]`：`0x01` = stdout（PTY 模式下合并终端输出），`0x02` = stderr（仅非 TTY pipe 模式） |
| **Text JSON** | `{"type":"exit","exit_code":0}` 进程结束（终端帧） |
| **Text JSON** | `{"type":"error","message":"..."}` 非致命错误信息 |

#### JavaScript 示例

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
  const text = new TextDecoder().decode(buf.subarray(1));
  process.stdout.write(text); // channel 0x01 stdout, 0x02 stderr
};

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'resize', rows: 30, cols: 120 }));
};
```

---

## 与短任务路径的区别

**Pipe 模式**（`exec_agent_script` / `session run`）：

- 无 `.tty(true)`
- 进程结束后一次性解析 stdout 中的 `execution_result` JSON
- 适合 Agent SDK、LLM tool-call

**PTY 模式**（`spawn` + attach）：

- `BoxCommand::tty(true)`
- stdin/stdout 实时双向流
- 支持 `resize_tty`、Unix signal
- 不解析 `execution_result`；退出码由 attach 结束时的 `exit` 帧传递

---

## XEnsemble 集成

控制面 `BoxLiteExecAdapter.spawn(cmd, args, env)` 应对：

1. `POST /api/sessions/{blinkSessionName}/spawn`（`tty: true`）
2. 将返回的 `attach_url` 桥接到 Desktop/Web 终端 WebSocket

详见 [XENSEMBLE.md](XENSEMBLE.md)。

---

## V-Hub（vsock，次要路径）

V-Hub 协议已预留 PTY 相关消息类型（见 [VHUB.md](VHUB.md)）：

- `StreamData (0x20)` — 原始 PTY 字节流
- `TtyResize (0x21)` — JSON `{"rows":N,"cols":N}`

当前 **主路径** 是 blink-server WebSocket attach；`blink-cli serve` 的 V-Hub 仍主要用于 RPC/Stdout 中继，完整 PTY over vsock 可按 XEnsemble `attachSession` 需求后续接入。

---

## 相关文档

- [STREAMING.md](STREAMING.md) — 双轨输出架构总览
- [XENSEMBLE.md](XENSEMBLE.md) — 控制面 API 映射
- [PRODUCT.md](PRODUCT.md) — 产品层级与能力边界
