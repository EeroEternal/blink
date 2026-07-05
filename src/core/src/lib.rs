//! Blink core runtime built on BoxLite VM infrastructure.

mod context;
mod exec;
mod pty;
mod runner;
mod session;
mod tier;
mod vhub;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct AgentResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_keys: Option<Vec<String>>,
}

pub use boxlite::{ExecResult, Execution};
pub use context::{BlinkContext, SessionInfo, SessionVolume};
pub use runner::{run_agent_script, run_agent_script_default};
pub use session::{
    checkpoint_session, export_session, import_session, list_checkpoints, list_sessions,
    open_session, open_warm_session, remove_session, restore_session, run_in_session,
    spawn_in_session, stop_session,
};
pub use pty::{ExecPump, SpawnSpec, handle_control_message, spawn_exec, start_exec_pump};
pub use tier::SandboxTier;
pub use vhub::{VhubSession, serve_vhub};
