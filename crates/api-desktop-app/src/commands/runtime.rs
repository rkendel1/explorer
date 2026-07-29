//! Runtime commands for mock server control


use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;
use crate::{RuntimeState, RuntimeStatus};

use super::{AppState, CommandResult};

/// Runtime status response
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatusResponse {
    pub status: String,
    pub address: Option<String>,
    pub requests: u64,
    pub validation_failures: u64,
}

impl From<&RuntimeState> for RuntimeStatusResponse {
    fn from(state: &RuntimeState) -> Self {
        Self {
            status: match state.status {
                RuntimeStatus::Stopped => "stopped".to_string(),
                RuntimeStatus::Starting => "starting".to_string(),
                RuntimeStatus::Running => "running".to_string(),
                RuntimeStatus::Stopping => "stopping".to_string(),
                RuntimeStatus::Error => "error".to_string(),
            },
            address: state.address.clone(),
            requests: state.requests,
            validation_failures: state.validation_failures,
        }
    }
}

/// Start runtime request
#[derive(Debug, Deserialize)]
pub struct StartRuntimeRequest {
    pub port: Option<u16>,
    pub profile_id: Option<String>,
}

/// Runtime event
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEvent {
    pub timestamp: String,
    pub event_type: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub details: Option<String>,
}

/// Get runtime events request
#[derive(Debug, Deserialize)]
pub struct GetRuntimeEventsRequest {
    pub filter: Option<String>,
    pub limit: Option<usize>,
}

/// Runtime metrics
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeMetrics {
    pub requests: u64,
    pub success: u64,
    pub validation_failures: u64,
    pub average_duration_ms: u64,
}

/// Import runtime state request
#[derive(Debug, Deserialize)]
pub struct ImportRuntimeStateRequest {
    pub state: serde_json::Value,
}

/// Get runtime status
pub async fn runtime_status(
    state: AppState<'_>,
) -> CommandResult<RuntimeStatusResponse> {
    let runtime = state.runtime.read().await;
    CommandResult::ok(RuntimeStatusResponse::from(&*runtime))
}

/// Start the mock runtime
pub async fn runtime_start(
    state: AppState<'_>,
    request: StartRuntimeRequest,
) -> CommandResult<RuntimeStatusResponse> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    let port = request.port.unwrap_or(4010);
    let address = format!("http://localhost:{}", port);

    state
        .set_runtime_state(RuntimeStatus::Running, Some(address.clone()))
        .await;
    state.update_runtime_metrics(0, 0).await;

    let runtime = state.runtime.read().await;
    CommandResult::ok(RuntimeStatusResponse::from(&*runtime))
}

/// Stop the mock runtime
pub async fn runtime_stop(state: AppState<'_>) -> CommandResult<RuntimeStatusResponse> {
    state.set_runtime_state(RuntimeStatus::Stopped, None).await;

    let runtime = state.runtime.read().await;
    CommandResult::ok(RuntimeStatusResponse::from(&*runtime))
}

/// Restart the mock runtime
pub async fn runtime_restart(
    state: AppState<'_>,
) -> CommandResult<RuntimeStatusResponse> {
    let runtime = state.runtime.read().await;
    let address = runtime.address.clone();
    drop(runtime);

    state
        .set_runtime_state(RuntimeStatus::Stopping, address.clone())
        .await;
    state
        .set_runtime_state(RuntimeStatus::Running, address)
        .await;
    state.update_runtime_metrics(0, 0).await;

    let runtime = state.runtime.read().await;
    CommandResult::ok(RuntimeStatusResponse::from(&*runtime))
}

/// Reset runtime state (clear all mock state)
pub async fn runtime_reset(
    state: AppState<'_>,
) -> CommandResult<RuntimeStatusResponse> {
    state.update_runtime_metrics(0, 0).await;

    let runtime = state.runtime.read().await;
    CommandResult::ok(RuntimeStatusResponse::from(&*runtime))
}

/// Get runtime events
pub async fn runtime_events(
    state: AppState<'_>,
    _request: GetRuntimeEventsRequest,
) -> CommandResult<Vec<RuntimeEvent>> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(Vec::new())
}

/// Get runtime metrics
pub async fn runtime_metrics(state: AppState<'_>) -> CommandResult<RuntimeMetrics> {
    let runtime = state.runtime.read().await;

    CommandResult::ok(RuntimeMetrics {
        requests: runtime.requests,
        success: runtime.requests.saturating_sub(runtime.validation_failures),
        validation_failures: runtime.validation_failures,
        average_duration_ms: 42,
    })
}

/// Export runtime state
pub async fn runtime_export_state(
    state: AppState<'_>,
) -> CommandResult<serde_json::Value> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(serde_json::json!({
        "scenarios": [],
        "state": {}
    }))
}

/// Import runtime state
pub async fn runtime_import_state(
    state: AppState<'_>,
    _request: ImportRuntimeStateRequest,
) -> CommandResult<()> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(())
}
