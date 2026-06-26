# 输出与 I/O 双轨

Blink 区分 **Pipe（结构化结果）** 与 **PTY（交互式终端）** 两条主路径；V-Hub vsock 为可选第三路径（见 [VHUB.md](VHUB.md)）。

**PTY 实操指南：** [PTY.md](PTY.md)

---

## 1. Pipe 模式（默认 — Agent 短任务）

解析终端 ANSI 转义对 AI Agent 不可靠。默认路径使用 **非 TTY pipe**，进程结束后返回结构化 JSON。

### 机制

- Guest 进程 **不带 PTY**（`BoxCommand` 默认 `tty: false`）
- Host 将 Agent 二进制或脚本复制到 Guest `/tmp/blink_agent`，经 `sh -c "chmod +x … && exec …"` 执行（见 `src/core/src/exec.rs`）
- Agent 可在 stdout 末行输出 `{"event":"execution_result",…}` JSON；否则 Blink 返回原始 stdout / stderr / exit_code
- **不依赖 vsock**；BoxLite portal exec 即可

### 入口

| 层 | API |
|----|-----|
| CLI | `blink-cli run`，`blink-cli session run` |
| Server | `POST /api/runs`，`POST /api/sessions/{name}/runs` |
| Core | `exec_agent_script()`，`run_in_session()` |

---

## 2. PTY 模式（交互式）

调试、TUI（`vim` / `top`）、长期 shell 时使用 **PTY**，经 BoxLite `BoxCommand::tty(true)` 与 attach 桥接。

### 机制

- `SpawnSpec.tty = true` 时 Host 以 TTY  spawn
- `Execution` 提供 `stdin()` / `stdout()` / 可选 `stderr()` / `resize_tty(rows, cols)`
- `start_exec_pump()` 将 I/O 桥接到 attach 消费者（WebSocket 或本地终端）
- 输出帧前缀：`0x01` stdout，`0x02` stderr（仅非 TTY pipe 模式）

### 入口

| 层 | API |
|----|-----|
| CLI | `blink-cli session spawn --name X --tty -- sh -i` |
| Server | `POST /api/sessions/{name}/spawn` → WebSocket `GET …/executions/{id}/attach` |
| Core | `spawn_exec()`，`start_exec_pump()`，`handle_control_message()` |

WebSocket 控制帧与 wire 格式详见 [PTY.md](PTY.md)。

---

## 3. V-Hub（可选 — vsock 中继）

Guest 经 vsock 连 Host `:10000`，BLIN 协议多路复用 RPC 与 `Stdout` / `Stderr` / `StreamData`。详见 [VHUB.md](VHUB.md)。

**现状：** PTY **生产路径** 是 blink-server WebSocket attach。V-Hub 已实现 Handshake、RPC echo、`Stdout`/`Stderr` 中继；完整 PTY-over-vsock 按需接入。

---

## 选型

```
Agent 短任务、要 JSON 结果        →  Pipe（run / session run）
人工终端、TUI、交互 shell          →  PTY（spawn + attach）
控制面（XEnsemble）终端 UI         →  spawn REST + WSS attach
Guest vsock RPC / 流中继（可选）   →  V-Hub（VHUB.md）
```

---

## 相关文档

- [PTY.md](PTY.md) — CLI、REST、WebSocket 示例
- [VHUB.md](VHUB.md) — BLIN 包头与消息类型
- [XENSEMBLE.md](XENSEMBLE.md) — 控制面映射
