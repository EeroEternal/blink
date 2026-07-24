use std::path::Path;
use std::sync::Arc;

use blink_sdk::{BlinkContext, OpenSessionOptions, SandboxResources};
use blink_shared::DEFAULT_ROOTFS_IMAGE;

use crate::exec_registry::ExecRegistry;
use crate::jobs::{JobRecord, JobStore};

#[derive(Clone)]
pub struct AppState {
    pub ctx: Arc<BlinkContext>,
    pub jobs: JobStore,
    pub execs: ExecRegistry,
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            ctx: Arc::new(BlinkContext::new()?),
            jobs: JobStore::default(),
            execs: ExecRegistry::new(),
        })
    }

    pub fn spawn_run(
        &self,
        tier: blink_sdk::SandboxTier,
        script_path: String,
        session_name: Option<String>,
        image: Option<String>,
        warm: bool,
        resources: SandboxResources,
    ) -> JobRecord {
        let job = self
            .jobs
            .create(tier, script_path.clone(), session_name.clone());
        let jobs = self.jobs.clone();
        let ctx = Arc::clone(&self.ctx);
        let job_id = job.id.clone();

        tokio::spawn(async move {
            jobs.set_running(&job_id);
            let path = Path::new(&script_path);
            let result = async {
                match tier {
                    blink_sdk::SandboxTier::Ephemeral => {
                        ctx.run_agent_ephemeral(path, image.as_deref(), resources)
                            .await
                    }
                    blink_sdk::SandboxTier::Session => {
                        let name = session_name.as_deref().unwrap();
                        let image = image.as_deref().unwrap_or(DEFAULT_ROOTFS_IMAGE);
                        let options = OpenSessionOptions {
                            resources,
                            ..Default::default()
                        };
                        ctx.open_session(name, image, warm, options).await?;
                        ctx.run_in_session(name, path).await
                    }
                }
            }
            .await;
            match result {
                Ok(agent) => jobs.set_succeeded(&job_id, agent),
                Err(err) => jobs.set_failed(&job_id, err.to_string()),
            }
        });

        job
    }
}
