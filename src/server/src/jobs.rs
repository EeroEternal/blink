use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use blink_sdk::{AgentResult, SandboxTier};
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobRecord {
    pub id: String,
    pub status: JobStatus,
    pub tier: SandboxTier,
    pub session_name: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub result: Option<AgentResult>,
    pub error: Option<String>,
}

#[derive(Default, Clone)]
pub struct JobStore {
    inner: Arc<RwLock<HashMap<String, JobRecord>>>,
}

impl JobStore {
    pub fn create(
        &self,
        tier: SandboxTier,
        _script: String,
        session_name: Option<String>,
    ) -> JobRecord {
        let job = JobRecord {
            id: Uuid::new_v4().to_string(),
            status: JobStatus::Queued,
            tier,
            session_name,
            created_at: Utc::now().to_rfc3339(),
            started_at: None,
            finished_at: None,
            result: None,
            error: None,
        };
        self.inner
            .write()
            .expect("job store lock")
            .insert(job.id.clone(), job.clone());
        job
    }

    pub fn get(&self, id: &str) -> Option<JobRecord> {
        self.inner.read().expect("job store lock").get(id).cloned()
    }

    pub fn set_running(&self, id: &str) {
        self.update(id, |job| {
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now().to_rfc3339());
        });
    }

    pub fn set_succeeded(&self, id: &str, result: AgentResult) {
        self.update(id, |job| {
            job.status = JobStatus::Succeeded;
            job.finished_at = Some(Utc::now().to_rfc3339());
            job.result = Some(result);
        });
    }

    pub fn set_failed(&self, id: &str, error: String) {
        self.update(id, |job| {
            job.status = JobStatus::Failed;
            job.finished_at = Some(Utc::now().to_rfc3339());
            job.error = Some(error);
        });
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut JobRecord)) {
        if let Some(job) = self.inner.write().expect("job store lock").get_mut(id) {
            f(job);
        }
    }
}
