//! Shared BoxLite runtime for long-lived server processes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use boxlite::{
    BoxCommand, BoxliteRuntime, LiteBox, SnapshotInfo,
    runtime::options::{
        BoxArchive, BoxOptions, ExportOptions, NetworkSpec, RootfsSpec,
        SnapshotOptions, VolumeSpec,
    },
};
use tracing::{info, warn};

use crate::boxlite_options::load_boxlite_options;
use crate::exec::exec_agent_script;
use crate::network::resolve_network_spec;
use crate::runner::run_agent_script;
use crate::AgentResult;
use blink_shared::AGENT_MEMORY_DIR;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct SessionVolume {
    pub host_path: String,
    pub guest_path: String,
    #[serde(default)]
    pub read_only: bool,
}

impl From<SessionVolume> for VolumeSpec {
    fn from(volume: SessionVolume) -> Self {
        Self {
            host_path: volume.host_path,
            guest_path: volume.guest_path,
            read_only: volume.read_only,
        }
    }
}

/// VM resource limits passed through to BoxLite `BoxOptions`.
///
/// Unset fields use BoxLite defaults (1 CPU, 2048 MiB RAM, 10 GB disk).
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct SandboxResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u32>,
    /// Sparse container rootfs virtual size in GB (at least the base image size).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_size_gb: Option<u64>,
}

impl SandboxResources {
    pub fn validate(&self) -> Result<()> {
        if let Some(0) = self.cpus {
            bail!("cpus must be >= 1");
        }
        if let Some(0) = self.memory_mib {
            bail!("memory_mib must be >= 1");
        }
        if let Some(0) = self.disk_size_gb {
            bail!("disk_size_gb must be >= 1");
        }
        Ok(())
    }
}

/// Options for creating or reusing a named session.
#[derive(Clone, Debug, Default)]
pub struct OpenSessionOptions {
    pub volumes: Vec<SessionVolume>,
    pub network: Option<boxlite::runtime::options::NetworkConfig>,
    pub resources: SandboxResources,
}

#[derive(Clone)]
pub struct BlinkContext {
    runtime: BoxliteRuntime,
    export_dir: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionInfo {
    pub name: Option<String>,
    pub box_id: String,
    pub status: String,
    pub running: bool,
}

impl BlinkContext {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("could not resolve home directory")?;
        let export_dir = home.join(".blink").join("exports");
        std::fs::create_dir_all(&export_dir).context("failed to create export directory")?;
        Ok(Self {
            runtime: BoxliteRuntime::new(load_boxlite_options()?)
                .context("failed to initialize BoxLite runtime")?,
            export_dir,
        })
    }

    pub fn export_dir(&self) -> &Path {
        &self.export_dir
    }

    fn session_options(
        image: &str,
        volumes: &[VolumeSpec],
        network: NetworkSpec,
        resources: &SandboxResources,
    ) -> BoxOptions {
        let working_dir = volumes
            .first()
            .map(|volume| volume.guest_path.clone());
        BoxOptions {
            rootfs: RootfsSpec::Image(image.to_string()),
            network,
            auto_remove: false,
            detach: true,
            volumes: volumes.to_vec(),
            working_dir,
            cpus: resources.cpus,
            memory_mib: resources.memory_mib,
            disk_size_gb: resources.disk_size_gb,
            ..Default::default()
        }
    }

    async fn get_box(&self, name: &str) -> Result<LiteBox> {
        self.runtime
            .get(name)
            .await
            .context("failed to lookup session")?
            .with_context(|| format!("session '{name}' not found"))
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let infos = self.runtime.list_info().await.context("list sessions")?;
        Ok(infos
            .into_iter()
            .map(|b| SessionInfo {
                name: b.name,
                box_id: b.id.to_string(),
                status: format!("{:?}", b.status),
                running: b.pid.is_some(),
            })
            .collect())
    }

    pub async fn open_session(
        &self,
        name: &str,
        image: &str,
        warm: bool,
        options: OpenSessionOptions,
    ) -> Result<(String, bool)> {
        options.resources.validate()?;
        info!(
            name,
            image,
            warm,
            volume_count = options.volumes.len(),
            cpus = ?options.resources.cpus,
            memory_mib = ?options.resources.memory_mib,
            disk_size_gb = ?options.resources.disk_size_gb,
            "opening session"
        );
        let network_spec = resolve_network_spec(options.network)?;
        let volume_specs: Vec<VolumeSpec> =
            options.volumes.into_iter().map(VolumeSpec::from).collect();
        let (litebox, created) = self
            .runtime
            .get_or_create(
                Self::session_options(image, &volume_specs, network_spec, &options.resources),
                Some(name.to_string()),
            )
            .await
            .context("open session")?;
        ensure_memory_dir(&litebox).await?;
        Ok((litebox.id().as_str().to_string(), created))
    }

    pub async fn run_agent_ephemeral(
        &self,
        script_path: &Path,
        image: Option<&str>,
        resources: SandboxResources,
    ) -> Result<AgentResult> {
        run_agent_script(
            script_path,
            image.unwrap_or(blink_shared::DEFAULT_ROOTFS_IMAGE),
            resources,
        )
        .await
    }

    pub async fn run_in_session(&self, name: &str, script_path: &Path) -> Result<AgentResult> {
        let litebox = self.get_box(name).await?;
        exec_agent_script(&litebox, script_path).await
    }

    pub async fn spawn_in_session(&self, name: &str, spec: crate::SpawnSpec) -> Result<crate::Execution> {
        let litebox = self.get_box(name).await?;
        crate::spawn_exec(&litebox, spec).await
    }

    pub async fn checkpoint_session(&self, name: &str, snapshot: &str) -> Result<SnapshotInfo> {
        let litebox = self.get_box(name).await?;
        litebox
            .snapshots()
            .create(SnapshotOptions::default(), snapshot)
            .await
            .context("checkpoint")
    }

    pub async fn restore_session(&self, name: &str, snapshot: &str) -> Result<()> {
        let litebox = self.get_box(name).await?;
        if litebox.info().status.is_active() {
            litebox.stop().await.context("stop before restore")?;
        }
        litebox.snapshots().restore(snapshot).await.context("restore")
    }

    pub async fn list_checkpoints(&self, name: &str) -> Result<Vec<SnapshotInfo>> {
        let litebox = self.get_box(name).await?;
        litebox.snapshots().list().await.context("list checkpoints")
    }

    pub async fn stop_session(&self, name: &str) -> Result<()> {
        let litebox = self.get_box(name).await?;
        if litebox.info().status.is_active() {
            litebox.stop().await.context("stop session")?;
        }
        Ok(())
    }

    pub async fn remove_session(&self, name: &str, force: bool) -> Result<()> {
        self.runtime.remove(name, force).await.context("remove session")
    }

    pub async fn export_session(&self, name: &str) -> Result<PathBuf> {
        let litebox = self.get_box(name).await?;
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let dest = self
            .export_dir
            .join(format!("{name}-{stamp}.boxlite"));
        let archive = litebox
            .export(ExportOptions::default(), &dest)
            .await
            .context("export session")?;
        Ok(archive.path().to_path_buf())
    }

    pub async fn import_session(&self, archive_path: &Path, name: Option<&str>) -> Result<String> {
        if !archive_path.exists() {
            bail!("archive not found: {}", archive_path.display());
        }
        let archive = BoxArchive::new(archive_path);
        let litebox = self
            .runtime
            .import_box(archive, name.map(String::from))
            .await
            .context("import session")?;
        ensure_memory_dir(&litebox).await?;
        Ok(litebox.id().as_str().to_string())
    }
}

async fn ensure_memory_dir(litebox: &LiteBox) -> Result<()> {
    let cmd = format!("mkdir -p {AGENT_MEMORY_DIR}");
    for attempt in 0..3 {
        let execution = match litebox
            .exec(BoxCommand::new("sh").arg("-c").arg(&cmd))
            .await
        {
            Ok(e) => e,
            Err(e) => {
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
                return Err(e).context("mkdir memory dir");
            }
        };
        let status = execution.wait().await.context("mkdir wait")?;
        if status.exit_code == 0 {
            return Ok(());
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    warn!(exit_code = "unknown", "mkdir {AGENT_MEMORY_DIR} failed after 3 attempts, agent may need to create it");
    Ok(())
}
