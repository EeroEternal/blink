# Blink (v2.0)

Blink rents **hardware-isolated sandboxes for AI agents** — built on [BoxLite](https://github.com/boxlite-ai/boxlite)'s libkrun VM core.

**Product summary:** Ephemeral one-shot runs are for try/debug (free tier). Paid tier covers **persistent sessions, snapshots, export/import, and warm sessions**. Users bring their own Agent and API keys (BYOK). See **[docs/PRODUCT.md](docs/PRODUCT.md)** for the full product definition.

**Architecture role:** Blink is the **execution plane**. User login, quotas, and Agent consoles live in the **control plane** (e.g. [XEnsemble](https://github.com/EeroEternal/XEnsemble)). See **[docs/XENSEMBLE.md](docs/XENSEMBLE.md)** for integration.

## Architecture

- **blink-sdk**: Rust library for agent execution via BoxLite `LiteBox` + BLIN V-Hub protocol ([crates.io](https://crates.io/crates/blink-sdk))
- **blink-cli**: Host CLI (`run`, `session`, `serve`)
- **blink-server**: Sandbox REST API for control-plane consumers (`:8787`)
- **V-Hub**: 20-byte BLIN header over vsock port `10000` (see `docs/COMMUNICATION_ARCH.md`)

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

# V-Hub (vsock bridge wiring)
./target/debug/blink-cli serve --socket /tmp/blink-vhub.sock
```

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

```bash
PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo run -p blink-server
# http://127.0.0.1:8787 — service status page + REST API
```

XEnsemble integration: **[docs/XENSEMBLE.md](docs/XENSEMBLE.md)**

Blink **不做 API 鉴权**。用户身份与配额由控制面（XEnsemble）负责；Blink 默认只监听 `127.0.0.1`，生产环境通过内网或 sidecar 暴露：

```bash
# 默认 localhost:8787
cargo run -p blink-server

# 内网暴露（需确保网络隔离）
BLINK_BIND=0.0.0.0 cargo run -p blink-server -- --port 8787
```

### REST endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |
| GET | `/api/product` | Feature flags |
| POST | `/api/runs` | Queue ephemeral or session run (`script` = host path) |
| GET | `/api/runs/{id}` | Poll run status |
| GET | `/api/sessions` | List sessions |
| POST | `/api/sessions` | Open session (`{name, warm?}`) |
| POST | `/api/sessions/{name}/runs` | Run agent binary in session (`script` = host path) |
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
