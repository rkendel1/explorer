//! Application state management for the desktop app.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use api_customer_journey::CustomerJourneyState;
use api_projects::ApiProject;
use api_runtime_events::RuntimeEvent;
use api_vault::VaultState;
use api_workflows::Workflow;
use api_workflows::events::WorkflowCompletionEngine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::{RecentProject, RuntimeState, RuntimeStatus};

/// Maximum number of recent runtime events kept in memory for `get_events`.
const MAX_BUFFERED_RUNTIME_EVENTS: usize = 200;

/// Background tasks backing a running mock server: the server itself and the
/// task pumping its events into `DesktopStateManager::runtime_events`. Both
/// get aborted together on stop/restart.
pub struct RunningMockServer {
    pub server_task: JoinHandle<()>,
    pub event_pump_task: JoinHandle<()>,
}

impl RunningMockServer {
    /// Abort both tasks and wait for them to actually finish. `abort()`
    /// alone only *requests* cancellation - the listening socket isn't
    /// guaranteed to be closed (and the port released) until the aborted
    /// task is actually polled and torn down, so callers that need the
    /// port free immediately (e.g. a restart binding the same port) must
    /// await this rather than fire-and-forget `abort()`.
    pub async fn abort(self) {
        self.server_task.abort();
        self.event_pump_task.abort();
        let _ = self.server_task.await;
        let _ = self.event_pump_task.await;
    }
}

/// Desktop application state manager
pub struct DesktopStateManager {
    /// Path to the application data directory
    pub app_data_dir: PathBuf,

    /// Currently active project root
    pub active_root: RwLock<Option<PathBuf>>,

    /// Loaded project
    pub project: RwLock<Option<ApiProject>>,

    /// Active environment name
    pub active_environment: RwLock<Option<String>>,

    /// Vault state (cached, since state() is async)
    pub vault_state: RwLock<VaultState>,

    /// Runtime state
    pub runtime: RwLock<RuntimeState>,

    /// Handles for the currently running mock server, if any
    pub runtime_server: RwLock<Option<RunningMockServer>>,

    /// Recent runtime events, most recent first (bounded ring buffer)
    pub runtime_events: RwLock<VecDeque<RuntimeEvent>>,

    /// Workflows
    pub workflows: RwLock<Vec<Workflow>>,

    /// Workflow completion engine
    pub workflow_engine: RwLock<Option<WorkflowCompletionEngine>>,

    /// Recent projects
    pub recent_projects: RwLock<Vec<RecentProject>>,

    /// Request history per project
    pub request_history: RwLock<HashMap<String, Vec<RequestHistoryEntry>>>,

    /// Customer journey state for progressive onboarding
    pub customer_journey: RwLock<Option<CustomerJourneyState>>,
}

impl DesktopStateManager {
    /// Push a runtime event into the bounded buffer, evicting the oldest
    /// entry once `MAX_BUFFERED_RUNTIME_EVENTS` is exceeded.
    pub async fn push_runtime_event(&self, event: RuntimeEvent) {
        let mut events = self.runtime_events.write().await;
        events.push_front(event);
        events.truncate(MAX_BUFFERED_RUNTIME_EVENTS);
    }
}

/// Request history entry (secrets redacted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHistoryEntry {
    pub id: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

impl DesktopStateManager {
    /// Create a new state manager with the given app data directory
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            app_data_dir,
            active_root: RwLock::new(None),
            project: RwLock::new(None),
            active_environment: RwLock::new(None),
            vault_state: RwLock::new(VaultState::Locked),
            runtime: RwLock::new(RuntimeState::default()),
            runtime_server: RwLock::new(None),
            runtime_events: RwLock::new(VecDeque::new()),
            workflows: RwLock::new(Vec::new()),
            workflow_engine: RwLock::new(None),
            recent_projects: RwLock::new(Vec::new()),
            request_history: RwLock::new(HashMap::new()),
            customer_journey: RwLock::new(None),
        }
    }

    /// Load recent projects from disk
    pub async fn load_recent_projects(&self) -> anyhow::Result<Vec<RecentProject>> {
        let path = self.app_data_dir.join("recent_projects.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let projects: Vec<RecentProject> = serde_json::from_str(&content)?;
        *self.recent_projects.write().await = projects.clone();
        Ok(projects)
    }

    /// Save recent projects to disk
    pub async fn save_recent_projects(&self) -> anyhow::Result<()> {
        let projects = self.recent_projects.read().await;
        let path = self.app_data_dir.join("recent_projects.json");

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(&*projects)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Add a project to recent projects
    pub async fn add_recent_project(&self, path: PathBuf, name: String) {
        let mut projects = self.recent_projects.write().await;

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

    /// Open a project at the given path
    pub async fn open_project(&self, path: PathBuf) -> anyhow::Result<ApiProject> {
        // Try to load existing project or create new one
        let project = if let Some(existing) = api_projects::load_project(&path)? {
            existing
        } else {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled Project".to_string());
            api_projects::create_project(&path, name)?
        };

        // Store the root and project
        *self.active_root.write().await = Some(path.clone());
        *self.project.write().await = Some(project.clone());

        // Add to recent projects
        self.add_recent_project(path.clone(), project.name.clone())
            .await;

        // Load workflows
        let workflows_dir = self.project_workflows_dir().await;
        if let Ok(workflows) = api_workflows::list_workflows(&workflows_dir) {
            *self.workflows.write().await = workflows.clone();

            // Set up workflow completion engine
            let engine = WorkflowCompletionEngine::with_defaults(&path);
            *self.workflow_engine.write().await = Some(engine);
        }

        if let Ok(journey) = api_customer_journey::load_or_initialize_customer_journey_state(
            &path,
            project.id.clone(),
        ) {
            *self.customer_journey.write().await = Some(journey);
        }

        Ok(project)
    }

    /// Get the workflows directory for the current project
    pub async fn project_workflows_dir(&self) -> PathBuf {
        if let Some(root) = self.active_root.read().await.as_ref() {
            root.join(".repo-api").join("workflows")
        } else {
            PathBuf::from(".repo-api/workflows")
        }
    }

    /// Close the current project
    pub async fn close_project(&self) {
        *self.active_root.write().await = None;
        *self.project.write().await = None;
        *self.workflows.write().await = Vec::new();
        *self.workflow_engine.write().await = None;
        *self.customer_journey.write().await = None;
    }

    /// Get the vault state
    pub async fn get_vault_state(&self) -> VaultState {
        *self.vault_state.read().await
    }

    /// Set the vault state
    pub async fn set_vault_state(&self, state: VaultState) {
        *self.vault_state.write().await = state;
    }

    /// Set the runtime state
    pub async fn set_runtime_state(&self, status: RuntimeStatus, address: Option<String>) {
        let mut runtime = self.runtime.write().await;
        runtime.status = status;
        runtime.address = address;
        if status != RuntimeStatus::Running {
            runtime.managed_by_desktop = false;
        }
    }

    /// Set runtime state with explicit ownership metadata.
    pub async fn set_runtime_state_with_ownership(
        &self,
        status: RuntimeStatus,
        address: Option<String>,
        managed_by_desktop: bool,
    ) {
        let mut runtime = self.runtime.write().await;
        runtime.status = status;
        runtime.address = address;
        runtime.managed_by_desktop = managed_by_desktop;
    }

    /// Update runtime metrics
    pub async fn update_runtime_metrics(&self, requests: u64, validation_failures: u64) {
        let mut runtime = self.runtime.write().await;
        runtime.requests = requests;
        runtime.validation_failures = validation_failures;
    }

    /// Add a request to history
    pub async fn add_request_history(&self, project_id: &str, entry: RequestHistoryEntry) {
        let mut history = self.request_history.write().await;
        let entries = history.entry(project_id.to_string()).or_default();
        entries.insert(0, entry);
        // Keep only 100 entries per project
        entries.truncate(100);
    }

    /// Get request history for a project
    pub async fn get_request_history(&self, project_id: &str) -> Vec<RequestHistoryEntry> {
        let history = self.request_history.read().await;
        history.get(project_id).cloned().unwrap_or_default()
    }

    /// Get or initialize customer journey state for the active project
    pub async fn get_or_initialize_customer_journey(&self) -> anyhow::Result<CustomerJourneyState> {
        if let Some(existing) = self.customer_journey.read().await.clone() {
            return Ok(existing);
        }

        let active_root = self.active_root.read().await.clone();
        let project = self.project.read().await.clone();
        let root = active_root.ok_or_else(|| anyhow::anyhow!("no active project root"))?;
        let project = project.ok_or_else(|| anyhow::anyhow!("no active project"))?;

        let journey =
            api_customer_journey::load_or_initialize_customer_journey_state(&root, project.id)?;
        *self.customer_journey.write().await = Some(journey.clone());
        Ok(journey)
    }

    /// Persist customer journey for active project
    pub async fn save_customer_journey(&self, state: CustomerJourneyState) -> anyhow::Result<()> {
        let active_root = self.active_root.read().await.clone();
        let root = active_root.ok_or_else(|| anyhow::anyhow!("no active project root"))?;
        api_customer_journey::save_customer_journey_state(&root, &state)?;
        *self.customer_journey.write().await = Some(state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_recent_projects() {
        let dir = tempdir().unwrap();
        let manager = DesktopStateManager::new(dir.path().to_path_buf());

        manager
            .add_recent_project("/test/project".into(), "Test Project".to_string())
            .await;

        let projects = manager.recent_projects.read().await;
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Test Project");
    }

    #[tokio::test]
    async fn test_vault_state() {
        let dir = tempdir().unwrap();
        let manager = DesktopStateManager::new(dir.path().to_path_buf());
        assert_eq!(manager.get_vault_state().await, VaultState::Locked);
    }

    #[tokio::test]
    async fn test_request_history() {
        let dir = tempdir().unwrap();
        let manager = DesktopStateManager::new(dir.path().to_path_buf());

        let entry = RequestHistoryEntry {
            id: "req-1".to_string(),
            method: "GET".to_string(),
            url: "http://localhost/test".to_string(),
            status: 200,
            duration_ms: 50,
            timestamp: Utc::now(),
        };

        manager.add_request_history("proj-1", entry).await;

        let history = manager.get_request_history("proj-1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].method, "GET");
    }

    #[tokio::test]
    async fn test_customer_journey_state() {
        let dir = tempdir().unwrap();
        let manager = DesktopStateManager::new(dir.path().to_path_buf());
        let project = api_projects::create_project(dir.path(), "Journey Project").unwrap();
        *manager.active_root.write().await = Some(dir.path().to_path_buf());
        *manager.project.write().await = Some(project.clone());

        let mut state = manager.get_or_initialize_customer_journey().await.unwrap();
        assert_eq!(state.project_id, project.id);

        state.complete_outcome(api_customer_journey::JourneyOutcome::RepositoryConnected);
        manager.save_customer_journey(state.clone()).await.unwrap();

        let loaded = manager.get_or_initialize_customer_journey().await.unwrap();
        assert!(
            loaded
                .completed_outcomes
                .contains(&api_customer_journey::JourneyOutcome::RepositoryConnected)
        );
    }
}
