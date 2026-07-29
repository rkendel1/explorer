//! Contract change commands

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ContractChangeSummary;
use crate::state::DesktopStateManager;

use super::CommandResult;

/// Review change request
#[derive(Debug, Deserialize)]
pub struct ReviewChangeRequest {
    pub change_id: String,
}

/// Change detail
#[derive(Debug, Serialize)]
pub struct ChangeDetail {
    pub id: String,
    pub kind: String,
    pub description: String,
    pub path: Option<String>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub is_breaking: bool,
    pub impact: String,
}

/// Accept change request
#[derive(Debug, Deserialize)]
pub struct AcceptChangeRequest {
    pub change_id: String,
}

/// Reject change request
#[derive(Debug, Deserialize)]
pub struct RejectChangeRequest {
    pub change_id: String,
    pub reason: Option<String>,
}

/// List contract changes
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_list(state: Arc<DesktopStateManager>) -> CommandResult<ContractChangeSummary> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(ContractChangeSummary {
        total_changes: 0,
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        potentially_breaking: Vec::new(),
    })
}

/// Review a specific change
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_review(
    state: Arc<DesktopStateManager>,
    _request: ReviewChangeRequest,
) -> CommandResult<ChangeDetail> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::not_found("Change not found")
}

/// Accept a change (update effective contract)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_accept(
    state: Arc<DesktopStateManager>,
    _request: AcceptChangeRequest,
) -> CommandResult<()> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(())
}

/// Reject a change (keep current contract)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_reject(
    state: Arc<DesktopStateManager>,
    _request: RejectChangeRequest,
) -> CommandResult<()> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(())
}

/// Accept all changes
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_accept_all(state: Arc<DesktopStateManager>) -> CommandResult<usize> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(0)
}

/// Keep current contract (reject all changes)
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn change_keep_current(state: Arc<DesktopStateManager>) -> CommandResult<usize> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(0)
}
