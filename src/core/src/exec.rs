use std::path::Path;

use anyhow::{Context, Result, bail};
use boxlite::{BoxCommand, LiteBox, litebox::copy::CopyOptions};
use futures::StreamExt;

use crate::AgentResult;

pub const AGENT_BINARY_CONTAINER_PATH: &str = "/tmp/blink_agent";

/// Copy a host binary or script into the box, mark it executable, and run it.
pub async fn exec_agent_script(litebox: &LiteBox, script_path: &Path) -> Result<AgentResult> {
    let script_path = script_path
        .canonicalize()
        .with_context(|| format!("agent binary not found: {}", script_path.display()))?;

    litebox
        .copy_into(
            &script_path,
            AGENT_BINARY_CONTAINER_PATH,
            CopyOptions::default(),
        )
        .await
        .context("failed to copy agent binary into sandbox")?;

    let mut execution = litebox
        .exec(
            BoxCommand::new("sh").arg("-c").arg(format!(
                "chmod +x {AGENT_BINARY_CONTAINER_PATH} && exec {AGENT_BINARY_CONTAINER_PATH}"
            )),
        )
        .await
        .context("failed to start agent process in sandbox")?;

    let mut stdout = String::new();
    if let Some(mut stream) = execution.stdout() {
        while let Some(chunk) = stream.next().await {
            stdout.push_str(&chunk);
        }
    }

    let mut stderr = String::new();
    if let Some(mut stream) = execution.stderr() {
        while let Some(chunk) = stream.next().await {
            stderr.push_str(&chunk);
        }
    }

    let status = execution
        .wait()
        .await
        .context("failed waiting for agent process")?;

    if let Some(message) = status.error_message {
        bail!("agent process failed: {message}");
    }

    if let Some(parsed) = parse_execution_payload(&stdout) {
        return Ok(parsed);
    }

    Ok(AgentResult {
        stdout,
        stderr,
        exit_code: status.exit_code,
        memory_keys: None,
    })
}

fn parse_execution_payload(stdout: &str) -> Option<AgentResult> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return agent_result_from_json(&value);
    }

    for line in trimmed.lines().rev() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(result) = agent_result_from_json(&value) {
                return Some(result);
            }
        }
    }

    None
}

fn agent_result_from_json(value: &serde_json::Value) -> Option<AgentResult> {
    if value.get("event").and_then(|v| v.as_str()) != Some("execution_result") {
        return None;
    }

    Some(AgentResult {
        stdout: value
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        stderr: value
            .get("stderr")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        exit_code: value
            .get("exit_code")
            .and_then(|v| v.as_i64())
            .unwrap_or(1) as i32,
        memory_keys: value
            .get("memory_keys")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
    })
}
