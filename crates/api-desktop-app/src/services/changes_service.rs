//! Changes service for contract change review.
//!
//! This service handles:
//! - Contract change detection
//! - Change classification (breaking/non-breaking)
//! - Accept/reject workflow
//! - Effective contract updates

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use api_compiler::DiffKind;
use api_core::ApiContract;
use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;
use crate::{ChangeEntry, ContractChangeSummary};

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
    /// Load the generated (freshly discovered) contract, if one exists.
    fn load_generated(root: &Path) -> Option<ApiContract> {
        let bytes = std::fs::read(root.join(".repo-api/contract/generated.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Diff the effective (current/accepted) contract against the generated
    /// (freshly discovered) one, grouping the resulting `DiffKind`s by
    /// endpoint key. Returns `(generated, effective, by_key)`; `by_key` is
    /// empty if there's no generated contract to compare against yet.
    fn diff(
        root: &Path,
    ) -> ServiceResult<(ApiContract, ApiContract, BTreeMap<String, Vec<DiffKind>>)> {
        let effective = api_storage::load_effective_contract(root)
            .map_err(|_| ServiceError::not_found("Effective contract"))?;

        let Some(generated) = Self::load_generated(root) else {
            return Ok((effective.clone(), effective, BTreeMap::new()));
        };

        let mut by_key: BTreeMap<String, Vec<DiffKind>> = BTreeMap::new();
        for (key, kind) in api_compiler::diff_contracts(&effective, &generated) {
            if key == "no-change" {
                continue;
            }
            by_key.entry(key).or_default().push(kind);
        }

        Ok((generated, effective, by_key))
    }

    fn primary_classification(kinds: &[DiffKind]) -> ChangeClassification {
        if kinds.contains(&DiffKind::Removed) {
            ChangeClassification::Removed
        } else if kinds.contains(&DiffKind::Added) {
            ChangeClassification::Added
        } else if kinds.contains(&DiffKind::Modified) {
            ChangeClassification::Modified
        } else {
            ChangeClassification::Uncertain
        }
    }

    /// List all pending contract changes (diff of effective vs. generated)
    pub async fn list(state: &Arc<DesktopStateManager>) -> ServiceResult<ContractChangeSummary> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (_, _, by_key) = Self::diff(&root)?;

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut removed = Vec::new();
        let mut potentially_breaking = Vec::new();

        for (key, kinds) in &by_key {
            let classification = Self::primary_classification(kinds);
            let entry = ChangeEntry {
                kind: format!("{:?}", classification).to_lowercase(),
                description: format!("{:?} endpoint: {}", classification, key),
                path: Some(key.clone()),
            };
            match classification {
                ChangeClassification::Added => added.push(entry.clone()),
                ChangeClassification::Removed => removed.push(entry.clone()),
                ChangeClassification::Modified => modified.push(entry.clone()),
                _ => {}
            }
            if kinds.contains(&DiffKind::Breaking) {
                potentially_breaking.push(entry);
            }
        }

        Ok(ContractChangeSummary {
            total_changes: by_key.len(),
            added,
            modified,
            removed,
            potentially_breaking,
        })
    }

    /// Get change detail for a single endpoint key (as produced by `list`)
    pub async fn get(
        state: &Arc<DesktopStateManager>,
        change_id: &str,
    ) -> ServiceResult<ChangeDetail> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (generated, effective, by_key) = Self::diff(&root)?;
        let kinds = by_key
            .get(change_id)
            .ok_or_else(|| ServiceError::not_found(&format!("Change '{change_id}'")))?;

        let before = effective
            .endpoints
            .iter()
            .find(|e| api_compiler::endpoint_key(&e.method, &e.path) == change_id)
            .and_then(|e| serde_json::to_value(e).ok());
        let after = generated
            .endpoints
            .iter()
            .find(|e| api_compiler::endpoint_key(&e.method, &e.path) == change_id)
            .and_then(|e| serde_json::to_value(e).ok());

        let classification = Self::primary_classification(kinds);
        let is_breaking = kinds.contains(&DiffKind::Breaking);

        let mut detail = ChangeDetail {
            id: change_id.to_string(),
            classification,
            description: format!("{:?} endpoint: {}", classification, change_id),
            path: Some(change_id.to_string()),
            before,
            after,
            is_breaking,
            impact_analysis: String::new(),
            suggested_action: if is_breaking {
                "Review carefully before accepting".to_string()
            } else {
                "Safe to accept".to_string()
            },
        };
        detail.impact_analysis = Self::analyze_impact(&detail);
        Ok(detail)
    }

    /// Accept a change: apply that endpoint's generated definition into the
    /// effective contract (or remove it, for a `Removed` change).
    pub async fn accept(
        state: &Arc<DesktopStateManager>,
        change_id: &str,
    ) -> ServiceResult<ChangeReviewResult> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (generated, mut effective, by_key) = Self::diff(&root)?;
        let kinds = by_key
            .get(change_id)
            .ok_or_else(|| ServiceError::not_found(&format!("Change '{change_id}'")))?;

        if kinds.contains(&DiffKind::Removed) {
            effective
                .endpoints
                .retain(|e| api_compiler::endpoint_key(&e.method, &e.path) != change_id);
        } else if let Some(new_endpoint) = generated
            .endpoints
            .iter()
            .find(|e| api_compiler::endpoint_key(&e.method, &e.path) == change_id)
        {
            effective
                .endpoints
                .retain(|e| api_compiler::endpoint_key(&e.method, &e.path) != change_id);
            effective.endpoints.push(new_endpoint.clone());
        }

        api_storage::save_effective_contract(&root, &effective)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(ChangeReviewResult {
            change_id: change_id.to_string(),
            decision: ChangeDecision::Accept,
            effective_contract_updated: true,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Reject a change (leave the effective contract as-is)
    pub async fn reject(
        state: &Arc<DesktopStateManager>,
        change_id: &str,
        _reason: Option<&str>,
    ) -> ServiceResult<ChangeReviewResult> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (_, _, by_key) = Self::diff(&root)?;
        if !by_key.contains_key(change_id) {
            return Err(ServiceError::not_found(&format!("Change '{change_id}'")));
        }

        Ok(ChangeReviewResult {
            change_id: change_id.to_string(),
            decision: ChangeDecision::Reject,
            effective_contract_updated: false,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Accept all pending changes (replace effective with generated wholesale)
    pub async fn accept_all(state: &Arc<DesktopStateManager>) -> ServiceResult<usize> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (generated, _, by_key) = Self::diff(&root)?;
        let count = by_key.len();

        if count > 0 {
            api_storage::save_effective_contract(&root, &generated)
                .map_err(|e| ServiceError::internal(&e.to_string()))?;
        }

        Ok(count)
    }

    /// Keep current contract (reject all changes) - a no-op on the effective
    /// contract, returning how many pending changes were dismissed.
    pub async fn keep_current(state: &Arc<DesktopStateManager>) -> ServiceResult<usize> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let (_, _, by_key) = Self::diff(&root)?;
        Ok(by_key.len())
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

    fn contract_with_endpoint(path: &str) -> ApiContract {
        use api_core::{
            ApiEndpoint, ApiMetadata, Confidence, ConfidenceLevel, EvidenceIndex, HttpMethod,
            SchemaRegistry, SecurityRequirement,
        };

        ApiContract {
            version: "1".into(),
            metadata: ApiMetadata {
                title: "t".into(),
                version: "1".into(),
                repository_root: None,
            },
            servers: vec![],
            endpoints: vec![ApiEndpoint {
                id: format!("ep-{path}"),
                operation_id: None,
                method: HttpMethod::GET,
                path: path.to_string(),
                summary: None,
                parameters: vec![],
                request_bodies: vec![],
                responses: vec![],
                security: SecurityRequirement { schemes: vec![] },
                confidence: Confidence {
                    level: ConfidenceLevel::High,
                    score: 1.0,
                },
                evidence: vec![],
            }],
            schemas: SchemaRegistry::default(),
            security_schemes: vec![],
            diagnostics: vec![],
            evidence: EvidenceIndex {
                endpoint_evidence: vec![],
                schema_evidence: vec![],
                security_evidence: vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_list_and_accept_real_diff() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        // Effective (current) has /users; generated (freshly scanned) adds /orders.
        let effective = contract_with_endpoint("/users");
        let mut generated = effective.clone();
        generated
            .endpoints
            .push(contract_with_endpoint("/orders").endpoints.remove(0));

        api_storage::save_effective_contract(project_dir.path(), &effective).unwrap();
        api_storage::save_generated_contract(project_dir.path(), &generated).unwrap();

        let summary = ChangesService::list(&state).await.unwrap();
        assert_eq!(summary.total_changes, 1);
        assert_eq!(summary.added.len(), 1);
        assert!(summary.added[0].path.as_deref().unwrap().contains("/orders"));

        let change_id = summary.added[0].path.clone().unwrap();
        let result = ChangesService::accept(&state, &change_id).await.unwrap();
        assert_eq!(result.decision, ChangeDecision::Accept);

        // Effective contract should now include /orders too.
        let updated = api_storage::load_effective_contract(project_dir.path()).unwrap();
        assert_eq!(updated.endpoints.len(), 2);

        // And the change should no longer show up as pending.
        let summary = ChangesService::list(&state).await.unwrap();
        assert_eq!(summary.total_changes, 0);
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
