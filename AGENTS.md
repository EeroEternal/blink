# AGENTS.md

## Product

Blink rents isolated sandboxes for user-owned agents. **Ephemeral `run` = one-shot (free tier). Session + snapshot + export/import = paid persistence.** Details: [docs/PRODUCT.md](docs/PRODUCT.md).

**Blink 是执行面能力，不是用户产品。** 用户登录、Agent 控制台、配额由 [XEnsemble](https://github.com/EeroEternal/XEnsemble) 等控制面负责；Blink 通过 `blink-server` REST API 供控制面调用。集成契约见 [docs/XENSEMBLE.md](docs/XENSEMBLE.md)。

## 1. Goal

Agents in the Blink ecosystem are untrusted, dynamically generated, or ephemeral programs executing within a hardware-isolated libkrun VM sandbox provided by BoxLite. Integrators use the **`blink-sdk`** Rust crate (crates.io) or `blink-server` REST API.

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

### Communication (V-Hub)

For streaming and RPC, agents may connect over `AF_VSOCK` to port `10000` (CID `2`) using the BLIN 20-byte header protocol. Use `blink-cli serve --socket <path>` on the host and wire the socket via libkrun's vsock bridge. XEnsemble `BoxLiteExecAdapter.spawn` should bridge this for interactive terminals.

#### Vsock Payload Protocol

- `<I` (4 bytes): Magic `0x424C494E` (`BLIN`)
- `B` (1 byte): Version (`1`)
- `B` (1 byte): MessageType (`0x01` Handshake, `0x10` RpcRequest, etc.)
- `H` (2 bytes): Flags (`0`)
- `I` (4 bytes): Payload Length
- `Q` (8 bytes): Request ID
- Payload: Arbitrary length (usually JSON for RPC)

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
