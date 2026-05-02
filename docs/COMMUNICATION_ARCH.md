# Blink V-Hub Dual-Track Architecture

Blink implements a high-performance, asynchronous communication bus between the Host and the Guest Sandbox using `virtio-vsock`.

## 1. Dual-Track Communication

The communication is logically split into two planes, both multiplexed over the same Vsock connection but distinguished by the `msg_type` in the 20-byte Vsock Header.

### A. Data Plane (Real-Time Observability)
- **Message Types:** `.Stdout` (0x30), `.Stderr` (0x31).
- **Relay:** `blink-init` (PID 1) captures the guest process's standard streams via Linux pipes.
- **Behavior:** Captured data is immediately wrapped in a Vsock packet and sent to the Host.
- **Host Handling:** The V-Hub Dispatcher prints these payloads to the Host's `stderr`. This allows developers to see real-time progress bars, colors (VT100), and debug logs without polluting the final machine-readable result.

### B. Control Plane (Structured Result Reporting)
- **Message Types:** `.RpcRequest` (0x10), `.RpcResponse` (0x11).
- **Logic:** The Agent SDK (or script) explicitly sends a JSON-encoded payload upon completion.
- **Host Handling:** The V-Hub Dispatcher prints the raw JSON to the Host's `stdout` and terminates the `blink-cli` process with exit code 0.
- **Purpose:** Enables high-reliability integration with external AI orchestrators (e.g., Python `subprocess.run`).

## 2. Packet Life-cycle
1. **Connect:** `blink-init` connects to Host CID 2, Port 10000.
2. **Execute:** `blink-init` forks and executes the Agent payload.
3. **Stream:** Any `print()` from the agent is caught by `blink-init` and relayed as `.Stdout`.
4. **Report:** The Agent SDK sends a final `.RpcRequest` JSON.
5. **Exit:** Host acknowledges the RPC and shuts down; `blink-init` detects EOF and halts the VM.
