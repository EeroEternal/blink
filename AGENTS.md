# AGENTS.md

## Documentation

`docs/` documentation filenames use a **single English word** (UPPERCASE) for easy reference and lookup:

| File | Topic |
|------|-------|
| [PRODUCT.md](docs/PRODUCT.md) | Product definition, free/paid, BYOK |
| [XENSEMBLE.md](docs/XENSEMBLE.md) | Control-plane integration contract |
| [STREAMING.md](docs/STREAMING.md) | Pipe / PTY dual-track I/O |
| [PTY.md](docs/PTY.md) | Interactive terminal (spawn + WebSocket) |
| [VHUB.md](docs/VHUB.md) | V-Hub vsock / BLIN protocol |

New documents should follow the same naming; proper nouns (e.g. `XENSEMBLE`) may be kept.

## Product

Blink rents isolated sandboxes for user-owned agents. **Ephemeral `run` = one-shot (free tier). Session + snapshot + export/import = paid persistence.** Details: [docs/PRODUCT.md](docs/PRODUCT.md).

**Blink is an execution-plane capability, not a user-facing product.** User login, Agent consoles, and quotas are provided by control planes such as [XEnsemble](https://github.com/EeroEternal/XEnsemble); Blink supplies a `blink-server` REST API for control planes to call. See the integration contract in [docs/XENSEMBLE.md](docs/XENSEMBLE.md).

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

- **Pipe (default):** Agent short tasks, BoxLite exec, optional `execution_result` JSON — [docs/STREAMING.md](docs/STREAMING.md)
- **PTY (interactive):** spawn + WebSocket attach — [docs/PTY.md](docs/PTY.md); XEnsemble terminal UI uses this path
- **V-Hub (optional):** Guest vsock RPC / stream relay, BLIN protocol — [docs/VHUB.md](docs/VHUB.md); `blink-cli serve --socket <path>`

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

- **blink-server**: performs no API authentication. Binds `127.0.0.1` by default; in production rely on network isolation (intranet / sidecar). User authentication and quotas are handled by the control plane.
