# Blink 产品定义

## 一句话

**Blink 出租「内含 Agent 的隔离沙箱」**——低价提供可运行的 VM 环境；用户自带 Agent 与 LLM Key（BYOK）；需要保存、快照、导入导出时付费。

Blink 不是 Agent 框架，不规定 Agent 的输出格式，也不托管用户的模型 Token。

---

## 目标用户：为什么不在本地跑？

很多用户**没法**或**不适合**在本地维护 Agent 沙箱：

- **操作系统**：Windows 等环境难以直接跑 libkrun/KVM 栈；Mac 需额外工具链。云端统一 Linux VM，用户无感。
- **临时设备**：网吧、借用的笔记本、轻量 Chromebook——没有持久磁盘、没有 root、重启即清空。云端 Session 与 Snapshot **不绑设备**。
- **随时启停**：本地合盖、关机、换机器，环境就断。云端保存后**任意时间、任意终端**打开同一 Session 或从 Snapshot 恢复。
- **灵活性**：Export 带走、Import 迁移；同一 Agent 环境在手机上触发、在平板上查看进度（通过你的产品层），底层沙箱在云端一致。

本地 BoxLite/Blink 仍适合：**有 Linux/macOS 开发机、数据不出域、长期 warm、自己有机房** 的团队。Blink 出租主要服务 **环境不可控、设备临时、要把沙箱当云端资产** 的用户——不是和「会用本地 Docker 的极客」抢同一类人，而是覆盖 **本地根本跑不起来或守不住状态** 的那部分。

---

## 我们卖什么

用户租到的是一台 **命名沙箱实例（Session）**：

- 硬件级隔离的 libkrun VM（底层由 BoxLite 提供）
- 磁盘上的完整环境：Agent 代码、依赖、数据、记忆文件、中间产物
- 可选：**Snapshot** 封存某一时刻的整机磁盘状态
- 可选：**Export / Import** 将整台沙箱打包带走或还原（基于 BoxLite 归档能力）

用户可以在沙箱里跑任意 Agent（LangChain、自研脚本、长期 daemon 等）。Blink 负责 **环境的生命周期**，不负责 Agent 的业务逻辑。

---

## 免费 vs 付费

| 能力 | 层级 | 说明 |
|------|------|------|
| **单次执行（Ephemeral）** | 免费 / 试用 | `Box` / `blink-cli run`：每次新建沙箱，跑完即删，不保留磁盘 |
| **命名 Session（持久磁盘）** | 付费 | 同一 Agent 环境跨多次运行，磁盘状态保留 |
| **Snapshot 创建与恢复** | 付费 | 封存 / 回滚某一时刻的完整沙箱状态 |
| **Export（导出归档）** | 付费 | 将 Session 导出为可迁移的归档文件 |
| **Import（导入归档）** | 付费 | 从归档恢复或迁移沙箱 |
| **长期常驻（Warm Agent）** | 付费（规划） | VM 或 Agent 进程保持运行，减少逐步冷启动 |

**商业模式核心：**

- **临时能力 = 单次执行**，用于体验、调试、轻量 tool-call，成本低，可不收费。
- **持久化能力 = 付费点**：保存、Snapshot、导入、导出——凡是要「留住这台沙箱里的一切」，都走付费。

定价策略方向：**出租价尽量低**，靠 volume 与持久化/storage 增值项盈利，而非单次执行抽成。

---

## BYOK（Bring Your Own Key）

- 用户自行提供 LLM / API Token，Blink **不**代管、不**转售** Token。
- Blink 只提供执行环境与（按需）出站网络；Key 注入方式由用户或上层编排器负责。
- 运营侧是否代传 Key、是否做网关，属于后续产品决策，**不属于 Blink 核心定义**。

---

## 结构化输出：刻意不做

Agent 类型多、执行形态杂（对话、tool loop、多模态、长期任务），**现阶段不强制、不承诺**统一的 Agent 结构化输出 schema。

Blink 层只保证：

- 执行容器级结果（如 `execution_result`：stdout / stderr / exit_code）
- 可选的交互式 PTY（spawn + WebSocket attach，见 [PTY.md](PTY.md)）
- 可选的 V-Hub 传输通道（见 [COMMUNICATION_ARCH.md](COMMUNICATION_ARCH.md)）

Agent 说什么、什么格式，由用户自己的 Agent 框架决定。Blink 做 **沙箱租赁**，不做 **Agent 协议标准化**。

---

## 与「Blink = 快」的关系

「Blink（眨眼）」指 **交互与开通路径轻**，不是声称 VM 永远零延迟：

- **快**：单次试用（ephemeral run）、快照恢复、导出迁移的操作路径简单
- **慢可接受**：长任务、付费 Session 场景下，用户买的是 **环境可续**，不是每步亚秒级冷启动

长期目标是通过 **Warm / 常驻 Agent** 缩短多轮交互的等待，与 Snapshot 封存互补，而非替代。

---

## 技术映射（当前实现）

| 产品概念 | 实现 |
|----------|------|
| 单次执行 | `blink-cli run`，`blink-sdk`，`POST /api/runs` |
| 命名 Session | `blink-cli session`，`blink-sdk`，`POST /api/sessions` |
| 交互式终端（PTY） | `blink-cli session spawn --tty`，`POST /api/sessions/{name}/spawn` + WS attach（见 [PTY.md](PTY.md)） |
| Snapshot | `session checkpoint`，`POST /api/sessions/{name}/checkpoints` |
| 执行面 API | `blink-server` REST（供 XEnsemble 等控制面调用，见 [docs/XENSEMBLE.md](XENSEMBLE.md)） |
| 用户控制台 | **不在 Blink**；由 XEnsemble Desktop / Web Admin 提供 |
| Agent 记忆目录 | Guest 内 `/var/blink/memory/`（Session 磁盘持久） |
| Export / Import | BoxLite 归档 + REST API |
| 常驻 Agent | 规划中：`detach` + Guest daemon + V-Hub |

计费、配额、多租户隔离、用户登录属于 **控制面（如 XEnsemble）**；Blink 只提供 Session / Snapshot / Export API，不做鉴权。

---

## 非目标（Out of Scope）

- 不做托管 LLM 与 Token 计费
- 不做统一 Agent 输出 schema
- 不做通用容器 PaaS（那是 BoxLite 全量能力；Blink 只聚焦 Agent 沙箱租赁）
