//! Tracks in-flight sandbox executions awaiting WebSocket attach.

use std::collections::HashMap;
use std::sync::Arc;

use blink_sdk::Execution;
use tokio::sync::Mutex;

pub struct PendingExec {
    pub session_name: String,
    pub execution: Execution,
    pub tty: bool,
}

#[derive(Clone, Default)]
pub struct ExecRegistry {
    inner: Arc<Mutex<HashMap<String, PendingExec>>>,
}

impl ExecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, exec: PendingExec) -> String {
        let id = exec.execution.id().to_string();
        self.inner.lock().await.insert(id.clone(), exec);
        id
    }

    pub async fn take(&self, id: &str) -> Option<PendingExec> {
        self.inner.lock().await.remove(id)
    }
}
