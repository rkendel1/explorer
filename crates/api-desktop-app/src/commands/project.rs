//! Project commands

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::RecentProject;
use crate::state::DesktopStateManager;

use super::CommandResult;

/// Open project request
#[derive(Debug, Deserialize)]
pub struct OpenProjectRequest {
    pub path: String,
}

/// Open project response
#[derive(Debug, Serialize)]
pub struct OpenProjectResponse {
    pub name: String,
    pub environment_count: usize,
    pub has_contract: bool,
}

/// Create project request
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub path: String,
    pub name: Option<String>,
}

/// List recent projects
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_list(state: Arc<DesktopStateManager>) -> CommandResult<Vec<RecentProject>> {
    match state.load_recent_projects().await {
        Ok(projects) => CommandResult::ok(projects),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

/// Open a project at the given path
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_open(
    state: Arc<DesktopStateManager>,
    request: OpenProjectRequest,
) -> CommandResult<OpenProjectResponse> {
    let path = PathBuf::from(&request.path);

    match state.open_project(path).await {
        Ok(project) => {
            let response = OpenProjectResponse {
                name: project.name.clone(),
                environment_count: project.environments.len(),
                has_contract: !project.contract.path.is_empty(),
            };
            CommandResult::ok(response)
        }
        Err(e) => CommandResult::error(e.to_string()),
    }
}

/// Create a new project
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_create(
    state: Arc<DesktopStateManager>,
    request: CreateProjectRequest,
) -> CommandResult<OpenProjectResponse> {
    let path = PathBuf::from(&request.path);

    // Use the existing project creation
    match state.open_project(path).await {
        Ok(project) => {
            let response = OpenProjectResponse {
                name: project.name.clone(),
                environment_count: project.environments.len(),
                has_contract: !project.contract.path.is_empty(),
            };
            CommandResult::ok(response)
        }
        Err(e) => CommandResult::error(e.to_string()),
    }
}

/// Close the current project
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_close(state: Arc<DesktopStateManager>) -> CommandResult<()> {
    state.close_project().await;
    CommandResult::ok(())
}

/// Remove a recent project from the list
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn project_remove_recent(
    state: Arc<DesktopStateManager>,
    path: String,
) -> CommandResult<()> {
    let path = PathBuf::from(&path);
    let mut projects = state.recent_projects.write().await;
    projects.retain(|p| p.path != path);
    drop(projects);

    if let Err(e) = state.save_recent_projects().await {
        return CommandResult::error(e.to_string());
    }

    CommandResult::ok(())
}
