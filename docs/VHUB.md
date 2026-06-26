# V-Hub（vsock / BLIN 协议）

Guest Agent 经 **virtio-vsock** 连到 Host 端口 `10000`（CID `2`，常量见 `blink-shared`），使用 20 字节 **BLIN** 包头多路复用 RPC 与流式输出。

Host 侧入口：`blink-cli serve --socket <path>`（`blink-sdk::serve_vhub`），需由 libkrun vsock bridge 将 Guest 连接转到该 Unix socket。

**交互式终端的主路径是 WebSocket attach，不是 V-Hub** — 见 [PTY.md](PTY.md)。**Pipe 模式 Agent 执行默认走 BoxLite portal exec，不依赖 vsock** — 见 [STREAMING.md](STREAMING.md)。

---

## 包头（20 字节）

| 字段 | 大小 | 说明 |
|------|------|------|
| Magic | 4 | `0x424C494E`（`BLIN`） |
| Version | 1 | `1` |
| MessageType | 1 | 见下表 |
| Flags | 2 | `0` |
| Payload length | 4 | LE u32 |
| Request ID | 8 | LE u64 |
| Payload | 变长 | 通常 RPC 为 JSON |

常量定义：`blink-shared`（`BLINK_MAGIC`、`PROTOCOL_VERSION`、`VHUB_PORT`、`VMADDR_CID_HOST`）。

---

## 消息类型

| Type | Value | 方向 | 用途 |
|------|-------|------|------|
| `Handshake` | `0x01` | 双向 | 会话建立 |
| `Heartbeat` | `0x02` | 双向 | 保活 |
| `Error` | `0x03` | 双向 | 协议错误 |
| `RpcRequest` | `0x10` | guest → host | 结构化 JSON RPC |
| `RpcResponse` | `0x11` | host → guest | RPC 回复 |
| `RpcError` | `0x12` | host → guest | RPC 失败 |
| `StreamData` | `0x20` | guest → host | 原始 PTY 字节流（预留） |
| `TtyResize` | `0x21` | host → guest | JSON `{"rows":N,"cols":N}` |
| `Stdout` | `0x30` | guest → host | Pipe 模式 stdout 块 |
| `Stderr` | `0x31` | guest → host | Pipe 模式 stderr 块 |

---

## Host 分发器（`serve_vhub`）

实现：`src/core/src/vhub.rs`

1. 首包必须是 `Handshake` → Host 回复 payload `blink-vhub-ready`
2. 循环读包，直到收到 `RpcRequest` → 回复空 JSON `{}` 的 `RpcResponse`，并将 request payload 返回给 CLI 调用方
3. 等待 RPC 期间：`Stdout` / `Stderr` / `StreamData` 写入 Host 终端 stdout/stderr；`TtyResize` / `Heartbeat` 静默忽略

`blink-cli serve` 收到 RPC payload 后以 JSON 打印到 stdout。

---

## 与 REST / PTY 的关系

```
Agent 短任务（默认）     BoxLite exec，无 vsock     →  STREAMING.md
交互式终端（生产路径）   spawn + WebSocket attach   →  PTY.md
Guest vsock RPC/流中继  BLIN @ :10000              →  本文档
```

V-Hub 上 `StreamData` / 完整 PTY-over-vsock 尚未作为生产路径接入；控制面可按需桥接 vsock。

---

## 相关文档

- [STREAMING.md](STREAMING.md) — Pipe / PTY 双轨 I/O 选型
- [PTY.md](PTY.md) — WebSocket attach 协议与示例
- [XENSEMBLE.md](XENSEMBLE.md) — 控制面 API 映射
