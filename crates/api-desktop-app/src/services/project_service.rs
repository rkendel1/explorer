//! Project service for desktop application lifecycle management.
//!
//! This service handles:
//! - Project opening and closing
//! - Project state persistence
//! - Recent projects management
//! - Project restoration on restart

use std::path::{Path, PathBuf};
use std::sync::Arc;

use api_workflows::events::WorkflowCompletionEngine;
use api_workflows::{Workflow, create_workflow, list_workflows, starter_workflow_steps};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::RecentProject;
use crate::state::DesktopStateManager;

use super::{CustomerJourneyService, ServiceError, ServiceResult};

/// Project restoration state for application restart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRestorationState {
    /// Active project path
    pub project_path: Option<PathBuf>,
    /// Active environment name
    pub active_environment: Option<String>,
    /// Active runtime profile ID
    pub active_runtime_profile: Option<String>,
    /// Selected workspace (explorer, requests, etc.)
    pub selected_workspace: Option<String>,
    /// Selected endpoint ID
    pub selected_endpoint: Option<String>,
    /// Workflow progress by workflow ID
    pub workflow_progress: std::collections::HashMap<String, WorkflowProgress>,
}

/// Workflow progress for restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowProgress {
    pub workflow_id: String,
    pub completed_steps: Vec<String>,
}

/// Project summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub name: String,
    pub path: PathBuf,
    pub endpoint_count: usize,
    pub schema_count: usize,
    pub environment_count: usize,
    pub has_contract: bool,
    pub workflow_progress: Option<(usize, usize)>,
}

/// Project service implementation
pub struct ProjectService;

impl ProjectService {
    /// Open a project at the given path
    pub async fn open_project(
        state: &Arc<DesktopStateManager>,
        path: PathBuf,
    ) -> ServiceResult<ProjectSummary> {
        // Try to load existing project or create new one
        let project = match api_projects::load_project(&path) {
            Ok(Some(existing)) => existing,
            Ok(None) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Untitled Project".to_string());
                api_projects::create_project(&path, name)
                    .map_err(|e| ServiceError::internal(&e.to_string()))?
            }
            Err(e) => return Err(ServiceError::internal(&e.to_string())),
        };

        // Store the root and project
        *state.active_root.write().await = Some(path.clone());
        *state.project.write().await = Some(project.clone());
        let _ = api_customer_journey::load_or_initialize_customer_journey_state(
            &path,
            project.id.clone(),
        )
        .map_err(|e| ServiceError::internal(&e.to_string()))?;

        // Add to recent projects
        Self::add_to_recent_projects(state, path.clone(), project.name.clone()).await;

        // Initialize workflows
        let workflows = Self::initialize_workflows(state, &path).await?;
        let workflow_progress = workflows.first().map(|wf| {
            let completed = wf.steps.iter().filter(|s| s.completed).count();
            (completed, wf.steps.len())
        });

        let _ = CustomerJourneyService::complete_outcome(
            state,
            api_customer_journey::JourneyOutcome::RepositoryConnected,
        )
        .await;

        if let Ok(contract) = api_storage::load_effective_contract(&path)
            && !contract.endpoints.is_empty()
        {
            let _ = CustomerJourneyService::complete_outcome(
                state,
                api_customer_journey::JourneyOutcome::ApiDiscovered,
            )
            .await;
        }

        Ok(ProjectSummary {
            name: project.name,
            path,
            endpoint_count: 0, // Will be populated after discovery
            schema_count: 0,
            environment_count: project.environments.len(),
            has_contract: !project.contract.path.is_empty(),
            workflow_progress,
        })
    }

    /// Close the current project
    pub async fn close_project(state: &Arc<DesktopStateManager>) -> ServiceResult<()> {
        // Save current state before closing
        if let Some(path) = state.active_root.read().await.as_ref() {
            let _ = Self::save_restoration_state(state, path).await;
        }

        *state.active_root.write().await = None;
        *state.project.write().await = None;
        *state.workflows.write().await = Vec::new();
        *state.workflow_engine.write().await = None;

        Ok(())
    }

    /// Get recent projects list
    pub async fn get_recent_projects(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<RecentProject>> {
        match state.load_recent_projects().await {
            Ok(projects) => Ok(projects),
            Err(e) => Err(ServiceError::internal(&e.to_string())),
        }
    }

    /// Remove a project from recent list
    pub async fn remove_from_recent(
        state: &Arc<DesktopStateManager>,
        path: PathBuf,
    ) -> ServiceResult<()> {
        let mut projects = state.recent_projects.write().await;
        projects.retain(|p| p.path != path);
        drop(projects);

        state
            .save_recent_projects()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(())
    }

    /// Restore project state from persistence
    pub async fn restore_project(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Option<ProjectRestorationState>> {
        let restoration_path = state.app_data_dir.join("restoration_state.json");

        if !restoration_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&restoration_path)
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        let restoration_state: ProjectRestorationState =
            serde_json::from_str(&content).map_err(|e| ServiceError::internal(&e.to_string()))?;

        // If there's a project path, try to restore it
        if let Some(project_path) = &restoration_state.project_path
            && project_path.exists()
        {
            let _ = Self::open_project(state, project_path.clone()).await;

            // Restore active environment
            if let Some(env_name) = &restoration_state.active_environment {
                *state.active_environment.write().await = Some(env_name.clone());
            }

            // Restore workflow progress
            Self::restore_workflow_progress(state, &restoration_state).await;
        }

        Ok(Some(restoration_state))
    }

    /// Save restoration state for application restart
    pub async fn save_restoration_state(
        state: &Arc<DesktopStateManager>,
        project_path: &Path,
    ) -> ServiceResult<()> {
        let active_environment = state.active_environment.read().await.clone();
        let project = state.project.read().await;
        let active_runtime_profile = project
            .as_ref()
            .and_then(|p| p.active_runtime_profile.clone());
        drop(project);

        let workflows = state.workflows.read().await;
        let mut workflow_progress = std::collections::HashMap::new();
        for wf in workflows.iter() {
            let completed_steps: Vec<String> = wf
                .steps
                .iter()
                .filter(|s| s.completed)
                .map(|s| s.id.clone())
                .collect();
            workflow_progress.insert(
                wf.id.clone(),
                WorkflowProgress {
                    workflow_id: wf.id.clone(),
                    completed_steps,
                },
            );
        }

        let restoration_state = ProjectRestorationState {
            project_path: Some(project_path.to_path_buf()),
            active_environment,
            active_runtime_profile,
            selected_workspace: None, // Could be set by frontend
            selected_endpoint: None,
            workflow_progress,
        };

        let restoration_path = state.app_data_dir.join("restoration_state.json");
        if let Some(parent) = restoration_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ServiceError::internal(&e.to_string()))?;
        }

        let content = serde_json::to_string_pretty(&restoration_state)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        tokio::fs::write(&restoration_path, content)
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(())
    }

    // Private helpers

    async fn add_to_recent_projects(state: &Arc<DesktopStateManager>, path: PathBuf, name: String) {
        let mut projects = state.recent_projects.write().await;

        // Remove existing entry for this path
        projects.retain(|p| p.path != path);

        // Add to the front
        projects.insert(
            0,
            RecentProject {
                path,
                name,
                last_opened: Utc::now(),
            },
        );

        // Keep only 10 recent projects
        projects.truncate(10);
    }

    async fn initialize_workflows(
        state: &Arc<DesktopStateManager>,
        path: &Path,
    ) -> ServiceResult<Vec<Workflow>> {
        let workflows_dir = path.join(".repo-api/workflows");

        let workflows = match list_workflows(&workflows_dir) {
            Ok(wfs) if !wfs.is_empty() => wfs,
            _ => {
                // Create starter workflow if none exists
                let workflow = create_workflow(path, "Getting Started", starter_workflow_steps())
                    .map_err(|e| ServiceError::internal(&e.to_string()))?;
                vec![workflow]
            }
        };

        *state.workflows.write().await = workflows.clone();

        // Set up workflow completion engine
        let engine = WorkflowCompletionEngine::with_defaults(path);
        for wf in &workflows {
            engine.register_workflow(wf.clone()).await;
        }
        *state.workflow_engine.write().await = Some(engine);

        Ok(workflows)
    }

    async fn restore_workflow_progress(
        state: &Arc<DesktopStateManager>,
        restoration_state: &ProjectRestorationState,
    ) {
        let mut workflows = state.workflows.write().await;

        for workflow in workflows.iter_mut() {
            if let Some(progress) = restoration_state.workflow_progress.get(&workflow.id) {
                for step in workflow.steps.iter_mut() {
                    if progress.completed_steps.contains(&step.id) {
                        step.completed = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_project_open_and_close() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        // Open project
        let summary = ProjectService::open_project(&state, project_dir.path().to_path_buf())
            .await
            .unwrap();

        assert!(!summary.name.is_empty());
        assert!(state.project.read().await.is_some());
        assert!(
            project_dir
                .path()
                .join(".repo-api/customer-journey.json")
                .exists()
        );

        // Close project
        ProjectService::close_project(&state).await.unwrap();
        assert!(state.project.read().await.is_none());
    }

    #[tokio::test]
    async fn test_recent_projects() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        // Open project (adds to recent)
        ProjectService::open_project(&state, project_dir.path().to_path_buf())
            .await
            .unwrap();

        let recent = state.recent_projects.read().await;
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn test_restoration_state() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        // Open and configure project
        ProjectService::open_project(&state, project_dir.path().to_path_buf())
            .await
            .unwrap();
        *state.active_environment.write().await = Some("staging".to_string());

        // Save restoration state
        ProjectService::save_restoration_state(&state, project_dir.path())
            .await
            .unwrap();

        // Verify file exists
        let restoration_path = app_dir.path().join("restoration_state.json");
        assert!(restoration_path.exists());
    }
}
