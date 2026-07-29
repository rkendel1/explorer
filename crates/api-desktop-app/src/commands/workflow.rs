//! Workflow commands


use serde::{Deserialize, Serialize};

use api_workflows::WorkflowAction;
use api_workflows::events::WorkflowEventKind;

use crate::WorkflowStepSummary;
use crate::state::DesktopStateManager;

use super::{AppState, CommandResult};

/// Workflow summary
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub current_step: Option<WorkflowStepSummary>,
}

/// Workflow detail
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDetail {
    pub id: String,
    pub name: String,
    pub steps: Vec<WorkflowStepDetail>,
}

/// Workflow step detail
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStepDetail {
    pub id: String,
    pub title: String,
    pub description: String,
    pub completed: bool,
    pub action: String,
}

/// Get workflow request
#[derive(Debug, Deserialize)]
pub struct GetWorkflowRequest {
    pub id: String,
}

/// Resume workflow request
#[derive(Debug, Deserialize)]
pub struct ResumeWorkflowRequest {
    pub id: String,
}

/// Handle workflow event
#[derive(Debug, Deserialize)]
pub struct WorkflowEventRequest {
    pub event: String,
}

fn action_to_string(action: &WorkflowAction) -> String {
    match action {
        WorkflowAction::ConnectRepository => "connect_repository".to_string(),
        WorkflowAction::AnalyzeApi => "analyze_api".to_string(),
        WorkflowAction::ReviewEndpoints => "review_endpoints".to_string(),
        WorkflowAction::RunFirstRequest => "run_first_request".to_string(),
        WorkflowAction::CreateMockScenario => "create_mock_scenario".to_string(),
        WorkflowAction::RunTestSuite => "run_test_suite".to_string(),
        WorkflowAction::ConfigureEnvironment => "configure_environment".to_string(),
        WorkflowAction::Custom { action } => action.clone(),
    }
}

/// List workflows
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_list(state: AppState<'_>) -> CommandResult<Vec<WorkflowSummary>> {
    let workflows = state.workflows.read().await;

    let summaries: Vec<WorkflowSummary> = workflows
        .iter()
        .map(|wf| {
            let completed = wf.steps.iter().filter(|s| s.completed).count();
            let current = wf.steps.iter().find(|s| !s.completed);

            WorkflowSummary {
                id: wf.id.clone(),
                name: wf.name.clone(),
                completed_steps: completed,
                total_steps: wf.steps.len(),
                current_step: current.map(|s| WorkflowStepSummary {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    completed: s.completed,
                }),
            }
        })
        .collect();

    CommandResult::ok(summaries)
}

/// Get workflow details
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_get(
    state: AppState<'_>,
    request: GetWorkflowRequest,
) -> CommandResult<WorkflowDetail> {
    let workflows = state.workflows.read().await;

    if let Some(wf) = workflows.iter().find(|w| w.id == request.id) {
        let detail = WorkflowDetail {
            id: wf.id.clone(),
            name: wf.name.clone(),
            steps: wf
                .steps
                .iter()
                .map(|s| WorkflowStepDetail {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    description: s.description.clone(),
                    completed: s.completed,
                    action: action_to_string(&s.action),
                })
                .collect(),
        };
        CommandResult::ok(detail)
    } else {
        CommandResult::not_found("Workflow not found")
    }
}

/// Start a workflow
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_start(
    state: AppState<'_>,
    request: GetWorkflowRequest,
) -> CommandResult<WorkflowDetail> {
    workflow_get(state, request).await
}

/// Resume a workflow from where it left off
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_resume(
    state: AppState<'_>,
    request: ResumeWorkflowRequest,
) -> CommandResult<WorkflowDetail> {
    workflow_get(state, GetWorkflowRequest { id: request.id }).await
}

/// Handle a workflow event (for automatic step completion)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_handle_event(
    state: AppState<'_>,
    request: WorkflowEventRequest,
) -> CommandResult<Vec<String>> {
    let mut workflows = state.workflows.write().await;
    let engine_guard = state.workflow_engine.read().await;

    if let Some(engine) = engine_guard.as_ref() {
        let event_kind = match request.event.as_str() {
            "project_opened" => WorkflowEventKind::ProjectOpened,
            "repository_scanned" => WorkflowEventKind::RepositoryScanned,
            "endpoint_selected" => WorkflowEventKind::EndpointSelected,
            "request_executed" => WorkflowEventKind::RequestExecuted,
            "environment_selected" => WorkflowEventKind::EnvironmentSelected,
            "vault_credential_linked" => WorkflowEventKind::VaultCredentialLinked,
            "authentication_configured" => WorkflowEventKind::AuthenticationConfigured,
            "scenario_created" => WorkflowEventKind::ScenarioCreated,
            "mock_runtime_started" => WorkflowEventKind::MockRuntimeStarted,
            "test_suite_executed" => WorkflowEventKind::TestSuiteExecuted,
            "contract_change_reviewed" => WorkflowEventKind::ContractChangeReviewed,
            _ => return CommandResult::validation_error("Unknown event type"),
        };

        let mut completed_steps = Vec::new();

        for workflow in workflows.iter_mut() {
            let step_ids: Vec<String> = workflow.steps.iter().map(|s| s.id.clone()).collect();
            for step_id in step_ids {
                if engine.check_step(&step_id, &event_kind)
                    && let Some(step) = workflow
                        .steps
                        .iter_mut()
                        .find(|s| s.id == step_id && !s.completed)
                {
                    step.completed = true;
                    completed_steps.push(step_id.clone());
                }
            }
        }

        CommandResult::ok(completed_steps)
    } else {
        CommandResult::ok(Vec::new())
    }
}
