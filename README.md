# Blink (v2.0)

Blink is a stateless, micro-runtime designed for AI Agents. It provides a high-performance execution environment for AI tasks, built with Rust.

## Architecture
- **V-Hub (Rust)**: High-performance asynchronous communication gateway using `tokio` and `nix`.
- **Structured RPC**: Binary protocol (20-byte header) for robust host-agent communication.
- **Hypervisor Abstraction**: Pluggable backends (QEMU, Container/Namespace) via the `Hypervisor` trait.

## Building
Requires Rust 1.70+.
```bash
cargo build
```
