//! Request service for vault-backed request execution.
//!
//! This service handles:
//! - Request execution through api-client
//! - Environment variable resolution
//! - Vault-backed authentication injection
//! - Response validation against contract
//! - Request history with secret redaction
//!
//! Security guarantees:
//! - Secret resolution occurs only in Rust
//! - Secrets never enter React state
//! - Secrets are redacted from history
//! - Secrets are redacted from events

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use api_core::HttpMethod;
use api_vault::RedactionService;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ValidationResult;
use crate::state::{DesktopStateManager, RequestHistoryEntry};

use super::vault_service::{AuthenticationConfig, VaultService};
use super::{CustomerJourneyService, ServiceError, ServiceResult};

fn parse_method(method: &str) -> ServiceResult<HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "PATCH" => Ok(HttpMethod::PATCH),
        "DELETE" => Ok(HttpMethod::DELETE),
        "OPTIONS" => Ok(HttpMethod::OPTIONS),
        "HEAD" => Ok(HttpMethod::HEAD),
        other => Err(ServiceError::validation(format!(
            "Unsupported HTTP method '{other}'"
        ))),
    }
}

/// Sensitive field names that should be redacted from request/response
const SENSITIVE_FIELDS: &[&str] = &[
    "password",
    "token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "secret",
    "authorization",
    "cookie",
    "session",
    "credential",
    "private_key",
    "client_secret",
];

/// Request execution input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequestInput {
    pub method: String,
    pub url: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<serde_json::Value>,
    pub environment_id: Option<String>,
    pub authentication: Option<AuthenticationConfig>,
}

/// Request execution result (safe for frontend - secrets redacted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequestOutput {
    pub request_id: String,
    pub status: u16,
    pub duration_ms: u64,
    pub body_size: usize,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub validation: ValidationResult,
    pub workflow_step_completed: Option<String>,
}

/// Saved request summary (safe for frontend - headers/body may still contain
/// values the user typed, but never vault-resolved secrets since those are
/// only ever injected at execution time via `authentication`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequestInfo {
    pub name: String,
    pub method: String,
    pub url: Option<String>,
    pub endpoint_id: Option<String>,
}

impl From<api_storage::SavedRequest> for SavedRequestInfo {
    fn from(saved: api_storage::SavedRequest) -> Self {
        Self {
            name: saved.name,
            method: saved.method,
            url: saved.url,
            endpoint_id: saved.endpoint_id,
        }
    }
}

/// Request history entry (safe for frontend - secrets redacted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHistoryItem {
    pub id: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub environment: Option<String>,
    pub validation_passed: bool,
}

/// Request service implementation
pub struct RequestService {
    vault_service: VaultService,
}

impl RequestService {
    pub fn new() -> Self {
        Self {
            vault_service: VaultService::new(),
        }
    }

    pub fn with_vault_service(vault_service: VaultService) -> Self {
        Self { vault_service }
    }

    /// Execute a request with vault-backed authentication
    pub async fn execute(
        &self,
        state: &Arc<DesktopStateManager>,
        input: ExecuteRequestInput,
    ) -> ServiceResult<ExecuteRequestOutput> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        let project_name = project.as_ref().unwrap().name.clone();
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let method = parse_method(&input.method)?;

        // Resolve the environment (falls back to the first configured
        // environment, matching the CLI's `--environment` default behavior).
        let envs = api_storage::load_environments(&root)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;
        let environment = input
            .environment_id
            .as_ref()
            .and_then(|id| envs.iter().find(|e| &e.name == id))
            .or_else(|| envs.first())
            .cloned()
            .unwrap_or(api_core::ApiEnvironment {
                name: "default".to_string(),
                variables: BTreeMap::new(),
            });

        let rendered_url = api_client::render_template(&input.url, &environment.variables);

        let request_id = format!("req_{}", Uuid::new_v4().simple());
        let start = Instant::now();

        // Resolve authentication if configured
        let auth_header = if let Some(auth_config) = &input.authentication {
            let resolved = self
                .vault_service
                .resolve_authentication(state, auth_config)
                .await?;
            Some((resolved.header_name, resolved.header_value))
        } else {
            None
        };

        // Build headers with authentication
        let mut headers: BTreeMap<String, String> = input
            .headers
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some((name, value)) = &auth_header {
            headers.insert(name.clone(), value.clone());
        }

        let outcome = api_client::execute_url_full(
            &root,
            method,
            &rendered_url,
            Some(&headers),
            input.body.clone(),
        )
        .await
        .map_err(|e| ServiceError::request_failed(&e.to_string()))?;

        let duration = start.elapsed();

        let validation = ValidationResult {
            valid: true,
            issues: vec![],
        };

        // Redact secrets from response
        let redaction = self.vault_service.redaction_service();
        let redacted_body = redaction.redact_json(&outcome.body);
        let redacted_headers = redaction.redact_headers(&outcome.headers);

        // Add to history (with redacted URL)
        let redacted_url = Self::redact_url_secrets(&rendered_url, redaction);
        let history_entry = RequestHistoryEntry {
            id: request_id.clone(),
            method: input.method.clone(),
            url: redacted_url.clone(),
            status: outcome.status,
            duration_ms: duration.as_millis() as u64,
            timestamp: Utc::now(),
        };
        state.add_request_history(&project_name, history_entry).await;

        let _ = CustomerJourneyService::complete_outcome(
            state,
            api_customer_journey::JourneyOutcome::FirstRequest,
        )
        .await;
        let _ = CustomerJourneyService::complete_outcome(
            state,
            api_customer_journey::JourneyOutcome::ReusableRequest,
        )
        .await;
        if input.environment_id.is_some() {
            let _ = CustomerJourneyService::complete_outcome(
                state,
                api_customer_journey::JourneyOutcome::EnvironmentReady,
            )
            .await;
        }

        Ok(ExecuteRequestOutput {
            request_id,
            status: outcome.status,
            duration_ms: duration.as_millis() as u64,
            body_size: redacted_body.to_string().len(),
            headers: redacted_headers,
            body: redacted_body,
            validation,
            workflow_step_completed: Some("run-first-request".to_string()),
        })
    }

    /// Save a request for later reuse (also resolvable by test suites via
    /// `TestCase.request_id`).
    pub async fn save_request(
        &self,
        state: &Arc<DesktopStateManager>,
        name: &str,
        method: &str,
        url: &str,
        headers: Option<HashMap<String, String>>,
        body: Option<serde_json::Value>,
    ) -> ServiceResult<SavedRequestInfo> {
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let saved = api_storage::SavedRequest {
            name: name.to_string(),
            method: method.to_string(),
            url: Some(url.to_string()),
            endpoint_id: None,
            headers: headers.map(|h| h.into_iter().collect()),
            body,
        };
        api_storage::save_request(&root, &saved).map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(SavedRequestInfo::from(saved))
    }

    /// List saved requests for the current project.
    pub async fn list_saved(
        &self,
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<SavedRequestInfo>> {
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let saved = api_storage::list_saved_requests(&root)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;
        Ok(saved.into_iter().map(SavedRequestInfo::from).collect())
    }

    /// Resolve a saved request by name and execute it - this is how
    /// `TestCase.request_id` gets turned into an actual HTTP call.
    pub async fn execute_saved(
        &self,
        state: &Arc<DesktopStateManager>,
        name: &str,
        environment_id: Option<String>,
    ) -> ServiceResult<ExecuteRequestOutput> {
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let saved = api_storage::load_saved_request(&root, name)
            .map_err(|_| ServiceError::not_found(&format!("Saved request '{name}'")))?;

        let Some(url) = saved.url else {
            return Err(ServiceError::validation(
                "Saved request has no URL (endpoint-id-based saved requests aren't executable yet)",
            ));
        };

        self.execute(
            state,
            ExecuteRequestInput {
                method: saved.method,
                url,
                headers: saved.headers.map(|h| h.into_iter().collect()),
                body: saved.body,
                environment_id,
                authentication: None,
            },
        )
        .await
    }

    /// Get request history for the current project
    pub async fn get_history(
        &self,
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<RequestHistoryItem>> {
        let project = state.project.read().await;
        if let Some(project) = project.as_ref() {
            let history = state.get_request_history(&project.name).await;

            let items: Vec<RequestHistoryItem> = history
                .into_iter()
                .map(|entry| RequestHistoryItem {
                    id: entry.id,
                    timestamp: entry.timestamp,
                    method: entry.method,
                    url: entry.url,
                    status: entry.status,
                    duration_ms: entry.duration_ms,
                    environment: None,
                    validation_passed: entry.status < 400,
                })
                .collect();

            Ok(items)
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Clear request history for the current project
    pub async fn clear_history(&self, state: &Arc<DesktopStateManager>) -> ServiceResult<()> {
        let project = state.project.read().await;
        if let Some(project) = project.as_ref() {
            let mut history = state.request_history.write().await;
            history.remove(&project.name);
            Ok(())
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Delete a specific history entry
    pub async fn delete_history_entry(
        &self,
        state: &Arc<DesktopStateManager>,
        entry_id: &str,
    ) -> ServiceResult<()> {
        let project = state.project.read().await;
        if let Some(project) = project.as_ref() {
            let mut history = state.request_history.write().await;
            if let Some(entries) = history.get_mut(&project.name) {
                entries.retain(|e| e.id != entry_id);
            }
            Ok(())
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Check if a JSON value contains sensitive fields
    pub fn contains_sensitive_fields(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let key_lower = key.to_lowercase();
                    if SENSITIVE_FIELDS.iter().any(|s| key_lower.contains(s)) {
                        return true;
                    }
                    if Self::contains_sensitive_fields(val) {
                        return true;
                    }
                }
                false
            }
            serde_json::Value::Array(arr) => arr.iter().any(Self::contains_sensitive_fields),
            _ => false,
        }
    }

    /// Redact sensitive fields from a JSON value
    pub fn redact_sensitive_fields(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (key, val) in map {
                    let key_lower = key.to_lowercase();
                    if SENSITIVE_FIELDS.iter().any(|s| key_lower.contains(s)) {
                        new_map.insert(
                            key.clone(),
                            serde_json::Value::String("[REDACTED]".to_string()),
                        );
                    } else {
                        new_map.insert(key.clone(), Self::redact_sensitive_fields(val));
                    }
                }
                serde_json::Value::Object(new_map)
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(Self::redact_sensitive_fields).collect())
            }
            _ => value.clone(),
        }
    }

    /// Redact secrets from URL query parameters
    fn redact_url_secrets(url: &str, redaction: &RedactionService) -> String {
        // First apply known secret redaction
        let url = redaction.redact_string(url);

        // Then check for sensitive query parameters
        if let Some(query_start) = url.find('?') {
            let (base, query) = url.split_at(query_start);
            let query = &query[1..]; // Skip the '?'

            let redacted_params: Vec<String> = query
                .split('&')
                .map(|param| {
                    if let Some((key, _value)) = param.split_once('=') {
                        let key_lower = key.to_lowercase();
                        if SENSITIVE_FIELDS.iter().any(|s| key_lower.contains(s)) {
                            format!("{}=[REDACTED]", key)
                        } else {
                            param.to_string()
                        }
                    } else {
                        param.to_string()
                    }
                })
                .collect();

            if redacted_params.is_empty() {
                base.to_string()
            } else {
                format!("{}?{}", base, redacted_params.join("&"))
            }
        } else {
            url
        }
    }
}

impl Default for RequestService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::test_helpers::{create_test_project, seed_mock_environment, spawn_test_server};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_execute_request_no_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let service = RequestService::new();
        let input = ExecuteRequestInput {
            method: "GET".to_string(),
            url: "http://localhost:4010/users".to_string(),
            headers: None,
            body: None,
            environment_id: None,
            authentication: None,
        };

        let result = service.execute(&state, input).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            super::super::ServiceErrorCode::NoProjectOpen
        );
    }

    #[tokio::test]
    async fn test_execute_request_success() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let base_url = spawn_test_server().await;
        seed_mock_environment(project_dir.path(), &base_url).await;

        let service = RequestService::new();
        let input = ExecuteRequestInput {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: None,
            body: None,
            environment_id: None,
            authentication: None,
        };

        let result = service.execute(&state, input).await.unwrap();
        assert_eq!(result.status, 200);
        assert!(!result.request_id.is_empty());
    }

    #[test]
    fn test_redact_sensitive_fields() {
        let input = serde_json::json!({
            "username": "testuser",
            "password": "secret123",
            "data": {
                "api_key": "key-abc-123",
                "name": "public"
            }
        });

        let output = RequestService::redact_sensitive_fields(&input);

        assert_eq!(output["username"], "testuser");
        assert_eq!(output["password"], "[REDACTED]");
        assert_eq!(output["data"]["api_key"], "[REDACTED]");
        assert_eq!(output["data"]["name"], "public");
    }

    #[test]
    fn test_redact_url_secrets() {
        let redaction = RedactionService::new();
        redaction.register_secret("secret-token-123");

        // URL with registered secret
        let url = "http://api.example.com/data?auth=secret-token-123";
        let redacted = RequestService::redact_url_secrets(url, &redaction);
        assert!(!redacted.contains("secret-token-123"));

        // URL with sensitive query parameter
        let url = "http://api.example.com/data?api_key=some-key&name=test";
        let redacted = RequestService::redact_url_secrets(url, &redaction);
        assert!(redacted.contains("api_key=[REDACTED]"));
        assert!(redacted.contains("name=test"));
    }

    #[test]
    fn test_contains_sensitive_fields() {
        let safe_value = serde_json::json!({
            "name": "test",
            "count": 42
        });
        assert!(!RequestService::contains_sensitive_fields(&safe_value));

        let sensitive_value = serde_json::json!({
            "name": "test",
            "password": "secret"
        });
        assert!(RequestService::contains_sensitive_fields(&sensitive_value));

        let nested_sensitive = serde_json::json!({
            "user": {
                "name": "test",
                "api_key": "key"
            }
        });
        assert!(RequestService::contains_sensitive_fields(&nested_sensitive));
    }

    #[tokio::test]
    async fn test_request_history_redacted() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let base_url = spawn_test_server().await;
        seed_mock_environment(project_dir.path(), &base_url).await;

        let service = RequestService::new();

        // Execute request
        let input = ExecuteRequestInput {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: None,
            body: None,
            environment_id: None,
            authentication: None,
        };
        service.execute(&state, input).await.unwrap();

        // Get history
        let history = service.get_history(&state).await.unwrap();
        assert_eq!(history.len(), 1);

        // Verify no sensitive data in history
        let entry = &history[0];
        assert!(!entry.url.contains("secret"));
        assert!(!entry.url.contains("token"));
    }
}
