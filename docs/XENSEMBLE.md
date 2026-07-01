# Blink × XEnsemble Integration

Blink is the **Execution Plane**: provides hardware-isolated libkrun VM sandboxes.  
XEnsemble is the **Control Plane**: user login, quotas, Agent/Session orchestration, Desktop/Web UI.

**Desktop / Web clients do not call Blink directly.** Only XEnsemble Server's `BoxLiteRuntimeProvider` (and corresponding Adapters) call the Blink API using service credentials.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  XEnsemble Desktop / Web Admin                          │
│  (user login, Agent management, terminal, preview)      │
└───────────────────────────┬─────────────────────────────┘
                            │ HTTPS / WSS
                            ▼
┌─────────────────────────────────────────────────────────┐
│  XEnsemble Server (Control Plane)                       │
│  Auth · Quota · SessionManager · DeploymentService      │
│  BoxLiteRuntimeProvider / ExecAdapter / FsAdapter       │
└───────────────────────────┬─────────────────────────────┘
                            │ HTTP (intranet / localhost)
                            ▼
┌─────────────────────────────────────────────────────────┐
│  blink-server (Execution Plane)                         │
│  libkrun VM · Session · Snapshot · Export/Import        │
└─────────────────────────────────────────────────────────┘
```

Aligns with XEnsemble [`docs/Architecture.md`](https://github.com/EeroEternal/XEnsemble/blob/main/docs/Architecture.md) §5.4.2 BoxLite Managed Sandbox Runtime.

---

## Blink Components and Integration

The Blink repository is not a single executable, but a **library + service + CLI** three-layer structure:

| Component | Form | Audience | Description |
|-----------|------|----------|-------------|
| **`blink-sdk`** (crate `blinkvm-sdk`) | Rust **library** ([crates.io](https://crates.io/crates/blinkvm-sdk)) | Rust integrators | Sandbox execution, Session, Snapshot and other core capabilities |
| **`blink-server`** | Standalone **process** (REST `:8787`) | Control planes (incl. XEnsemble) | Thin HTTP wrapper, internally calls `blink-sdk` |
| **`blink-cli`** | Command-line tool | Local development / ops | Same as above, without going through HTTP |

**XEnsemble's standard integration path is HTTP, not Rust lib linking.**

XEnsemble Server is a Node.js control plane and should act as an **HTTP client** calling `blink-server` (sidecar or same-machine `127.0.0.1:8787`), with `BoxLiteRuntimeProvider` mapping the REST API. It is **neither necessary nor recommended** to link Blink into the XEnsemble process.

```
XEnsemble (Node.js)  ──HTTP──▶  blink-server (process)  ──▶  blink-sdk (lib)  ──▶  BoxLite / libkrun
```

Reasons for this design:

- **Language agnostic**: control plane and execution plane are decoupled; the Rust VM stack is not embedded in the Node process
- **Security boundary**: Blink binds to localhost by default; VM / libkrun isolation lives in a separate process
- **Clear responsibilities**: authentication and quotas live in XEnsemble; sandbox lifecycle lives in Blink

If the control plane itself is a **Rust** project, it can depend on the `blink-sdk` crate directly without going through `blink-server`. Under XEnsemble's current architecture, **process + REST** mode is still recommended.

---

## Responsibility Boundaries

| Layer | Responsible | Not Responsible |
|-------|-------------|-----------------|
| **XEnsemble** | User/Admin authentication, quotas, auditing, Agent authorization, LLM Gateway, Preview Gateway, Desktop terminal | VM creation, libkrun details, processes inside the sandbox |
| **Blink** | Ephemeral run, Session disk, Snapshot, Export/Import, V-Hub vsock | User accounts, Web login, Agent business protocol, LLM Token |

---

## Runtime Provider Mapping

XEnsemble `server/src/runtime/BoxLite*.js` should map to Blink REST API as follows (reference during implementation):

| XEnsemble Interface | Blink API | Description |
|---------------------|-----------|-------------|
| `RuntimeProvider.ensureReady(project, opts)` | `POST /api/sessions` | `name` = `{userId}-{projectId}` or runtime-mapped id; `warm` optional |
| `RuntimeProvider.destroy(runtimeRef)` | `DELETE /api/sessions/{name}` | Destroy sandbox |
| `RuntimeProvider.attachSession(...)` | WS `/api/sessions/{name}/executions/{id}/attach` | PTY streaming terminal (spawn first, see [PTY.md](PTY.md)) |
| `ExecAdapter.exec(cmd, args, env)` | `POST /api/sessions/{name}/runs` | Short task; or ephemeral `POST /api/runs` |
| `ExecAdapter.spawn(...)` | `POST /api/sessions/{name}/spawn` + WS `/executions/{id}/attach` | PTY interactive terminal, protocol compatible with BoxLite attach (see [PTY.md](PTY.md)) |
| Snapshot / checkpoint | `POST /api/sessions/{name}/checkpoints` | Corresponds to Deployment revision |
| Restore checkpoint | `POST /api/sessions/{name}/checkpoints/{snap}/restore` | |
| Export / migrate | `POST /api/sessions/{name}/export` + `POST /api/import` | |
| `FsAdapter.*` | (planned) sandbox file API or virtiofs proxy | Currently indirect via exec inside session |

Session naming recommendation: `{xensembleRuntimeId}`. The control plane maintains a `runtimeId ↔ blinkSessionName` mapping table. **Do not expose** Blink session names or internal box ids to clients.

---

## Security Boundary

Blink **does not perform API authentication**. Trust model:

- Blink listens on `127.0.0.1:8787` by default (`BLINK_BIND` / `--bind`)
- In production: Blink and XEnsemble Server run on the same machine or private network, **not exposed to the public internet**
- User login, quotas, and auditing are completed in the XEnsemble control plane; only then is Blink called

XEnsemble-side configuration (recommended):

```bash
export BLINK_API_URL=http://127.0.0.1:8787
export RUNTIME_PROVIDER=boxlite   # load BoxLiteRuntimeProvider
```

Optional extension (not implemented): request header `X-XEnsemble-User-Id` / `X-XEnsemble-Runtime-Id` for Blink audit logs.

---

## Product-Level Mapping

| Blink Capability | XEnsemble Scenario |
|------------------|--------------------|
| `POST /api/runs` (ephemeral) | Trial, one-off tool-call, CI probe |
| `POST /api/sessions` | Project workspace persistent environment |
| `POST /api/sessions/{name}/spawn` + WS attach | Desktop/Web interactive terminal |
| checkpoint / restore | Deployment revision, rollback |
| export / import | Environment migration, backup |
| warm session | Reduce Agent cold start |

XEnsemble `PolicyService` / quota decides whether a user may trigger persistence capabilities; Blink only executes and returns container-level results.

---

## Deployment

XEnsemble only needs a **long-running `blink-server` HTTP endpoint** (`BLINK_API_URL`). Integrators should use a **release binary or sidecar**; do not use `cargo run` (that is Blink repo local development usage, requires a Rust toolchain and triggers compilation).

### 1. Release Deployment (Recommended)

Deployers should use published Blink artifacts directly; no local BoxLite checkout, no `[patch.crates-io]`, and no need to build libkrun yourself.

```bash
docker pull ghcr.io/eeroeternal/blink-server:vX.Y.Z
docker run --rm --device /dev/kvm -p 8787:8787 ghcr.io/eeroeternal/blink-server:vX.Y.Z
curl -sf http://127.0.0.1:8787/api/health
```

Inside the container, `blink-server` binds `0.0.0.0:8787` by default. XEnsemble only needs to point `BLINK_API_URL` at the host-mapped address (e.g. `http://127.0.0.1:8787`) and ensure the host has KVM.

### 2. Build from Source (Local Development Only)

On a machine with the Blink repo or a pre-installed binary:

```bash
# one-time build (macOS requires LLVM in PATH, see repo README)
cargo build --release -p blink-server

# start execution plane (default 127.0.0.1:8787, no --port needed)
./target/release/blink-server
```

Environment variables (optional):

| Variable / Flag | Default | Description |
|-----------------|---------|-------------|
| `BLINK_BIND` / `--bind` | `127.0.0.1` | Listen address; keep default for same-host sidecar |
| `--port` | `8787` | Port |

For multi-machine intranet deployment (ensure network isolation; Blink has no API auth):

```bash
BLINK_BIND=0.0.0.0 ./target/release/blink-server
```

### 3. Health Check

Before starting XEnsemble or during orchestration readiness probes, confirm Blink is available:

```bash
curl -sf http://127.0.0.1:8787/api/health
# {"status":"ok","service":"blink-server",...}
```

### 4. Start XEnsemble Control Plane

```bash
export BLINK_API_URL=http://127.0.0.1:8787
export RUNTIME_PROVIDER=boxlite   # load BoxLiteRuntimeProvider
# start Server per XEnsemble docs (npm / node / your process manager)
```

The control plane accesses Blink via `BLINK_API_URL`; Desktop/Web **does not** connect directly to Blink.

### 5. Sidecar Deployment (Recommended for Production)

**Same-host dual process:** `blink-server` and XEnsemble Server on the same host, `BLINK_API_URL=http://127.0.0.1:8787`, Blink not exposed publicly.

**systemd example** (adjust paths to actual installation):

```ini
[Unit]
Description=Blink sandbox execution plane
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/blink-server
Environment=BLINK_BIND=127.0.0.1
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

**Container / Compose:** run `blink-server` as a sidecar container, sharing network namespace or private bridge with XEnsemble; XEnsemble's `BLINK_API_URL` points at the sidecar service name (e.g. `http://blink:8787`). Production images use the published Blink release directly; no local BoxLite checkout or libkrun build required; the runtime still needs a Linux KVM host and `/dev/kvm`.

Startup order: start `blink-server` **first** and pass `/api/health`, **then** start XEnsemble Server.

### 6. Blink Repo Local Development

`cargo run -p blink-server` is usable when only modifying Blink itself; when integrating with XEnsemble, still prefer the release binary so behavior matches production. To test BoxLite directly, use `blink-cli` / `blink-sdk` without going through XEnsemble.

---

## XEnsemble Items to Implement

Current XEnsemble `BoxLiteRuntimeProvider` etc. are **501 placeholders**. When wiring Blink:

1. Inside `BoxLiteRuntimeProvider`, have the HTTP client call `BLINK_API_URL`
2. Implement `ensureReady` / `destroy` / checkpoint mapping
3. Wire `BoxLiteExecAdapter.spawn` to Blink spawn + WebSocket attach (see [PTY.md](PTY.md))
4. Add `runtime_providers` / `blink_session_ref` mapping to the control-plane DB

Blink-side APIs are ready; control-plane Adapter implementation can proceed with XEnsemble Phase 3.
