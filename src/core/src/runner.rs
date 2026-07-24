use std::path::Path;

use anyhow::Context;
use boxlite::BoxliteRuntime;
use boxlite::runtime::options::{BoxOptions, RootfsSpec};
use tracing::info;

use crate::boxlite_options::load_boxlite_options;
use crate::context::SandboxResources;
use crate::exec::exec_agent_script;
use crate::network::resolve_network_spec;
use crate::AgentResult;
use blink_shared::DEFAULT_ROOTFS_IMAGE;

/// Execute a guest agent binary inside an ephemeral BoxLite sandbox.
pub async fn run_agent_script(
    script_path: &Path,
    image: &str,
    resources: SandboxResources,
) -> anyhow::Result<AgentResult> {
    resources.validate()?;
    let script_path = script_path
        .canonicalize()
        .with_context(|| format!("agent binary not found: {}", script_path.display()))?;

    info!(
        script = %script_path.display(),
        image,
        cpus = ?resources.cpus,
        memory_mib = ?resources.memory_mib,
        disk_size_gb = ?resources.disk_size_gb,
        "ephemeral run"
    );

    let runtime = BoxliteRuntime::new(load_boxlite_options()?)
        .context("failed to initialize BoxLite runtime")?;

    let network = resolve_network_spec(None).context("resolve ephemeral network")?;

    let options = BoxOptions {
        rootfs: RootfsSpec::Image(image.to_string()),
        network,
        auto_remove: true,
        detach: false,
        cpus: resources.cpus,
        memory_mib: resources.memory_mib,
        disk_size_gb: resources.disk_size_gb,
        ..Default::default()
    };

    let litebox = runtime.create(options, None).await.context("create box")?;
    let result = exec_agent_script(&litebox, &script_path).await;
    let _ = litebox.stop().await;
    result
}

/// Execute a guest agent binary in an ephemeral sandbox (default rootfs image).
pub async fn run_agent_script_default(script_path: &Path) -> anyhow::Result<AgentResult> {
    run_agent_script(script_path, DEFAULT_ROOTFS_IMAGE, SandboxResources::default()).await
}
