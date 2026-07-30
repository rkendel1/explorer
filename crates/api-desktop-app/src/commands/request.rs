//! Request execution commands

use std::collections::HashMap;

use serde::Deserialize;

use crate::RequestResult;
use crate::services::RequestService;
use crate::services::request_service::{
    ExecuteRequestInput as ServiceExecuteRequestInput, RequestHistoryItem, SavedRequestInfo,
};
use crate::services::vault_service::AuthenticationConfig;

use super::{AppState, CommandResult, from_service, state_handle};

/// Execute request input
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequestInput {
    pub method: String,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
    pub environment_id: Option<String>,
    #[serde(default)]
    pub authentication: Option<AuthenticationConfig>,
}

impl From<ExecuteRequestInput> for ServiceExecuteRequestInput {
    fn from(input: ExecuteRequestInput) -> Self {
        Self {
            method: input.method,
            url: input.url,
            headers: input.headers,
            body: input.body,
            environment_id: input.environment_id,
            authentication: input.authentication,
        }
    }
}

/// Save request input
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequestInput {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
}

/// Delete request input
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequestInput {
    pub id: String,
}

/// Execute a request
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_execute(
    state: AppState<'_>,
    request: ExecuteRequestInput,
) -> CommandResult<RequestResult> {
    let state = state_handle(&state);
    let service = RequestService::new();
    let output = from_service(service.execute(&state, request.into()).await)?;
    Ok(RequestResult {
        status: output.status,
        duration_ms: output.duration_ms,
        body_size: output.body_size,
        headers: output.headers,
        body: output.body,
        validation: output.validation,
    })
}

/// Save a request for later
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_save(
    state: AppState<'_>,
    request: SaveRequestInput,
) -> CommandResult<SavedRequestInfo> {
    let state = state_handle(&state);
    let service = RequestService::new();
    from_service(
        service
            .save_request(
                &state,
                &request.name,
                &request.method,
                &request.url,
                request.headers,
                request.body,
            )
            .await,
    )
}

/// List saved requests
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_saved_list(state: AppState<'_>) -> CommandResult<Vec<SavedRequestInfo>> {
    let state = state_handle(&state);
    let service = RequestService::new();
    from_service(service.list_saved(&state).await)
}

/// Get request history
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_history(state: AppState<'_>) -> CommandResult<Vec<RequestHistoryItem>> {
    let state = state_handle(&state);
    let service = RequestService::new();
    from_service(service.get_history(&state).await)
}

/// Delete a request from history
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_delete(state: AppState<'_>, request: DeleteRequestInput) -> CommandResult<()> {
    let state = state_handle(&state);
    let service = RequestService::new();
    from_service(service.delete_history_entry(&state, &request.id).await)
}

/// Clear all request history for the current project
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_history_clear(state: AppState<'_>) -> CommandResult<()> {
    let state = state_handle(&state);
    let service = RequestService::new();
    from_service(service.clear_history(&state).await)
}
