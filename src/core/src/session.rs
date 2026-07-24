use std::path::{Path, PathBuf};

use anyhow::Result;
use boxlite::SnapshotInfo;

pub use crate::context::{BlinkContext, OpenSessionOptions, SessionInfo};

/// Open (create or reuse) a named persistent agent session.
pub async fn open_session(name: &str, image: &str) -> Result<(String, bool)> {
    open_session_with(name, image, false, OpenSessionOptions::default()).await
}

pub async fn open_warm_session(name: &str, image: &str) -> Result<(String, bool)> {
    open_session_with(name, image, true, OpenSessionOptions::default()).await
}

pub async fn open_session_with(
    name: &str,
    image: &str,
    warm: bool,
    options: OpenSessionOptions,
) -> Result<(String, bool)> {
    BlinkContext::new()?
        .open_session(name, image, warm, options)
        .await
}

pub async fn run_in_session(name: &str, script_path: &Path) -> Result<crate::AgentResult> {
    BlinkContext::new()?.run_in_session(name, script_path).await
}

pub async fn spawn_in_session(name: &str, spec: crate::SpawnSpec) -> Result<crate::Execution> {
    BlinkContext::new()?.spawn_in_session(name, spec).await
}

pub async fn checkpoint_session(name: &str, snapshot: &str) -> Result<SnapshotInfo> {
    BlinkContext::new()?.checkpoint_session(name, snapshot).await
}

pub async fn restore_session(name: &str, snapshot: &str) -> Result<()> {
    BlinkContext::new()?.restore_session(name, snapshot).await
}

pub async fn list_checkpoints(name: &str) -> Result<Vec<SnapshotInfo>> {
    BlinkContext::new()?.list_checkpoints(name).await
}

pub async fn list_sessions() -> Result<Vec<SessionInfo>> {
    BlinkContext::new()?.list_sessions().await
}

pub async fn stop_session(name: &str) -> Result<()> {
    BlinkContext::new()?.stop_session(name).await
}

pub async fn remove_session(name: &str, force: bool) -> Result<()> {
    BlinkContext::new()?.remove_session(name, force).await
}

pub async fn export_session(name: &str) -> Result<PathBuf> {
    BlinkContext::new()?.export_session(name).await
}

pub async fn import_session(archive_path: &Path, name: Option<&str>) -> Result<String> {
    BlinkContext::new()?.import_session(archive_path, name).await
}
