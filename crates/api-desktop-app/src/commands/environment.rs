//! Environment commands

use serde::Deserialize;

use crate::services::EnvironmentService;
use crate::services::environment_service::{EnvironmentConfig, EnvironmentVariableInfo};

use super::{AppState, CommandError, CommandResult, from_service, state_handle};

/// Select environment request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectEnvironmentRequest {
    pub id: String,
}

/// Update environment request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironmentRequest {
    pub id: String,
    pub name: Option<String>,
}

/// Create environment request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentRequest {
    pub name: String,
}

/// Delete environment request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEnvironmentRequest {
    pub id: String,
}

/// Get environment variables request
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEnvironmentVariablesRequest {
    pub id: Option<String>,
}

/// List all environments
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_list(state: AppState<'_>) -> CommandResult<Vec<EnvironmentConfig>> {
    let state = state_handle(&state);
    from_service(EnvironmentService::list(&state).await)
}

/// Select an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_select(
    state: AppState<'_>,
    request: SelectEnvironmentRequest,
) -> CommandResult<EnvironmentConfig> {
    let state = state_handle(&state);
    from_service(EnvironmentService::select(&state, &request.id).await)
}

/// Update (rename) an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_update(
    state: AppState<'_>,
    request: UpdateEnvironmentRequest,
) -> CommandResult<EnvironmentConfig> {
    let id = {
        let mut project = state.project.write().await;

        let Some(project) = project.as_mut() else {
            return Err(CommandError::error("No project open"));
        };

        let Some(env) = project.environments.iter_mut().find(|e| e.name == request.id) else {
            return Err(CommandError::not_found("Environment not found"));
        };

        if let Some(name) = request.name {
            env.name = name;
        }
        env.name.clone()
    };

    let state = state_handle(&state);
    from_service(EnvironmentService::get(&state, &id).await)
}

/// Create a new environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_create(
    state: AppState<'_>,
    request: CreateEnvironmentRequest,
) -> CommandResult<EnvironmentConfig> {
    let state = state_handle(&state);
    from_service(EnvironmentService::create(&state, &request.name).await)
}

/// Delete an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_delete(
    state: AppState<'_>,
    request: DeleteEnvironmentRequest,
) -> CommandResult<bool> {
    let state = state_handle(&state);
    from_service(EnvironmentService::delete(&state, &request.id).await)
}

/// List variables for selected/active environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_variables(
    state: AppState<'_>,
    request: Option<GetEnvironmentVariablesRequest>,
) -> CommandResult<Vec<EnvironmentVariableInfo>> {
    let state = state_handle(&state);
    let id = request.and_then(|r| r.id);
    from_service(EnvironmentService::list_variables(&state, id.as_deref()).await)
}
