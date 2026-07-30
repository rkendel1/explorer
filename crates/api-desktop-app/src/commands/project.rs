//! Project commands

use std::path::PathBuf;

use serde::Deserialize;

use crate::RecentProject;
use crate::services::ProjectService;
use crate::services::project_service::ProjectSummary;

use super::{AppState, CommandResult, from_service, state_handle};

/// Open project request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectRequest {
    pub path: String,
}

/// Create project request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub path: String,
    pub name: Option<String>,
}

/// List recent projects
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_list(state: AppState<'_>) -> CommandResult<Vec<RecentProject>> {
    let state = state_handle(&state);
    from_service(ProjectService::get_recent_projects(&state).await)
}

/// Open a project at the given path
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_open(
    state: AppState<'_>,
    request: OpenProjectRequest,
) -> CommandResult<ProjectSummary> {
    let state = state_handle(&state);
    from_service(ProjectService::open_project_input(&state, &request.path).await)
}

/// Create a new project (currently identical to opening one - project
/// creation happens implicitly the first time a path is opened)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_create(
    state: AppState<'_>,
    request: CreateProjectRequest,
) -> CommandResult<ProjectSummary> {
    let state = state_handle(&state);
    from_service(ProjectService::open_project_input(&state, &request.path).await)
}

/// Close the current project
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_close(state: AppState<'_>) -> CommandResult<()> {
    let state = state_handle(&state);
    from_service(ProjectService::close_project(&state).await)
}

/// Remove a recent project from the list
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_remove_recent(state: AppState<'_>, path: String) -> CommandResult<()> {
    let state = state_handle(&state);
    from_service(ProjectService::remove_from_recent(&state, PathBuf::from(&path)).await)
}
