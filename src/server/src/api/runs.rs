use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use blink_sdk::SandboxTier;
use serde::{Deserialize, Serialize};

use crate::api::error::{valid_session_name, ApiError};
use crate::jobs::JobRecord;
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/runs", post(submit_run))
        .route("/runs/{id}", get(get_run))
}

#[derive(Deserialize)]
struct SubmitRunRequest {
    /// Host path to the agent binary or script to copy into the sandbox.
    script: String,
    #[serde(default)]
    tier: TierName,
    session_name: Option<String>,
    image: Option<String>,
    #[serde(default)]
    warm: bool,
    /// VM resource limits for new sandboxes (ignored when reusing an existing session).
    #[serde(default)]
    resources: blink_sdk::SandboxResources,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TierName {
    #[default]
    Ephemeral,
    Session,
}

#[derive(Serialize)]
struct SubmitRunResponse {
    job: JobRecord,
    message: &'static str,
}

async fn submit_run(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmitRunRequest>,
) -> Result<Json<SubmitRunResponse>, ApiError> {
    let script = body.script.trim();
    if script.is_empty() {
        return Err(ApiError::bad_request("script must not be empty"));
    }
    if !std::path::Path::new(script).exists() {
        return Err(ApiError::bad_request("script not found on host"));
    }

    let tier = match body.tier {
        TierName::Ephemeral => SandboxTier::Ephemeral,
        TierName::Session => {
            let name = body
                .session_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::bad_request("session_name required"))?;
            if !valid_session_name(name) {
                return Err(ApiError::bad_request("invalid session_name"));
            }
            SandboxTier::Session
        }
    };

    let job = state.spawn_run(
        tier,
        script.to_string(),
        body.session_name,
        body.image,
        body.warm,
        body.resources,
    );

    Ok(Json(SubmitRunResponse {
        job,
        message: "queued",
    }))
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<JobRecord>, ApiError> {
    state
        .jobs
        .get(&id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("run not found"))
}
