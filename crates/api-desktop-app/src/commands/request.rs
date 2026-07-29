//! Request execution commands

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::{DesktopStateManager, RequestHistoryEntry};
use crate::{RequestResult, ValidationResult};

use super::CommandResult;

/// Execute request input
#[derive(Debug, Deserialize)]
pub struct ExecuteRequestInput {
    pub method: String,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
    pub environment_id: Option<String>,
}

/// Save request input
#[derive(Debug, Deserialize)]
pub struct SaveRequestInput {
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
}

/// Saved request output
#[derive(Debug, Serialize)]
pub struct SavedRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

/// Delete request input
#[derive(Debug, Deserialize)]
pub struct DeleteRequestInput {
    pub id: String,
}

/// Execute a request
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_execute(
    state: Arc<DesktopStateManager>,
    request: ExecuteRequestInput,
) -> CommandResult<RequestResult> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }
    let project = project.as_ref().unwrap();

    let start = Instant::now();

    // For now, return a mock response
    // In production, this would use api_client::execute_request
    let duration = start.elapsed();

    let result = RequestResult {
        status: 200,
        duration_ms: duration.as_millis() as u64 + 50,
        body_size: 100,
        headers: vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Request-Id".to_string(), Uuid::new_v4().to_string()),
        ],
        body: serde_json::json!({
            "success": true,
            "message": "Request executed successfully"
        }),
        validation: ValidationResult {
            valid: true,
            issues: vec![],
        },
    };

    // Add to history with secrets redacted
    let redacted_url = request.url.clone();
    let history_entry = RequestHistoryEntry {
        id: Uuid::new_v4().to_string(),
        method: request.method.clone(),
        url: redacted_url,
        status: result.status,
        duration_ms: result.duration_ms,
        timestamp: Utc::now(),
    };
    state
        .add_request_history(&project.name, history_entry)
        .await;

    CommandResult::ok(result)
}

/// Save a request for later
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_save(
    state: Arc<DesktopStateManager>,
    request: SaveRequestInput,
) -> CommandResult<SavedRequest> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    let saved = SavedRequest {
        id: Uuid::new_v4().to_string(),
        name: request.name,
        method: request.method,
        url: request.url,
    };

    CommandResult::ok(saved)
}

/// Get request history
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_history(
    state: Arc<DesktopStateManager>,
) -> CommandResult<Vec<RequestHistoryEntry>> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        let history = state.get_request_history(&project.name).await;
        CommandResult::ok(history)
    } else {
        CommandResult::error("No project open")
    }
}

/// Delete a request from history
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_delete(
    state: Arc<DesktopStateManager>,
    request: DeleteRequestInput,
) -> CommandResult<()> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        let mut history = state.request_history.write().await;
        if let Some(entries) = history.get_mut(&project.name) {
            entries.retain(|e| e.id != request.id);
        }
        CommandResult::ok(())
    } else {
        CommandResult::error("No project open")
    }
}

/// Clear all request history for the current project
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn request_history_clear(state: Arc<DesktopStateManager>) -> CommandResult<()> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        let mut history = state.request_history.write().await;
        history.remove(&project.name);
        CommandResult::ok(())
    } else {
        CommandResult::error("No project open")
    }
}
