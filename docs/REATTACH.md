# Durable Reattach

> Let a control plane detach from a running execution and **re-attach** later without
> killing the process and without losing the output produced while detached.

## Motivation

Today an execution's WebSocket attach is **single-use and passthrough-only**:

- `attach_exec()` calls `execs.take(&exec_id)`, so the registry entry is consumed on the
  first attach — the same `execution_id` can never be attached again
  (`src/server/src/api/exec.rs`, `src/server/src/exec_registry.rs`).
- The I/O pump is created *inside* the attach handler (`start_exec_pump` in
  `run_ws_attach`), so output is only drained while a client is connected. With no
  attach, or between attaches, stdout is dropped — there is no buffer and no cursor
  (`src/core/src/pty.rs`).

XEnsemble's durable-sessions design (see `XEnsemble/docs/DurableSessions.md`) needs the
opposite: the control plane may restart and reconnect to a still-running agent, and must
be able to backfill the bytes it missed. This document specifies the blink-server change
that makes that possible.

## Goals

1. **Re-attachable execution** — attach can happen many times (sequentially; concurrent
   allowed), keyed by a stable `execution_id`.
2. **Server-side output buffer + cursor** — every output frame gets a monotonic `seq`;
   a bounded buffer retains recent frames so a re-attach with `?after=<seq>` backfills the
   gap. The buffer only needs to cover the reconnect window, not the whole session
   (XEnsemble keeps the durable transcript).
3. **Detach never kills the process** — this already holds (the VM process is
   independent); it becomes explicit: the pump/broker lives with the execution record,
   not with any single socket.

## Non-goals (this iteration)

- **Surviving a `blink-server` restart.** The boxlite `Execution` handle and its stdout
  stream are in-memory and consumed once; re-opening a running exec's streams after a
  server restart needs boxlite-level support that isn't exposed today. Tracked as a
  follow-up (§Future). The buffer is in-memory and is lost if `blink-server` itself
  restarts.
- Multiplexing distinct stdin writers intelligently. Concurrent attaches may interleave
  stdin; the expected usage is one active writer at a time (sequential reconnect).

## Design

### Broker per execution

Introduce an **`ExecSession` broker** that owns one execution end-to-end and is started
**at spawn time** (not attach time):

```
spawn_exec  ──►  ExecSession::start(pump, execution_for_control, tty)
                    │  pump = start_exec_pump(execution, tty)   // once
                    │  spawn broker task:
                    │     for frame in pump.output_rx:
                    │        seq += 1
                    │        buffer.push(seq, frame)            // bounded ring
                    │        broadcast.send((seq, frame))       // live tail
                    │     on pump.done: record exit_code, mark closed, close broadcast
                    │     schedule reap after LINGER
                    └─ registry.insert(id, Arc<ExecSession>)
```

- The broker drains `output_rx` **continuously**, independent of any client, so output is
  captured from the instant the process starts.
- `seq` is a server-assigned, per-execution monotonic counter starting at `1`.
- The buffer is a bounded ring (evict oldest by total bytes, default
  `BLINK_ATTACH_BUFFER_BYTES = 1 MiB`), tracking `first_seq` (oldest retained) and
  `head_seq` (newest). Requests for an `after` older than `first_seq` are served what
  remains and flagged (client already has the rest in its own transcript).
- Live subscribers use a `tokio::sync::broadcast` channel.
- `execution_for_control` is a clone of the `Execution` kept for `resize`/`signal`
  control messages (mirrors today's `execution_for_control`).
- Exit result (`exit_code`) is stored in the broker state; a terminal marker is also kept
  so a client attaching *after* exit still learns the code.

### Registry

`ExecRegistry` changes from *take-once* to *shared*:

- `HashMap<String, Arc<ExecSession>>`.
- `insert(exec) -> id` (unchanged shape).
- `get(&id) -> Option<Arc<ExecSession>>` — clone the Arc; **does not remove**.
- `remove(&id)` — used by the reaper.

**Reaping:** when the broker's `done` fires, record the finish time and, after
`BLINK_ATTACH_LINGER_SECS` (default `300`), remove the record. This bounds memory while
giving a reconnecting control plane a window to fetch the final frames + exit code.

### Attach protocol (backward compatible)

`GET /api/sessions/{name}/executions/{exec_id}/attach?after=<seq>&seq=1`

Query params (both optional):

| Param | Meaning |
|-------|---------|
| `after` | Resume: only deliver frames with `seq > after`. Omitted ⇒ replay whole buffer. |
| `seq=1` | Enable **seq framing** on binary output frames (see below). Implied when `after` is present. |

Handler flow (`run_ws_attach`):

1. `execs.get(exec_id)` (Arc clone), verify `session_name`.
2. `subscribe()` to the broadcast **before** snapshotting the buffer (no gap).
3. Snapshot buffered frames with `seq > after`; send them; remember `last_sent_seq`.
4. If exit is already recorded, send the `exit` frame and close.
5. Otherwise loop with `tokio::select!`:
   - broadcast frame → skip `seq <= last_sent_seq` (dedupe snapshot/live overlap), else send.
   - ws binary → `broker.stdin_tx.send(bytes)`.
   - ws text → `stdin_eof` / `handle_control_message(&broker.execution_for_control, …)`.
   - broker exit signal → send `exit` frame, close.

**Binary frame format:**

- Legacy (no `seq` framing): `[channel][payload…]` — `channel` = `0x01` stdout / `0x02`
  stderr. Unchanged; existing clients keep working.
- Seq framing (`seq=1`): `[channel][seq: u64 big-endian][payload…]`. The client reads the
  8-byte seq after the channel byte and records it as its resume cursor.

Text control frames (`exit`, `error`) are unchanged.

### Testability

The broker's buffer/seq/replay logic is decoupled from a real VM: it consumes an
`mpsc::UnboundedReceiver<Vec<u8>>` (the pump's `output_rx`). Unit tests feed synthetic
frames and assert:

- monotonic `seq` assignment,
- ring eviction advances `first_seq`,
- a fresh subscriber with `after = N` receives exactly frames `> N` once (no dupes, no
  gaps) across the snapshot→live boundary,
- exit code is recorded and delivered to a post-exit attach.

## XEnsemble side (contract)

- On (re)connect, XEnsemble attaches with `?seq=1&after=<last_seq_it_has>`; it treats the
  8-byte seq as the cursor to persist alongside its transcript.
- Detach = just close the socket. The execution keeps running; reconnect later.
- Within `LINGER` after exit, a reconnect still yields the tail + exit code.

## Future

- **Server-restart durability (non-goal above):** requires boxlite to re-open a running
  execution's stdio after process restart, plus persisting the buffer (e.g. to the
  session disk under `/var/blink/`). Design once boxlite exposes exec-stream reconnect.
- **Idle hibernate/wake** of the session VM (separate from attach lifecycle).
