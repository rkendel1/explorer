//! Desktop application integration for Repo API.
//!
//! This crate provides:
//! - Desktop application lifecycle management
//! - Tauri command bridge with typed responses
//! - Application window state
//! - Desktop notifications
//! - Frontend asset integration
//!
//! This crate does NOT own:
//! - Contract compilation
//! - Repository analysis
//! - Request execution
//! - Vault encryption
//! - Runtime behavior
//! - Workflow business logic

pub mod commands;
pub mod state;

use api_projects::ApiProject;
use api_vault::VaultState;
use api_workflows::Workflow;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::RwLock;

/// Desktop application state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopApplicationState {
    pub active_project: Option<ApiProject>,
    pub active_environment: Option<String>,
    pub active_runtime_profile: Option<String>,
    pub vault_state: VaultState,
    pub runtime_state: RuntimeState,
    pub workflow_state: WorkflowStateSnapshot,
}

/// Runtime state for the mock server
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    pub status: RuntimeStatus,
    pub address: Option<String>,
    pub requests: u64,
    pub validation_failures: u64,
}

/// Runtime status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

/// Workflow state snapshot for the desktop
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowStateSnapshot {
    pub active_workflow_id: Option<String>,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub current_step: Option<WorkflowStepSummary>,
}

/// Workflow step summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepSummary {
    pub id: String,
    pub title: String,
    pub completed: bool,
}

/// Recent project entry for the project picker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub last_opened: chrono::DateTime<chrono::Utc>,
}

/// Desktop launch result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopLaunchResult {
    pub project: Option<ApiProject>,
    pub recent_projects: Vec<RecentProject>,
    pub state: DesktopApplicationState,
}

/// Endpoint summary for the API explorer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSummary {
    pub id: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub confidence: f32,
    pub tag: Option<String>,
}

/// Endpoint detail for viewing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDetail {
    pub id: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub parameters: Vec<ParameterInfo>,
    pub request_body: Option<RequestBodyInfo>,
    pub responses: Vec<ResponseInfo>,
    pub security: Vec<String>,
    pub confidence: f32,
    pub evidence: Vec<EvidenceInfo>,
}

/// Parameter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    pub location: String,
    pub required: bool,
    pub schema_type: String,
}

/// Request body information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBodyInfo {
    pub content_type: String,
    pub required: bool,
    pub schema_ref: Option<String>,
}

/// Response information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseInfo {
    pub status: u16,
    pub content_type: Option<String>,
    pub schema_ref: Option<String>,
}

/// Evidence information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceInfo {
    pub file: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

/// Request execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResult {
    pub status: u16,
    pub duration_ms: u64,
    pub body_size: usize,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub validation: ValidationResult,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

/// Validation issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: String,
    pub message: String,
    pub path: Option<String>,
}

/// Vault entry metadata (no secret values)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntryMetadata {
    pub id: String,
    pub name: String,
    pub secret_type: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Contract change summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChangeSummary {
    pub total_changes: usize,
    pub added: Vec<ChangeEntry>,
    pub modified: Vec<ChangeEntry>,
    pub removed: Vec<ChangeEntry>,
    pub potentially_breaking: Vec<ChangeEntry>,
}

/// Change entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEntry {
    pub kind: String,
    pub description: String,
    pub path: Option<String>,
}

/// Test suite summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteSummary {
    pub id: String,
    pub name: String,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Test result detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultDetail {
    pub test_id: String,
    pub test_name: String,
    pub passed: bool,
    pub duration_ms: u64,
    pub assertions: Vec<AssertionResult>,
}

/// Assertion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    pub passed: bool,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Application state manager
pub struct AppState {
    pub root: RwLock<Option<PathBuf>>,
    pub project: RwLock<Option<ApiProject>>,
    pub active_environment: RwLock<Option<String>>,
    pub vault_state: RwLock<VaultState>,
    pub runtime_state: RwLock<RuntimeState>,
    pub workflows: RwLock<Vec<Workflow>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            root: RwLock::new(None),
            project: RwLock::new(None),
            active_environment: RwLock::new(None),
            vault_state: RwLock::new(VaultState::Locked),
            runtime_state: RwLock::new(RuntimeState::default()),
            workflows: RwLock::new(Vec::new()),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_snapshot(&self) -> DesktopApplicationState {
        let project = self.project.read().await.clone();
        let active_environment = self.active_environment.read().await.clone();
        let active_runtime_profile = project
            .as_ref()
            .and_then(|p| p.active_runtime_profile.clone());
        let vault_state = *self.vault_state.read().await;
        let runtime_state = self.runtime_state.read().await.clone();

        let workflows = self.workflows.read().await;
        let workflow_state = if let Some(wf) = workflows.first() {
            let completed = wf.steps.iter().filter(|s| s.completed).count();
            let current = wf.steps.iter().find(|s| !s.completed);
            WorkflowStateSnapshot {
                active_workflow_id: Some(wf.id.clone()),
                completed_steps: completed,
                total_steps: wf.steps.len(),
                current_step: current.map(|s| WorkflowStepSummary {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    completed: s.completed,
                }),
            }
        } else {
            WorkflowStateSnapshot::default()
        };

        DesktopApplicationState {
            active_project: project,
            active_environment,
            active_runtime_profile,
            vault_state,
            runtime_state,
            workflow_state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn app_state_default() {
        let state = AppState::new();
        let snapshot = state.get_snapshot().await;
        assert!(snapshot.active_project.is_none());
        assert_eq!(snapshot.vault_state, VaultState::Locked);
    }
}
