//! Contract change commands

use serde::Deserialize;

use crate::ContractChangeSummary;
use crate::services::ChangesService;
use crate::services::changes_service::ChangeDetail;

use super::{AppState, CommandResult, from_service, state_handle};

/// Review change request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewChangeRequest {
    pub change_id: String,
}

/// Accept change request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptChangeRequest {
    pub change_id: String,
}

/// Reject change request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectChangeRequest {
    pub change_id: String,
    pub reason: Option<String>,
}

/// List contract changes
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_list(state: AppState<'_>) -> CommandResult<ContractChangeSummary> {
    let state = state_handle(&state);
    from_service(ChangesService::list(&state).await)
}

/// Review a specific change
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_review(
    state: AppState<'_>,
    request: ReviewChangeRequest,
) -> CommandResult<ChangeDetail> {
    let state = state_handle(&state);
    from_service(ChangesService::get(&state, &request.change_id).await)
}

/// Accept a change (update effective contract)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_accept(state: AppState<'_>, request: AcceptChangeRequest) -> CommandResult<()> {
    let state = state_handle(&state);
    from_service(ChangesService::accept(&state, &request.change_id).await).map(|_| ())
}

/// Reject a change (keep current contract)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_reject(state: AppState<'_>, request: RejectChangeRequest) -> CommandResult<()> {
    let state = state_handle(&state);
    from_service(ChangesService::reject(&state, &request.change_id, request.reason.as_deref()).await)
        .map(|_| ())
}

/// Accept all changes
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_accept_all(state: AppState<'_>) -> CommandResult<usize> {
    let state = state_handle(&state);
    from_service(ChangesService::accept_all(&state).await)
}

/// Keep current contract (reject all changes)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_keep_current(state: AppState<'_>) -> CommandResult<usize> {
    let state = state_handle(&state);
    from_service(ChangesService::keep_current(&state).await)
}
