//! Project commands

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::RecentProject;

use super::{AppState, CommandResult};

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
pub async fn project_list(state: AppState<'_>) -> CommandResult<Vec<RecentProject>> {
    match state.load_recent_projects().await {
        Ok(projects) => CommandResult::ok(projects),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

/// Open a project at the given path
pub async fn project_open(
    state: AppState<'_>,
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
pub async fn project_create(
    state: AppState<'_>,
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
pub async fn project_close(state: AppState<'_>) -> CommandResult<()> {
    state.close_project().await;
    CommandResult::ok(())
}

/// Remove a recent project from the list
pub async fn project_remove_recent(state: AppState<'_>, path: String) -> CommandResult<()> {
    let path = PathBuf::from(&path);
    let mut projects = state.recent_projects.write().await;
    projects.retain(|p| p.path != path);
    drop(projects);

    if let Err(e) = state.save_recent_projects().await {
        return CommandResult::error(e.to_string());
    }

    CommandResult::ok(())
}
