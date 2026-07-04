//! Shared execution broker + registry for durable reattach.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use blink_sdk::{Execution, start_exec_pump};
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

const DEFAULT_ATTACH_BUFFER_BYTES: usize = 1024 * 1024;
const DEFAULT_ATTACH_LINGER_SECS: u64 = 300;
const BROADCAST_CAPACITY: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferedFrame {
    pub seq: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameSnapshot {
    pub first_seq: u64,
    pub head_seq: u64,
    pub frames: Vec<BufferedFrame>,
    pub exit_code: Option<i32>,
}

struct ExecSessionState {
    buffer: VecDeque<BufferedFrame>,
    buffered_bytes: usize,
    first_seq: u64,
    head_seq: u64,
    exit_code: Option<i32>,
    finished_at: Option<DateTime<Utc>>,
    closed: bool,
}

impl ExecSessionState {
    fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            buffered_bytes: 0,
            first_seq: 0,
            head_seq: 0,
            exit_code: None,
            finished_at: None,
            closed: false,
        }
    }
}

pub struct ExecSessionCore {
    state: Mutex<ExecSessionState>,
    broadcast_tx: broadcast::Sender<BufferedFrame>,
    pending_exit_tx: Mutex<Option<oneshot::Sender<i32>>>,
    pending_exit_rx: Mutex<Option<oneshot::Receiver<i32>>>,
    exit_tx: watch::Sender<Option<i32>>,
    buffer_limit_bytes: usize,
}

impl ExecSessionCore {
    pub fn new(buffer_limit_bytes: usize) -> Arc<Self> {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (pending_exit_tx, pending_exit_rx) = oneshot::channel();
        let (exit_tx, _) = watch::channel(None);
        Arc::new(Self {
            state: Mutex::new(ExecSessionState::new()),
            broadcast_tx,
            pending_exit_tx: Mutex::new(Some(pending_exit_tx)),
            pending_exit_rx: Mutex::new(Some(pending_exit_rx)),
            exit_tx,
            buffer_limit_bytes: buffer_limit_bytes.max(1),
        })
    }

    pub fn start_output_drain(
        self: Arc<Self>,
        mut output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(bytes) = output_rx.recv().await {
                let frame = self.push_frame(bytes);
                let _ = self.broadcast_tx.send(frame);
            }

            let pending_exit_rx = self
                .pending_exit_rx
                .lock()
                .expect("pending exit lock")
                .take();
            let exit_code = match pending_exit_rx {
                Some(rx) => rx.await.unwrap_or(-1),
                None => -1,
            };
            self.mark_exit(exit_code);
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BufferedFrame> {
        self.broadcast_tx.subscribe()
    }

    pub fn exit_rx(&self) -> watch::Receiver<Option<i32>> {
        self.exit_tx.subscribe()
    }

    pub fn signal_exit_code(&self, code: i32) {
        if let Some(tx) = self
            .pending_exit_tx
            .lock()
            .expect("pending exit lock")
            .take()
        {
            let _ = tx.send(code);
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.state.lock().expect("exec session lock").exit_code
    }

    #[cfg(test)]
    pub fn is_closed(&self) -> bool {
        self.state.lock().expect("exec session lock").closed
    }

    #[cfg(test)]
    pub fn head_seq(&self) -> u64 {
        self.state.lock().expect("exec session lock").head_seq
    }

    #[cfg(test)]
    pub fn first_seq(&self) -> u64 {
        self.state.lock().expect("exec session lock").first_seq
    }

    #[cfg(test)]
    pub fn finished_at(&self) -> Option<DateTime<Utc>> {
        self.state.lock().expect("exec session lock").finished_at
    }

    pub fn snapshot_after(&self, after: u64) -> FrameSnapshot {
        let state = self.state.lock().expect("exec session lock");
        let frames = state
            .buffer
            .iter()
            .filter(|frame| frame.seq > after)
            .cloned()
            .collect::<Vec<_>>();
        FrameSnapshot {
            first_seq: state.first_seq,
            head_seq: state.head_seq,
            frames,
            exit_code: state.exit_code,
        }
    }

    pub fn push_frame(&self, bytes: Vec<u8>) -> BufferedFrame {
        let mut state = self.state.lock().expect("exec session lock");
        let seq = state.head_seq.saturating_add(1);
        let frame = BufferedFrame { seq, bytes };
        let frame_bytes = frame.bytes.len();

        if state.first_seq == 0 {
            state.first_seq = seq;
        }
        state.head_seq = seq;
        state.buffered_bytes += frame_bytes;
        state.buffer.push_back(frame.clone());

        while state.buffered_bytes > self.buffer_limit_bytes && state.buffer.len() > 1 {
            if let Some(evicted) = state.buffer.pop_front() {
                state.buffered_bytes = state.buffered_bytes.saturating_sub(evicted.bytes.len());
            }
        }

        state.first_seq = state.buffer.front().map(|frame| frame.seq).unwrap_or(seq);
        frame
    }

    pub fn mark_exit(&self, code: i32) {
        let mut state = self.state.lock().expect("exec session lock");
        state.exit_code = Some(code);
        state.closed = true;
        state.finished_at = Some(Utc::now());
        let _ = self.exit_tx.send(Some(code));
    }
}

#[derive(Clone)]
pub struct ExecSession {
    id: String,
    session_name: String,
    core: Arc<ExecSessionCore>,
    stdin_tx: Arc<Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>>,
    stdin_closed: Arc<AtomicBool>,
    execution_for_control: Execution,
}

impl ExecSession {
    pub fn start(
        session_name: String,
        execution: Execution,
        tty: bool,
        registry: ExecRegistry,
    ) -> Arc<Self> {
        let id = execution.id().to_string();
        let execution_for_control = execution.clone();
        let pump = start_exec_pump(execution, tty);
        let core = ExecSessionCore::new(attach_buffer_bytes_from_env());
        let stdin_closed = Arc::new(AtomicBool::new(false));

        let session = Arc::new(Self {
            id: id.clone(),
            session_name,
            core: Arc::clone(&core),
            stdin_tx: Arc::new(Mutex::new(Some(pump.stdin_tx))),
            stdin_closed,
            execution_for_control,
        });

        std::mem::drop(Arc::clone(&core).start_output_drain(pump.output_rx));

        let core_for_exit = Arc::clone(&core);
        let registry_for_reap = registry.clone();
        let id_for_reap = id.clone();
        let linger = Duration::from_secs(attach_linger_secs_from_env());
        std::mem::drop(tokio::spawn(async move {
            let exit_code = match pump.done.await {
                Ok(Ok(status)) => status.exit_code,
                Ok(Err(err)) => {
                    tracing::warn!(execution_id = %id_for_reap, error = %err, "execution finished with pump error");
                    -1
                }
                Err(err) => {
                    tracing::warn!(execution_id = %id_for_reap, error = %err, "execution exit watcher dropped");
                    -1
                }
            };
            core_for_exit.signal_exit_code(exit_code);
            tokio::time::sleep(linger).await;
            registry_for_reap.remove(&id_for_reap).await;
        }));

        session
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BufferedFrame> {
        self.core.subscribe()
    }

    pub fn snapshot_after(&self, after: u64) -> FrameSnapshot {
        self.core.snapshot_after(after)
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.core.exit_code()
    }

    pub fn exit_rx(&self) -> watch::Receiver<Option<i32>> {
        self.core.exit_rx()
    }

    pub fn execution_for_control(&self) -> &Execution {
        &self.execution_for_control
    }

    pub fn send_stdin(&self, bytes: Vec<u8>) {
        if self.stdin_closed.load(Ordering::SeqCst) {
            return;
        }
        if let Some(tx) = self.stdin_tx.lock().expect("stdin lock").as_ref() {
            let _ = tx.send(bytes);
        }
    }

    pub fn close_stdin(&self) {
        self.stdin_closed.store(true, Ordering::SeqCst);
        self.stdin_tx.lock().expect("stdin lock").take();
    }
}

#[derive(Clone, Default)]
pub struct ExecRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<String, Arc<ExecSession>>>>,
}

impl ExecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, session: Arc<ExecSession>) -> String {
        let id = session.id().to_string();
        self.inner.lock().await.insert(id.clone(), session);
        id
    }

    pub async fn get(&self, id: &str) -> Option<Arc<ExecSession>> {
        self.inner.lock().await.get(id).cloned()
    }

    pub async fn remove(&self, id: &str) -> Option<Arc<ExecSession>> {
        self.inner.lock().await.remove(id)
    }
}

fn attach_buffer_bytes_from_env() -> usize {
    env::var("BLINK_ATTACH_BUFFER_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ATTACH_BUFFER_BYTES)
}

fn attach_linger_secs_from_env() -> u64 {
    env::var("BLINK_ATTACH_LINGER_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ATTACH_LINGER_SECS)
}

pub(crate) fn encode_frame(frame: &BufferedFrame, seq_framing: bool) -> Vec<u8> {
    if !seq_framing {
        return frame.bytes.clone();
    }
    let mut out = Vec::with_capacity(frame.bytes.len() + 9);
    out.push(frame.bytes.first().copied().unwrap_or(0));
    out.extend_from_slice(&frame.seq.to_be_bytes());
    if frame.bytes.len() > 1 {
        out.extend_from_slice(&frame.bytes[1..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn frame(bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }

    #[tokio::test]
    async fn assigns_monotonic_seqs_and_evicts_by_total_bytes() {
        let core = ExecSessionCore::new(5);
        assert_eq!(core.push_frame(frame(&[0x01, b'a'])).seq, 1);
        assert_eq!(core.push_frame(frame(&[0x01, b'b'])).seq, 2);
        assert_eq!(core.push_frame(frame(&[0x01, b'c'])).seq, 3);
        assert_eq!(core.head_seq(), 3);
        assert_eq!(core.first_seq(), 2);

        let snapshot = core.snapshot_after(0);
        assert_eq!(snapshot.first_seq, 2);
        assert_eq!(snapshot.head_seq, 3);
        assert_eq!(snapshot.frames.len(), 2);
        assert_eq!(snapshot.frames[0].seq, 2);
        assert_eq!(snapshot.frames[1].seq, 3);
    }

    #[tokio::test]
    async fn broadcast_subscriber_receives_new_frames_once_after_snapshot_boundary() {
        let core = ExecSessionCore::new(1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let _join = Arc::clone(&core).start_output_drain(rx);

        tx.send(frame(&[0x01, b'a'])).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while core.head_seq() < 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let mut live = core.subscribe();
        tx.send(frame(&[0x01, b'b'])).unwrap();
        core.signal_exit_code(0);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(1), async {
            while core.head_seq() < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let snapshot = core.snapshot_after(1);
        assert_eq!(snapshot.frames.len(), 1);
        assert_eq!(snapshot.frames[0].seq, 2);

        let received = tokio::time::timeout(Duration::from_secs(1), live.recv())
            .await
            .expect("live frame")
            .expect("broadcast frame");
        assert_eq!(received.seq, 2);
    }

    #[test]
    fn seq_framing_inserts_cursor_after_channel_byte() {
        let frame = BufferedFrame {
            seq: 0x0102_0304_0506_0708,
            bytes: vec![0x01, b'h', b'i'],
        };

        let encoded = crate::exec_registry::encode_frame(&frame, true);
        assert_eq!(encoded[0], 0x01);
        assert_eq!(&encoded[1..9], &frame.seq.to_be_bytes());
        assert_eq!(&encoded[9..], b"hi");
    }

    #[tokio::test]
    async fn records_exit_code_and_replays_it_to_late_attachments() {
        let core = ExecSessionCore::new(1024);
        assert_eq!(core.exit_code(), None);
        core.mark_exit(7);
        assert_eq!(core.exit_code(), Some(7));
        assert!(core.is_closed());
        assert!(core.finished_at().is_some());
        let snapshot = core.snapshot_after(0);
        assert_eq!(snapshot.exit_code, Some(7));
    }

    #[tokio::test]
    async fn snapshot_after_recovers_tail_after_cursor_even_with_eviction() {
        let core = ExecSessionCore::new(5);
        for idx in 1..=5 {
            core.push_frame(frame(&[0x01, b'0' + idx as u8]));
        }

        let snapshot = core.snapshot_after(3);
        assert_eq!(snapshot.first_seq, 4);
        assert_eq!(snapshot.head_seq, 5);
        assert_eq!(snapshot.frames.len(), 2);
        assert_eq!(
            snapshot.frames.iter().map(|f| f.seq).collect::<Vec<_>>(),
            vec![4, 5]
        );
    }

    #[tokio::test]
    async fn drains_remaining_output_before_recording_exit() {
        let core = ExecSessionCore::new(1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let join = Arc::clone(&core).start_output_drain(rx);

        tx.send(frame(&[0x01, b'a'])).unwrap();
        tx.send(frame(&[0x01, b'b'])).unwrap();
        core.signal_exit_code(9);
        drop(tx);

        join.await.expect("drain task");

        let snapshot = core.snapshot_after(0);
        assert_eq!(snapshot.frames.len(), 2);
        assert_eq!(snapshot.frames[0].seq, 1);
        assert_eq!(snapshot.frames[1].seq, 2);
        assert_eq!(snapshot.exit_code, Some(9));
        assert_eq!(core.exit_code(), Some(9));
        assert!(core.is_closed());
    }
}
