# AGENTS.md

## Documentation

`docs/` 下文档文件名使用 **单个英文词**（UPPERCASE），便于引用与检索：

| 文件 | 主题 |
|------|------|
| [PRODUCT.md](docs/PRODUCT.md) | 产品定义、免费/付费、BYOK |
| [XENSEMBLE.md](docs/XENSEMBLE.md) | 控制面集成契约 |
| [STREAMING.md](docs/STREAMING.md) | Pipe / PTY 双轨 I/O |
| [PTY.md](docs/PTY.md) | 交互式终端（spawn + WebSocket） |
| [VHUB.md](docs/VHUB.md) | V-Hub vsock / BLIN 协议 |

新增文档请延续此命名；专有名词（如 `XENSEMBLE`）可保留。

## Product

Blink rents isolated sandboxes for user-owned agents. **Ephemeral `run` = one-shot (free tier). Session + snapshot + export/import = paid persistence.** Details: [docs/PRODUCT.md](docs/PRODUCT.md).

**Blink 是执行面能力，不是用户产品。** 用户登录、Agent 控制台、配额由 [XEnsemble](https://github.com/EeroEternal/XEnsemble) 等控制面负责；Blink 通过 `blink-server` REST API 供控制面调用。集成契约见 [docs/XENSEMBLE.md](docs/XENSEMBLE.md)。

## 1. Goal

Agents in the Blink ecosystem are untrusted, dynamically generated, or ephemeral programs executing within a hardware-isolated libkrun VM sandbox provided by BoxLite.

**Integrator paths:** Core logic lives in **`blink-sdk`** (Rust library, crates.io). **`blink-server`** is an HTTP sidecar over that library; **`blink-cli`** is for local dev. Control planes such as XEnsemble call **`blink-server` REST** — they do not link `blink-sdk`. Rust-only integrators may depend on `blink-sdk` directly. See [docs/XENSEMBLE.md](docs/XENSEMBLE.md).

## 2. Basic Specifications

### Execution Environment

- **Hypervisor**: libkrun via BoxLite (`boxlite-shim` + jailer)
- **Root Filesystem:** OCI image rootfs inside the VM (default `alpine:3.20`)
- **Hot-zone:** Container `/tmp` for ephemeral agent artifacts
- **Runtimes:** User-provided agent binaries or scripts copied into the sandbox; host maps additional runtimes via virtiofs when needed

### Lifecycle

1. Control plane (e.g. XEnsemble `BoxLiteRuntimeProvider`) calls `POST /api/sessions` or `POST /api/runs`.
2. Host `blink-cli run` remains for direct/local one-shot execution.
3. Agent binary/script is copied into the sandbox, marked executable, and run.
4. Agent may report results as JSON (`execution_result` event) on stdout; otherwise Blink returns raw stdout/stderr/exit_code.
5. Blink returns structured JSON to the caller (stdout/stderr/exit_code).

### I/O paths

- **Pipe（默认）：** Agent 短任务，BoxLite exec，可选 `execution_result` JSON — [docs/STREAMING.md](docs/STREAMING.md)
- **PTY（交互式）：** spawn + WebSocket attach — [docs/PTY.md](docs/PTY.md)；XEnsemble 终端 UI 走此路径
- **V-Hub（可选）：** Guest vsock RPC / 流中继，BLIN 协议 — [docs/VHUB.md](docs/VHUB.md)；`blink-cli serve --socket <path>`

### Persistent Sessions (long-running agents)

For agents that run across multiple steps or need resumable memory:

1. `blink-cli session open --name my-agent` — create/reuse a named box (`auto_remove=false`).
2. Agent memory lives under `/var/blink/memory/` on the session disk (survives stop/start).
3. `blink-cli session checkpoint --name my-agent --snapshot step-3` — full disk snapshot via BoxLite QCOW2.
4. `blink-cli session restore --name my-agent --snapshot step-3` — rewind disk state (box must be stopped).
5. Rust: `blink-sdk` — `open_session()`, `run_in_session()`, `checkpoint_session()`, `restore_session()`.
6. Control plane: map XEnsemble runtime/session ids to Blink session names (internal only).

Ephemeral `blink-cli run` remains for one-shot execution without persistence.

### Authentication

- **blink-server**：无 API 鉴权。默认绑定 `127.0.0.1`；生产环境靠网络隔离（内网 / sidecar），用户鉴权与配额由控制面负责。
