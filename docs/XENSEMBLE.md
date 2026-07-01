# Blink × XEnsemble 集成

Blink 是 **执行面（Execution Plane）**：提供硬件隔离的 libkrun VM 沙箱。  
XEnsemble 是 **控制面（Control Plane）**：用户登录、配额、Agent/Session 编排、Desktop/Web UI。

**Desktop / Web 客户端不直接调用 Blink。** 只有 XEnsemble Server 的 `BoxLiteRuntimeProvider`（及对应 Adapter）通过服务密钥调用 Blink API。

---

## 架构

```
┌─────────────────────────────────────────────────────────┐
│  XEnsemble Desktop / Web Admin                          │
│  （用户登录、Agent 管理、终端、预览）                      │
└───────────────────────────┬─────────────────────────────┘
                            │ HTTPS / WSS
                            ▼
┌─────────────────────────────────────────────────────────┐
│  XEnsemble Server（控制面）                               │
│  Auth · Quota · SessionManager · DeploymentService      │
│  BoxLiteRuntimeProvider / ExecAdapter / FsAdapter       │
└───────────────────────────┬─────────────────────────────┘
                            │ HTTP（内网 / localhost）
                            ▼
┌─────────────────────────────────────────────────────────┐
│  blink-server（执行面）                                   │
│  libkrun VM · Session · Snapshot · Export/Import        │
└─────────────────────────────────────────────────────────┘
```

对齐 XEnsemble [`docs/Architecture.md`](https://github.com/EeroEternal/XEnsemble/blob/main/docs/Architecture.md) §5.4.2 BoxLite Managed Sandbox Runtime。

---

## Blink 组件与集成方式

Blink 仓库不是单一可执行文件，而是 **库 + 服务 + CLI** 三层：

| 组件 | 形态 | 面向谁 | 说明 |
|------|------|--------|------|
| **`blink-sdk`**（`src/core`，crate `blinkvm-sdk`） | Rust **library**（[crates.io](https://crates.io/crates/blinkvm-sdk)） | Rust 集成方 | 沙箱执行、Session、Snapshot 等核心能力 |
| **`blink-server`** | 独立 **进程**（REST `:8787`） | 控制面（含 XEnsemble） | HTTP 薄封装，内部调用 `blink-sdk` |
| **`blink-cli`** | 命令行工具 | 本地开发 / 运维 | 同上，不经 HTTP |

**XEnsemble 的标准集成路径是 HTTP，不是 Rust lib 链接。**

XEnsemble Server 是 Node.js 控制面，应作为 **HTTP 客户端** 调用 `blink-server`（sidecar 或同机 `127.0.0.1:8787`），由 `BoxLiteRuntimeProvider` 映射 REST API。**不需要**、也**不建议**把 Blink 链进 XEnsemble 进程。

```
XEnsemble (Node.js)  ──HTTP──▶  blink-server (进程)  ──▶  blink-sdk (库)  ──▶  BoxLite / libkrun
```

这样设计的原因：

- **语言无关**：控制面与执行面解耦，Rust VM 栈不嵌入 Node 进程
- **安全边界**：Blink 默认只绑 localhost；VM / libkrun 隔离在独立进程
- **职责清晰**：鉴权、配额在 XEnsemble；沙箱生命周期在 Blink

若控制面本身是 **Rust** 项目，可直接依赖 `blink-sdk` crate，无需经过 `blink-server`。XEnsemble 当前架构下仍推荐 **进程 + REST** 模式。

---

## 职责边界

| 层 | 负责 | 不负责 |
|----|------|--------|
| **XEnsemble** | 用户/Admin 认证、配额、审计、Agent 授权、LLM Gateway、Preview Gateway、Desktop 终端 | VM 创建、libkrun 细节、沙箱内进程 |
| **Blink** | Ephemeral run、Session 磁盘、Snapshot、Export/Import、V-Hub vsock | 用户账号、Web 登录、Agent 业务协议、LLM Token |

---

## Runtime Provider 映射

XEnsemble `server/src/runtime/BoxLite*.js` 应对 Blink REST API 做如下映射（实现时参考）：

| XEnsemble 接口 | Blink API | 说明 |
|----------------|-----------|------|
| `RuntimeProvider.ensureReady(project, opts)` | `POST /api/sessions` | `name` = `{userId}-{projectId}` 或 runtime 映射 id；`warm` 可选 |
| `RuntimeProvider.destroy(runtimeRef)` | `DELETE /api/sessions/{name}` | 销毁沙箱 |
| `RuntimeProvider.attachSession(...)` | WS `/api/sessions/{name}/executions/{id}/attach` | PTY 流式终端（需先 spawn，见 [PTY.md](PTY.md)） |
| `ExecAdapter.exec(cmd, args, env)` | `POST /api/sessions/{name}/runs` | 短任务；或 ephemeral `POST /api/runs` |
| `ExecAdapter.spawn(...)` | `POST /api/sessions/{name}/spawn` + WS `/executions/{id}/attach` | PTY 交互终端，协议与 BoxLite attach 兼容（见 [PTY.md](PTY.md)） |
| Snapshot / checkpoint | `POST /api/sessions/{name}/checkpoints` | 对应 Deployment revision |
| Restore checkpoint | `POST /api/sessions/{name}/checkpoints/{snap}/restore` | |
| Export / migrate | `POST /api/sessions/{name}/export` + `POST /api/import` | |
| `FsAdapter.*` | （规划）沙箱内文件 API 或 virtiofs 代理 | 当前可通过 session 内 exec 间接操作 |

Session 命名建议：`{xensembleRuntimeId}`，由控制面维护 `runtimeId ↔ blinkSessionName` 映射表，**不向客户端暴露** Blink session 名或内部 box id。

---

## 安全边界

Blink **不做 API 鉴权**。信任模型：

- Blink 默认监听 `127.0.0.1:8787`（`BLINK_BIND` / `--bind`）
- 生产环境：Blink 与 XEnsemble Server 同机或同私有网络，**不暴露公网**
- 用户登录、配额、审计均在 XEnsemble 控制面完成，通过后才调用 Blink

XEnsemble 侧配置（建议）：

```bash
export BLINK_API_URL=http://127.0.0.1:8787
export RUNTIME_PROVIDER=boxlite   # 加载 BoxLiteRuntimeProvider
```

可选扩展（未实现）：请求头 `X-XEnsemble-User-Id` / `X-XEnsemble-Runtime-Id` 供 Blink 审计日志使用。

---

## 产品层级映射

| Blink 能力 | XEnsemble 场景 |
|------------|----------------|
| `POST /api/runs` (ephemeral) | 试用、一次性 tool-call、CI 探测 |
| `POST /api/sessions` | 项目 workspace 持久环境 |
| `POST /api/sessions/{name}/spawn` + WS attach | Desktop/Web 交互式终端 |
| checkpoint / restore | Deployment revision、回滚 |
| export / import | 环境迁移、备份 |
| warm session | 减少 Agent 冷启动 |

XEnsemble `PolicyService` / quota 决定是否允许用户触发持久化能力；Blink 仅执行并返回容器级结果。

---

## 部署

XEnsemble 只需一个 **长期运行的 `blink-server` HTTP 端点**（`BLINK_API_URL`）。集成方应使用 **release 二进制或 sidecar**，不要用 `cargo run`（那是 Blink 仓库本地开发用法，需 Rust 工具链且会触发编译）。

### 1. 发布版部署（推荐）

部署方应直接使用 Blink 发布物，不需要本地 BoxLite checkout、`[patch.crates-io]`，也不需要自己构建 libkrun。

```bash
docker pull ghcr.io/eeroeternal/blink-server:vX.Y.Z
docker run --rm --device /dev/kvm -p 8787:8787 ghcr.io/eeroeternal/blink-server:vX.Y.Z
curl -sf http://127.0.0.1:8787/api/health
```

容器内 `blink-server` 默认绑定 `0.0.0.0:8787`。XEnsemble 只需把 `BLINK_API_URL` 指向宿主映射的地址（例如 `http://127.0.0.1:8787`），并确保宿主具备 KVM。

### 2. 从源码构建（仅本地开发）

在 Blink 仓库或已安装二进制的机器上：

```bash
# 一次性构建（macOS 需 LLVM 在 PATH，见仓库 README）
cargo build --release -p blink-server

# 启动执行面（默认 127.0.0.1:8787，无需 --port）
./target/release/blink-server
```

环境变量（可选）：

| 变量 / 参数 | 默认 | 说明 |
|-------------|------|------|
| `BLINK_BIND` / `--bind` | `127.0.0.1` | 监听地址；同机 sidecar 保持默认即可 |
| `--port` | `8787` | 端口 |

内网多机部署时（需确保网络隔离，Blink 无 API 鉴权）：

```bash
BLINK_BIND=0.0.0.0 ./target/release/blink-server
```

### 3. 健康检查

XEnsemble 启动前或编排就绪探针应确认 Blink 可用：

```bash
curl -sf http://127.0.0.1:8787/api/health
# {"status":"ok","service":"blink-server",...}
```

### 4. 启动 XEnsemble 控制面

```bash
export BLINK_API_URL=http://127.0.0.1:8787
export RUNTIME_PROVIDER=boxlite   # 加载 BoxLiteRuntimeProvider
# 按 XEnsemble 文档启动 Server（npm / node / 你的进程管理器）
```

控制面通过 `BLINK_API_URL` 访问 Blink；Desktop/Web **不**直连 Blink。

### 5. Sidecar 部署（生产推荐）

**同机双进程：** `blink-server` 与 XEnsemble Server 在同一 host，`BLINK_API_URL=http://127.0.0.1:8787`，Blink 不暴露公网。

**systemd 示例**（路径按实际安装调整）：

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

**容器 / Compose：** 将 `blink-server` 作为 sidecar 容器，与 XEnsemble 共享 network namespace 或私有 bridge；XEnsemble 的 `BLINK_API_URL` 指向 sidecar 服务名（如 `http://blink:8787`）。生产镜像直接使用发布版 Blink，不需要本地 BoxLite checkout 或 libkrun 构建；运行时仍需要 Linux KVM 主机和 `/dev/kvm`。

启动顺序：**先** `blink-server` 并通过 `/api/health`，**再** 启动 XEnsemble Server。

### 6. Blink 仓库本地开发

仅改 Blink 本身时可用 `cargo run -p blink-server`；对接 XEnsemble 时仍建议用 release 二进制，行为与生产一致。直接测 BoxLite 可用 `blink-cli` / `blink-sdk`，无需经过 XEnsemble。

---

## XEnsemble 待实现

当前 XEnsemble `BoxLiteRuntimeProvider` 等为 **501 占位**。接 Blink 时需：

1. 在 `BoxLiteRuntimeProvider` 内 HTTP 客户端调用 `BLINK_API_URL`
2. 实现 `ensureReady` / `destroy` / checkpoint 映射
3. `BoxLiteExecAdapter.spawn` 对接 Blink spawn + WebSocket attach（见 [PTY.md](PTY.md)）
4. 控制面 DB 增加 `runtime_providers` / `blink_session_ref` 映射

Blink 侧 API 已就绪；控制面 Adapter 实现可随 XEnsemble Phase 3 推进。
