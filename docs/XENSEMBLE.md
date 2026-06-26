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

## 部署示例

```bash
# 终端 1：Blink 执行面（默认 localhost）
cargo run -p blink-server -- --port 8787

# 终端 2：XEnsemble 控制面
# BLINK_API_URL=http://127.0.0.1:8787
# RUNTIME_PROVIDER=boxlite
```

本地开发可继续用 `blink-cli` / `blink-sdk` 直接调 BoxLite，无需经过 XEnsemble。

---

## XEnsemble 待实现

当前 XEnsemble `BoxLiteRuntimeProvider` 等为 **501 占位**。接 Blink 时需：

1. 在 `BoxLiteRuntimeProvider` 内 HTTP 客户端调用 `BLINK_API_URL`
2. 实现 `ensureReady` / `destroy` / checkpoint 映射
3. `BoxLiteExecAdapter.spawn` 对接 Blink spawn + WebSocket attach（见 [PTY.md](PTY.md)）
4. 控制面 DB 增加 `runtime_providers` / `blink_session_ref` 映射

Blink 侧 API 已就绪；控制面 Adapter 实现可随 XEnsemble Phase 3 推进。
