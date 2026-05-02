# AGENTS.md

## 1. Goal
Agents in the Blink ecosystem are untrusted, dynamically generated, or ephemeral scripts (primarily Python or TypeScript) executing within the hardware-isolated `libkrun` VM sandbox.

## 2. Basic Specifications

### Execution Environment
- **Root Filesystem:** Read-only (`/`) ephemeral filesystem.
- **Hot-zone:** Read-write memory-backed tmpfs (`/tmp`). This is the only mutable storage layer for Agent artifacts.
- **Runtimes:** The interpreter (e.g., Python 3.11) is ambiently mapped directly from the Host via `virtio-fs` DAX mode into `/lib/runtime`.

### Lifecycle
1. The Host dynamically configures the VM context using `blink-core`.
2. The statically cross-compiled `blink-init` boots as PID 1 to clean up zombies.
3. `blink-init` uses `execveZ` to execute the Agent payload.
4. When the Agent exits, the `init` traps the exit status and halts the VM.

### Communication (V-Hub)
Agents **MUST NOT** use traditional TCP/IP networking (e.g., `AF_INET`) to communicate out of the sandbox.
All IO happens over `AF_VSOCK` bound to Port `10000` (by default) targeting Context ID (CID) `2` (`VMADDR_CID_HOST`).

### Vsock Payload Protocol
Agent implementations must adhere to the 20-byte strict C ABI protocol:
- `<I` (4 bytes): Magic Number `0x424C494E` (`BLIN`)
- `B` (1 byte): Version (`1`)
- `B` (1 byte): MessageType (`0x01` Handshake, `0x10` RpcRequest, etc.)
- `H` (2 bytes): Flags (`0`)
- `I` (4 bytes): Payload Length
- `Q` (8 bytes): Request ID (Async tracking counter)
- Payload: Arbitrary length (usually JSON for RPC).

## 3. Creating an Agent (Python Example)

Use the built-in standard `socket` and `struct` libraries. See `examples/python/guest_agent.py` for a fully functional RPC implementation.
