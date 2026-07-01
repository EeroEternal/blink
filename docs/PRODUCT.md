# Blink Product Definition

## In One Sentence

**Blink rents "isolated sandboxes containing Agents"** — provides affordable runnable VM environments; users bring their own Agent and LLM Key (BYOK); pay when persistence, snapshots, import/export are needed.

Blink is not an Agent framework, does not dictate Agent output formats, and does not host user model tokens.

---

## Target Users: Why Not Run Locally?

Many users **cannot** or **should not** maintain Agent sandboxes locally:

- **Operating system**: Windows and similar environments cannot easily run the libkrun/KVM stack; macOS requires additional toolchains. A unified Linux VM in the cloud is transparent to users.
- **Temporary devices**: Internet cafes, borrowed laptops, lightweight Chromebooks — no persistent disk, no root, wipes on reboot. Cloud Session and Snapshot **are not bound to a device**.
- **Start/stop at will**: Closing the lid, shutting down, or switching machines breaks the environment locally. After saving in the cloud, open the same Session **at any time, from any terminal** or restore from a Snapshot.
- **Flexibility**: Export to take away, Import to migrate; trigger the same Agent environment from a phone, view progress on a tablet (via your product layer), while the underlying sandbox remains consistent in the cloud.

Local BoxLite/Blink remains suitable for teams that **have Linux/macOS dev machines, data must stay on-prem, long-running warm workloads, or their own machine rooms**. Blink rental primarily serves users whose **environment is uncontrollable, devices are temporary, or they treat the sandbox as a cloud asset** — it is not competing with "geeks who can use local Docker", but covering the part where **local runs simply cannot start or cannot preserve state**.

---

## What We Sell

Users rent a **named sandbox instance (Session)**:

- Hardware-isolated libkrun VM (powered by BoxLite underneath)
- Complete environment on disk: Agent code, dependencies, data, memory files, intermediate artifacts
- Optional: **Snapshot** to freeze the full disk state at a point in time
- Optional: **Export / Import** to package the entire sandbox for transport or restore (based on BoxLite archive capability)

Users can run any Agent inside the sandbox (LangChain, custom scripts, long-running daemons, etc.). Blink is responsible for **environment lifecycle**, not for the Agent's business logic.

---

## Free vs Paid

| Capability | Tier | Description |
|------------|------|-------------|
| **One-shot execution (Ephemeral)** | Free / Trial | `Box` / `blink-cli run`: create a fresh sandbox each time, delete after run, no disk retained |
| **Named Session (persistent disk)** | Paid | Same Agent environment across multiple runs, disk state preserved |
| **Snapshot create & restore** | Paid | Freeze / roll back the complete sandbox state at a moment |
| **Export (archive)** | Paid | Export a Session as a portable archive file |
| **Import (archive)** | Paid | Restore or migrate a sandbox from an archive |
| **Long-running resident (Warm Agent)** | Paid (planned) | Keep VM or Agent process running to reduce cold-start latency |

**Business model core:**

- **Ephemeral capability = one-shot execution**, for trials, debugging, lightweight tool-calls; low cost, may be free.
- **Persistence capability = paid points**: save, Snapshot, import, export — anything that "keeps everything inside this sandbox" is paid.

Pricing direction: keep **rental price as low as possible**, monetize via volume and persistence/storage add-ons rather than taking a cut on each execution.

---

## BYOK (Bring Your Own Key)

- Users provide their own LLM / API Tokens; Blink **does not** custody or **resell** tokens.
- Blink only provides the execution environment and (on-demand) outbound network; key injection is handled by the user or upper orchestration layer.
- Whether the operator proxies keys or acts as a gateway is a later product decision, **not part of Blink's core definition**.

---

## Structured Output: Deliberately Not Done

Agent types and execution patterns vary widely (chat, tool loops, multimodal, long-running tasks). **At this stage we do not enforce or promise** a unified Agent structured output schema.

Blink only guarantees:

- Container-level execution results (e.g. `execution_result`: stdout / stderr / exit_code)
- Optional interactive PTY (spawn + WebSocket attach, see [PTY.md](PTY.md))
- Optional V-Hub transport channel (see [VHUB.md](VHUB.md))

What the Agent says and in what format is decided by the user's own Agent framework. Blink does **sandbox rental**, not **Agent protocol standardization**.

---

## Relationship to "Blink = Fast"

"Blink (blink of an eye)" refers to **lightweight interaction and provisioning paths**, not a claim that VMs are always zero-latency:

- **Fast**: one-shot trials (ephemeral run), snapshot restore, export migration have simple operation paths
- **Slow is acceptable**: for long tasks and paid Session scenarios, users pay for **resumable environments**, not sub-second cold starts on every step

The long-term goal is to shorten multi-turn interaction waits via **Warm / resident Agents**, complementary to Snapshot freezing rather than a replacement.

---

## Technical Mapping (Current Implementation)

### Code Components

| Component | Form | Typical Usage |
|-----------|------|---------------|
| **`blink-sdk`** (crate `blinkvm-sdk`) | Rust library | Build your own Rust orchestrator, `cargo add blinkvm-sdk` |
| **`blink-server`** | Standalone process + REST | Control planes such as XEnsemble call via HTTP (see [XENSEMBLE.md](XENSEMBLE.md)) |
| **`blink-cli`** | Command line | Local trials, debugging, operations |

Core capabilities live in **`blink-sdk`**; `blink-server` and `blink-cli` are thin wrappers. **Control planes (e.g. XEnsemble) go through `blink-server` HTTP and do not need to link the Rust lib.**

### Product Capabilities

| Product Concept | Implementation |
|-----------------|----------------|
| One-shot execution | `blink-cli run`, `blink-sdk`, `POST /api/runs` |
| Named Session | `blink-cli session`, `blink-sdk`, `POST /api/sessions` |
| Interactive terminal (PTY) | `blink-cli session spawn --tty`, `POST /api/sessions/{name}/spawn` + WS attach (see [PTY.md](PTY.md)) |
| Snapshot | `session checkpoint`, `POST /api/sessions/{name}/checkpoints` |
| Execution-plane API | `blink-server` REST (for control planes such as XEnsemble to call, see [XENSEMBLE.md](XENSEMBLE.md)) |
| User console | **Not in Blink**; provided by XEnsemble Desktop / Web Admin |
| Agent memory directory | Inside Guest `/var/blink/memory/` (Session disk persistent) |
| Export / Import | BoxLite archive + REST API |
| Resident Agent | Planned: `detach` + Guest daemon + V-Hub |

Billing, quotas, multi-tenancy isolation, and user login belong to the **control plane (e.g. XEnsemble)**; Blink only provides Session / Snapshot / Export APIs and does not perform authentication.

---

## Out of Scope (Non-Goals)

- Do not host LLMs or perform Token billing
- Do not define a unified Agent output schema
- Do not become a general-purpose container PaaS (that is BoxLite's full scope; Blink focuses only on Agent sandbox rental)
