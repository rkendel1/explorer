//! Environment commands


use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;

use super::{AppState, CommandResult};

/// Environment summary
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentSummary {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

/// Select environment request
#[derive(Debug, Deserialize)]
pub struct SelectEnvironmentRequest {
    pub id: String,
}

/// Update environment request
#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub id: String,
    pub name: Option<String>,
}

/// Create environment request
#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
}

/// Delete environment request
#[derive(Debug, Deserialize)]
pub struct DeleteEnvironmentRequest {
    pub id: String,
}

/// List all environments
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_list(
    state: AppState<'_>,
) -> CommandResult<Vec<EnvironmentSummary>> {
    let project = state.project.read().await;
    let active_env = state.active_environment.read().await;

    if let Some(project) = project.as_ref() {
        let environments: Vec<EnvironmentSummary> = project
            .environments
            .iter()
            .map(|env| EnvironmentSummary {
                id: env.name.clone(),
                name: env.name.clone(),
                is_active: active_env.as_ref() == Some(&env.name),
            })
            .collect();

        CommandResult::ok(environments)
    } else {
        CommandResult::error("No project open")
    }
}

/// Select an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_select(
    state: AppState<'_>,
    request: SelectEnvironmentRequest,
) -> CommandResult<EnvironmentSummary> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        if let Some(env) = project.environments.iter().find(|e| e.name == request.id) {
            *state.active_environment.write().await = Some(env.name.clone());

            CommandResult::ok(EnvironmentSummary {
                id: env.name.clone(),
                name: env.name.clone(),
                is_active: true,
            })
        } else {
            CommandResult::not_found("Environment not found")
        }
    } else {
        CommandResult::error("No project open")
    }
}

/// Update an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_update(
    state: AppState<'_>,
    request: UpdateEnvironmentRequest,
) -> CommandResult<EnvironmentSummary> {
    let mut project = state.project.write().await;

    if let Some(project) = project.as_mut() {
        if let Some(env) = project
            .environments
            .iter_mut()
            .find(|e| e.name == request.id)
        {
            if let Some(name) = request.name {
                env.name = name;
            }

            let active_env = state.active_environment.read().await;
            CommandResult::ok(EnvironmentSummary {
                id: env.name.clone(),
                name: env.name.clone(),
                is_active: active_env.as_ref() == Some(&env.name),
            })
        } else {
            CommandResult::not_found("Environment not found")
        }
    } else {
        CommandResult::error("No project open")
    }
}

/// Create a new environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_create(
    state: AppState<'_>,
    request: CreateEnvironmentRequest,
) -> CommandResult<EnvironmentSummary> {
    let mut project = state.project.write().await;

    if let Some(project) = project.as_mut() {
        if project.environments.iter().any(|e| e.name == request.name) {
            return CommandResult::validation_error("Environment with this name already exists");
        }

        let env = api_projects::EnvironmentReference {
            name: request.name.clone(),
        };
        project.environments.push(env.clone());

        CommandResult::ok(EnvironmentSummary {
            id: env.name.clone(),
            name: env.name.clone(),
            is_active: false,
        })
    } else {
        CommandResult::error("No project open")
    }
}

/// Delete an environment
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn environment_delete(
    state: AppState<'_>,
    request: DeleteEnvironmentRequest,
) -> CommandResult<()> {
    let mut project = state.project.write().await;

    if let Some(project) = project.as_mut() {
        let initial_len = project.environments.len();
        project.environments.retain(|e| e.name != request.id);

        if project.environments.len() < initial_len {
            let mut active_env = state.active_environment.write().await;
            if active_env.as_ref() == Some(&request.id) {
                *active_env = None;
            }
            CommandResult::ok(())
        } else {
            CommandResult::not_found("Environment not found")
        }
    } else {
        CommandResult::error("No project open")
    }
}
