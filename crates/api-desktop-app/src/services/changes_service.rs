//! Changes service for contract change review.
//!
//! This service handles:
//! - Contract change detection
//! - Change classification (breaking/non-breaking)
//! - Accept/reject workflow
//! - Effective contract updates

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ContractChangeSummary;
use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

/// Change classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClassification {
    Added,
    Removed,
    Modified,
    Breaking,
    NonBreaking,
    Uncertain,
}

/// Change detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeDetail {
    pub id: String,
    pub classification: ChangeClassification,
    pub description: String,
    pub path: Option<String>,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub is_breaking: bool,
    pub impact_analysis: String,
    pub suggested_action: String,
}

/// Change review decision
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeDecision {
    Accept,
    Reject,
    Defer,
}

/// Change review result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeReviewResult {
    pub change_id: String,
    pub decision: ChangeDecision,
    pub effective_contract_updated: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Changes service implementation
pub struct ChangesService;

impl ChangesService {
    /// List all pending contract changes
    pub async fn list(state: &Arc<DesktopStateManager>) -> ServiceResult<ContractChangeSummary> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        // Try to load generated and effective contracts to compare
        let _generated = api_storage::load_effective_contract(root);

        // In production, this would compare generated vs effective
        // and produce a diff
        Ok(ContractChangeSummary {
            total_changes: 0,
            added: Vec::new(),
            modified: Vec::new(),
            removed: Vec::new(),
            potentially_breaking: Vec::new(),
        })
    }

    /// Get change detail
    pub async fn get(
        state: &Arc<DesktopStateManager>,
        _change_id: &str,
    ) -> ServiceResult<ChangeDetail> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would load the specific change
        Err(ServiceError::not_found("Change"))
    }

    /// Accept a change (update effective contract)
    pub async fn accept(
        state: &Arc<DesktopStateManager>,
        change_id: &str,
    ) -> ServiceResult<ChangeReviewResult> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would:
        // 1. Apply the change to effective contract
        // 2. Persist the decision
        // 3. Emit workflow event

        Ok(ChangeReviewResult {
            change_id: change_id.to_string(),
            decision: ChangeDecision::Accept,
            effective_contract_updated: true,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Reject a change (keep current contract)
    pub async fn reject(
        state: &Arc<DesktopStateManager>,
        change_id: &str,
        _reason: Option<&str>,
    ) -> ServiceResult<ChangeReviewResult> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would:
        // 1. Record the rejection with reason
        // 2. Persist the decision
        // 3. Possibly mark for future review

        Ok(ChangeReviewResult {
            change_id: change_id.to_string(),
            decision: ChangeDecision::Reject,
            effective_contract_updated: false,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Accept all pending changes
    pub async fn accept_all(state: &Arc<DesktopStateManager>) -> ServiceResult<usize> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        // In production, this would:
        // 1. Replace effective with generated
        // 2. Clear pending changes
        // 3. Emit workflow event

        // Try to copy generated to effective if it exists
        let generated_path = root.join(".repo-api/contract/generated.json");
        let effective_path = root.join(".repo-api/contract/effective.json");

        if generated_path.exists() {
            std::fs::copy(&generated_path, &effective_path)
                .map_err(|e| ServiceError::internal(&e.to_string()))?;
        }

        Ok(0)
    }

    /// Keep current contract (reject all changes)
    pub async fn keep_current(state: &Arc<DesktopStateManager>) -> ServiceResult<usize> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would clear all pending changes
        // without updating effective contract

        Ok(0)
    }

    /// Classify a change as breaking or non-breaking
    pub fn classify_change(
        before: &serde_json::Value,
        after: &serde_json::Value,
    ) -> ChangeClassification {
        // Simple classification logic
        // In production, this would use semantic analysis

        if before.is_null() && !after.is_null() {
            return ChangeClassification::Added;
        }

        if !before.is_null() && after.is_null() {
            return ChangeClassification::Removed;
        }

        // Check for potentially breaking changes
        // Removing required fields, changing types, etc.
        if let (Some(before_obj), Some(after_obj)) = (before.as_object(), after.as_object()) {
            // Check for removed required fields
            if let (Some(before_required), Some(after_required)) = (
                before_obj.get("required").and_then(|v| v.as_array()),
                after_obj.get("required").and_then(|v| v.as_array()),
            ) {
                if before_required.len() > after_required.len() {
                    return ChangeClassification::NonBreaking; // Removing required is non-breaking
                }
                if before_required.len() < after_required.len() {
                    return ChangeClassification::Breaking; // Adding required is breaking
                }
            }

            // Check for type changes
            if before_obj.get("type") != after_obj.get("type") {
                return ChangeClassification::Breaking;
            }
        }

        ChangeClassification::Modified
    }

    /// Analyze impact of a change
    pub fn analyze_impact(change: &ChangeDetail) -> String {
        match change.classification {
            ChangeClassification::Breaking => {
                "This change may break existing clients. Review carefully before accepting."
                    .to_string()
            }
            ChangeClassification::Removed => {
                "Removing this element may break clients that depend on it.".to_string()
            }
            ChangeClassification::Added => {
                "Adding new elements is generally safe for existing clients.".to_string()
            }
            ChangeClassification::NonBreaking => {
                "This change should be safe for existing clients.".to_string()
            }
            ChangeClassification::Modified => {
                "Review the specific changes to determine impact.".to_string()
            }
            ChangeClassification::Uncertain => {
                "Unable to determine impact automatically. Manual review recommended.".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_list_changes_no_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let result = ChangesService::list(&state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_accept_change() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let result = ChangesService::accept(&state, "change-1").await.unwrap();
        assert_eq!(result.decision, ChangeDecision::Accept);
    }

    #[test]
    fn test_classify_addition() {
        let before = json!(null);
        let after = json!({"type": "string"});

        let classification = ChangesService::classify_change(&before, &after);
        assert_eq!(classification, ChangeClassification::Added);
    }

    #[test]
    fn test_classify_removal() {
        let before = json!({"type": "string"});
        let after = json!(null);

        let classification = ChangesService::classify_change(&before, &after);
        assert_eq!(classification, ChangeClassification::Removed);
    }

    #[test]
    fn test_classify_breaking_type_change() {
        let before = json!({"type": "string"});
        let after = json!({"type": "number"});

        let classification = ChangesService::classify_change(&before, &after);
        assert_eq!(classification, ChangeClassification::Breaking);
    }

    #[test]
    fn test_classify_breaking_new_required() {
        let before = json!({"required": ["name"]});
        let after = json!({"required": ["name", "email"]});

        let classification = ChangesService::classify_change(&before, &after);
        assert_eq!(classification, ChangeClassification::Breaking);
    }

    #[test]
    fn test_classify_non_breaking_remove_required() {
        let before = json!({"required": ["name", "email"]});
        let after = json!({"required": ["name"]});

        let classification = ChangesService::classify_change(&before, &after);
        assert_eq!(classification, ChangeClassification::NonBreaking);
    }
}
