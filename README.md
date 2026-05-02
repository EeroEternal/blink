# Blink (v2.0)

Blink is a stateless, micro-runtime designed for AI Agents. It provides an execution environment that is "as light as a process, but as strong as a virtual machine". Powered by Zig and `libkrun`, Blink leverages hardware-level virtualization, built for high-frequency, short-lived, and strongly isolated AI tasks.

## Key Features

1. **Ambient Environment (Zero-Copy Pass-through)**
   Instead of pulling heavy Docker images, Blink mounts the host's existing runtime (e.g., Python, Bun, Node.js) directly into a read-only ephemeral rootfs via `virtio-fs` DAX mode. An agent's script can run in under 50ms with zero build steps.
   
2. **V-Hub (Vsock Dispatcher)**
   Blink abandons traditional L3/L4 network stacks. All communication between the host and agents (or agent to agent) happens over kernel-bypass `virtio-vsock`. The V-Hub acts as a non-blocking asynchronous message gateway.

3. **Structured RPC Protocol**
   Communication utilizes a strictly packed 20-byte C ABI binary header for zero-overhead routing, preventing sticky packets and enabling high-concurrency request/response matching.

4. **Blink-Init (Guest PID 1)**
   A statically compiled, micro-sized musl-Linux `init` process that manages the guest environment. It reaps orphaned zombie processes, ensuring the sandbox remains clean and stable even when agents spawn complex subprocesses.

## Project Structure

- `src/blink.zig`: Core KVM instance manager mapping `virtio-fs` and preparing the Hot-zone (`/tmp`).
- `src/libkrun.zig`: Zero-overhead Zig C ABI bindings for the underlying `libkrun` hypervisor engine.
- `src/path_translator.zig`: Automatically discovers the host Python environment (`sys.prefix`) and dynamically constructs `virtio-fs` mapping strings.
- `src/vsock.zig` & `src/protocol.zig`: The V-Hub message gateway and 20-byte strict RPC packet parser.
- `src/init.zig`: The minimal Linux-musl static binary acting as PID 1 inside the KVM boundary.
- `src/guest_agent.py` & `src/blink_sdk.py`: Example guest payload and draft Python Host SDK.
- `tests/`: Contains test scripts for `path_translator`, `vsock`, and `array_list` validation.
- `docs/`: Dedicated directory for deep technical specifications and architecture diagrams.

## Building

This project requires **Zig 0.15** and `libkrun` headers/libs.

```bash
zig build
```
This outputs:
1. `blink-core` static library.
2. `blink-cli` executable.
3. `blink-init` statically cross-compiled `aarch64-linux-musl` or `x86_64-linux-musl` binary for the sandbox guest.
