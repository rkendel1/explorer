//! Runtime commands for mock server control

use serde::Deserialize;

use crate::services::RuntimeService;
use crate::services::runtime_service::{
    RuntimeConfig, RuntimeEventInfo, RuntimeMetricsInfo, RuntimeStateSnapshot, RuntimeStatusInfo,
};

use super::{AppState, CommandResult, from_service, state_handle};

/// Start runtime request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRuntimeRequest {
    pub port: Option<u16>,
    pub profile_id: Option<String>,
}

impl From<StartRuntimeRequest> for RuntimeConfig {
    fn from(request: StartRuntimeRequest) -> Self {
        Self {
            port: request.port.unwrap_or(4010),
            profile_id: request.profile_id,
            auto_start: false,
        }
    }
}

/// Get runtime events request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetRuntimeEventsRequest {
    pub filter: Option<String>,
    pub limit: Option<usize>,
}

/// Import runtime state request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRuntimeStateRequest {
    pub state: RuntimeStateSnapshot,
}

/// Get runtime status
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_status(state: AppState<'_>) -> CommandResult<RuntimeStatusInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::get_status(&state).await)
}

/// Start the mock runtime
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_start(
    state: AppState<'_>,
    request: StartRuntimeRequest,
) -> CommandResult<RuntimeStatusInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::start(&state, request.into()).await)
}

/// Stop the mock runtime
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_stop(state: AppState<'_>) -> CommandResult<RuntimeStatusInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::stop(&state).await)
}

/// Restart the mock runtime
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_restart(
    state: AppState<'_>,
    request: StartRuntimeRequest,
) -> CommandResult<RuntimeStatusInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::restart(&state, request.into()).await)
}

/// Reset runtime state (clear all mock state)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_reset(state: AppState<'_>) -> CommandResult<RuntimeStatusInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::reset_state(&state).await)
}

/// Get runtime events
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_events(
    state: AppState<'_>,
    request: GetRuntimeEventsRequest,
) -> CommandResult<Vec<RuntimeEventInfo>> {
    let state = state_handle(&state);
    from_service(RuntimeService::get_events(&state, request.limit.unwrap_or(100)).await)
}

/// Get runtime metrics
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_metrics(state: AppState<'_>) -> CommandResult<RuntimeMetricsInfo> {
    let state = state_handle(&state);
    from_service(RuntimeService::get_metrics(&state).await)
}

/// Export runtime state
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_export_state(state: AppState<'_>) -> CommandResult<RuntimeStateSnapshot> {
    let state = state_handle(&state);
    from_service(RuntimeService::export_state(&state).await)
}

/// Import runtime state
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn runtime_import_state(
    state: AppState<'_>,
    request: ImportRuntimeStateRequest,
) -> CommandResult<()> {
    let state = state_handle(&state);
    from_service(RuntimeService::import_state(&state, request.state).await)
}
