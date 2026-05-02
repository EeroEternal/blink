# Blink Output Streaming Architecture (Dual-Track)

Blink prioritizes **Machine-Readable (Structured)** output for AI Agents over **Human-Readable (Interactive)** output, while still supporting the latter via an opt-in PTY mode.

## 1. Primary Priority: Structured JSON (RPC)

For AI Agent workloads, parsing raw terminal output (including ANSI escape codes and progress bars) is error-prone and inefficient. The primary streaming mechanism strictly enforces structured JSON reporting.

### Mechanism
- The guest Agent does not rely on a PTY.
- The Agent SDK intercepts standard `stdout` and `stderr` streams at the language level (e.g., Python's `contextlib.redirect_stdout`).
- Execution results, errors, and metadata (like memory usage or timestamps) are packaged into a JSON object.
- The JSON object is transmitted back to the Host via the V-Hub using the `RpcRequest` (0x10) `MessageType` and the 20-byte Vsock Header.

### Advantages
- **Deterministic:** LLM dispatchers can directly read `.exit_code` or `.stderr` without regex parsing.
- **Metadata-Rich:** Logs can include context like Agent IDs, timestamps, or resource consumption limits.
- **Bandwidth Efficient:** No control characters or rendering artifacts are transmitted.

## 2. Secondary Priority: Raw PTY Streaming (Interactive)

For human developers debugging scripts or running TUI applications (like `vim` or `top`), structured JSON is visually unappealing. Blink provides a fallback PTY mode.

### Mechanism
- When initiated with an `--interactive` flag, `blink-init` requests a Pseudo-Terminal (PTY) via `posix.openpt()`.
- The Agent payload is executed with its file descriptors attached to this PTY.
- `blink-init` polls the PTY master file descriptor and forwards raw byte streams (including colors and cursor movements) to the V-Hub using the `StreamData` (0x20) `MessageType`.
- The Host writes these bytes directly to its own terminal `stdout`.

### Advantages
- **Immersive Debugging:** Retains color tracebacks, progress bars (`tqdm`), and interactive stdin capabilities.
