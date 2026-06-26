# Blink (v2.0)

Blink rents **hardware-isolated sandboxes for AI agents** — built on [BoxLite](https://github.com/boxlite-ai/boxlite)'s libkrun VM core.

**Product:** Ephemeral one-shot runs for try/debug (free tier). Paid tier covers **persistent sessions, snapshots, export/import, and warm sessions**. Users bring their own Agent and API keys (BYOK). Full definition → **[docs/PRODUCT.md](docs/PRODUCT.md)**

**Role:** Blink is the **execution plane**. User login, quotas, and Agent consoles live in the **control plane** (e.g. [XEnsemble](https://github.com/EeroEternal/XEnsemble)). Integration contract → **[docs/XENSEMBLE.md](docs/XENSEMBLE.md)**

## Documentation

| Topic | Doc |
|-------|-----|
| Product definition, free vs paid, BYOK | [docs/PRODUCT.md](docs/PRODUCT.md) |
| Control-plane integration (XEnsemble) | [docs/XENSEMBLE.md](docs/XENSEMBLE.md) |
| Dual-track output (pipe vs PTY) | [docs/STREAMING.md](docs/STREAMING.md) |
| Interactive terminal (spawn + WebSocket) | [docs/PTY.md](docs/PTY.md) |
| V-Hub vsock / BLIN protocol | [docs/VHUB.md](docs/VHUB.md) |

## Architecture

Blink is **library + service + CLI**, not a single binary:

| Component | Type | Integrators |
|-----------|------|-------------|
| **blink-sdk** | Rust library — BoxLite `LiteBox` + BLIN V-Hub ([crates.io](https://crates.io/crates/blink-sdk)) | Rust apps embed directly |
| **blink-server** | HTTP process (`:8787`) — thin REST wrapper over `blink-sdk` | Control planes call via HTTP ([XENSEMBLE.md](docs/XENSEMBLE.md)) |
| **blink-cli** | Host CLI (`run`, `session`, `serve`) | Local dev and ops |

**Control-plane integration:** run `blink-server` as a sidecar; the control plane's Runtime Provider calls REST — no Rust linking required. See **[docs/XENSEMBLE.md](docs/XENSEMBLE.md)**.

**Execution modes** (see [STREAMING.md](docs/STREAMING.md)):

| Mode | Use case | Entry |
|------|----------|-------|
| **Pipe** | AI Agent short tasks, structured JSON | `run`, `POST /api/runs`, `POST /api/sessions/{name}/runs` |
| **PTY** | Shell, TUI, interactive terminal | `session spawn --tty`, `POST .../spawn` + WebSocket attach ([PTY.md](docs/PTY.md)) |
| **V-Hub** | Optional vsock RPC / stream relay | `blink-cli serve --socket <path>` ([VHUB.md](docs/VHUB.md)) |

## Prerequisites

- Rust 1.88+
- macOS: `brew install llvm lld` (libkrun cross-compilation); build with `PATH="/opt/homebrew/opt/llvm/bin:$PATH"`

BoxLite is pulled from [crates.io](https://crates.io/crates/boxlite) automatically. For local BoxLite development, add a `[patch.crates-io]` section in `Cargo.toml`.

## Build

```bash
cargo build
```

Binaries: `target/debug/blink-cli`, `target/debug/blink-server`

## Usage

```bash
# Free tier: one-shot ephemeral run (agent binary or script on host)
./target/debug/blink-cli run /path/to/agent

# Paid tier: named session with persistent disk + snapshots
./target/debug/blink-cli session open --name my-agent
./target/debug/blink-cli session open --name my-agent --warm   # keep VM alive
./target/debug/blink-cli session run --name my-agent /path/to/agent
./target/debug/blink-cli session checkpoint --name my-agent --snapshot step-1
./target/debug/blink-cli session list --name my-agent
./target/debug/blink-cli session ls
./target/debug/blink-cli session export --name my-agent
./target/debug/blink-cli session import archive.boxlite --name my-agent

# PTY: interactive shell in session (see docs/PTY.md)
./target/debug/blink-cli session spawn --name my-agent --tty -- sh -i

# V-Hub (vsock bridge wiring — see docs/VHUB.md)
./target/debug/blink-cli serve --socket /tmp/blink-vhub.sock
```

Agent memory persists under `/var/blink/memory/` on the session disk. Details → [docs/PRODUCT.md](docs/PRODUCT.md)

### Rust SDK

Add to `Cargo.toml`:

```toml
blink-sdk = "0.2"
```

```rust
use blink_sdk::{run_agent_script_default, open_session, run_in_session};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let result = run_agent_script_default("/path/to/agent".as_ref()).await?;
    println!("exit={}", result.exit_code);

    let (box_id, _created) = open_session("my-agent", "alpine:3.20").await?;
    let result = run_in_session("my-agent", "/path/to/agent".as_ref()).await?;
    Ok(())
}
```

## Sandbox API (for control planes)

控制面（如 XEnsemble）通过 HTTP 调用 **`blink-server`**，不链接 Rust lib。完整集成与 sidecar 部署 → **[docs/XENSEMBLE.md](docs/XENSEMBLE.md)**。

```bash
cargo build --release -p blink-server
./target/release/blink-server          # 默认 http://127.0.0.1:8787
curl -sf http://127.0.0.1:8787/api/health
```

Blink **不做 API 鉴权**。用户身份与配额由控制面负责；默认只监听 `127.0.0.1`，生产环境同机 sidecar 或内网暴露（`BLINK_BIND=0.0.0.0` 时需网络隔离）。

Blink 仓库内快速迭代可用 `cargo run -p blink-server`；对接控制面时请用 release 二进制，见 [XENSEMBLE.md](docs/XENSEMBLE.md)。

### REST endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |
| GET | `/api/product` | Feature flags |
| POST | `/api/runs` | Queue ephemeral or session run (`script` = host path) |
| GET | `/api/runs/{id}` | Poll run status |
| GET | `/api/sessions` | List sessions |
| POST | `/api/sessions` | Open session (`{name, warm?}`) |
| GET | `/api/sessions/{name}` | Session info |
| POST | `/api/sessions/{name}/runs` | Run agent in session — pipe mode ([STREAMING.md](docs/STREAMING.md)) |
| POST | `/api/sessions/{name}/spawn` | Spawn process — PTY or pipe ([PTY.md](docs/PTY.md)) |
| WS | `/api/sessions/{name}/executions/{id}/attach` | WebSocket attach for spawn ([PTY.md](docs/PTY.md)) |
| POST | `/api/sessions/{name}/checkpoints` | Create snapshot |
| GET | `/api/sessions/{name}/checkpoints` | List snapshots |
| POST | `/api/sessions/{name}/checkpoints/{snap}/restore` | Restore snapshot |
| POST | `/api/sessions/{name}/export` | Export to `.boxlite` |
| GET | `/api/exports/{filename}` | Download export |
| POST | `/api/import` | Multipart upload (`archive`, optional `name`) |
| POST | `/api/sessions/{name}/stop` | Stop VM |
| DELETE | `/api/sessions/{name}` | Remove session |

## Publishing to crates.io

```bash
# Publish shared types first, then the SDK
cargo publish -p blink-shared
cargo publish -p blink-sdk
```

CI publishes both crates on git tags matching `v*` (see `.github/workflows/publish.yml`).
