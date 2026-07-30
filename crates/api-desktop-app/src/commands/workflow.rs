//! Workflow commands

use serde::Deserialize;

use api_workflows::events::WorkflowEventKind;

use crate::services::WorkflowService;
use crate::services::workflow_service::{WorkflowDetail, WorkflowEventResult, WorkflowSummary};

use super::{AppState, CommandError, CommandResult, from_service, state_handle};

/// Get workflow request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWorkflowRequest {
    pub id: String,
}

/// Resume workflow request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeWorkflowRequest {
    pub id: String,
}

/// Handle workflow event
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEventRequest {
    pub event: String,
}

fn parse_event_kind(value: &str) -> Option<WorkflowEventKind> {
    match value {
        "project_opened" => Some(WorkflowEventKind::ProjectOpened),
        "repository_scanned" => Some(WorkflowEventKind::RepositoryScanned),
        "endpoint_selected" => Some(WorkflowEventKind::EndpointSelected),
        "request_executed" => Some(WorkflowEventKind::RequestExecuted),
        "environment_selected" => Some(WorkflowEventKind::EnvironmentSelected),
        "vault_credential_linked" => Some(WorkflowEventKind::VaultCredentialLinked),
        "authentication_configured" => Some(WorkflowEventKind::AuthenticationConfigured),
        "scenario_created" => Some(WorkflowEventKind::ScenarioCreated),
        "mock_runtime_started" => Some(WorkflowEventKind::MockRuntimeStarted),
        "test_suite_executed" => Some(WorkflowEventKind::TestSuiteExecuted),
        "contract_change_reviewed" => Some(WorkflowEventKind::ContractChangeReviewed),
        _ => None,
    }
}

/// List workflows
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_list(state: AppState<'_>) -> CommandResult<Vec<WorkflowSummary>> {
    let state = state_handle(&state);
    from_service(WorkflowService::list(&state).await)
}

/// Get workflow details
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn workflow_get(
    state: AppState<'_>,
    request: GetWorkflowRequest,
) -> CommandResult<WorkflowDetail> {
    let state = state_handle(&state);
    from_service(WorkflowService::get(&state, &request.id).await)
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
) -> CommandResult<WorkflowEventResult> {
    let Some(event_kind) = parse_event_kind(&request.event) else {
        return Err(CommandError::validation_error("Unknown event type"));
    };

    let state = state_handle(&state);
    from_service(WorkflowService::emit_event(&state, event_kind).await)
}
