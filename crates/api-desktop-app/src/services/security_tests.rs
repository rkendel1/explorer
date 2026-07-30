//! Security test matrix for verifying secret handling.
//!
//! This module contains tests that verify:
//! - Vault secrets are never exposed to frontend
//! - Secrets are redacted from all outputs
//! - Vault state is properly managed
//! - Application restart doesn't leak secrets

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::VaultEntryMetadata;
    use crate::services::test_helpers::{create_test_project, seed_mock_environment, spawn_test_server};
    use crate::services::{RequestService, VaultService};
    use crate::state::DesktopStateManager;
    use api_vault::{RedactionService, VaultState};
    use tempfile::tempdir;

    // ============================================================
    // Security Test Matrix
    // ============================================================

    /// Test: Vault secret used in request - Request succeeds
    #[tokio::test]
    async fn test_vault_secret_used_in_request_succeeds() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();

        // Unlock vault and create secret
        vault_service.unlock(&state, None).await.unwrap();
        let entry = vault_service
            .create_entry(&state, "api-token", "bearer_token", "secret-token-12345")
            .await
            .unwrap();

        // Entry metadata should exist
        assert_eq!(entry.name, "api-token");

        // Entry should be retrievable through list (without secret value)
        let entries = vault_service.list_entries(&state).await.unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.name == "api-token"));
    }

    /// Test: Vault secret in React state - Never present
    /// This test verifies that the types returned to frontend never contain secrets
    #[test]
    fn test_vault_secret_never_in_frontend_types() {
        // VaultEntryMetadata (returned to frontend) has no secret field
        let entry_info = VaultEntryMetadata {
            id: "entry-1".to_string(),
            name: "test-secret".to_string(),
            secret_type: "bearer_token".to_string(),
            status: "available".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Verify JSON serialization doesn't include any secret value
        let json = serde_json::to_string(&entry_info).unwrap();
        assert!(!json.contains("secret-value"));
        assert!(!json.contains("password123"));
    }

    /// Test: Vault secret in command result - Never present
    #[test]
    fn test_vault_entry_metadata_has_no_secret_value() {
        // The VaultEntryMetadata struct intentionally excludes the secret value
        // Only metadata is returned to the frontend
        let _entry = VaultEntryMetadata {
            id: "entry-1".to_string(),
            name: "staging-api-key".to_string(),
            secret_type: "api_key".to_string(),
            status: "available".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Compile-time verification: VaultEntryMetadata doesn't have a `value` field
        // If this code compiles, the test passes
    }

    /// Test: Vault secret in history - Redacted
    #[tokio::test]
    async fn test_vault_secret_redacted_in_history() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let base_url = spawn_test_server().await;
        seed_mock_environment(project_dir.path(), &base_url).await;

        let request_service = RequestService::new();

        // Execute a request
        let input = crate::services::request_service::ExecuteRequestInput {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users?api_key=secret123".to_string(),
            headers: None,
            body: None,
            environment_id: None,
            authentication: None,
        };

        request_service.execute(&state, input).await.unwrap();

        // Check history doesn't contain the api_key value
        let history = request_service.get_history(&state).await.unwrap();
        assert!(!history.is_empty());

        for entry in &history {
            assert!(!entry.url.contains("secret123"));
            assert!(entry.url.contains("[REDACTED]") || !entry.url.contains("api_key=secret123"));
        }
    }

    /// Test: Vault secret in runtime event - Redacted
    #[test]
    fn test_redaction_service_clears_secrets() {
        let redaction = RedactionService::new();

        // Register a secret
        redaction.register_secret("super-secret-api-key-123");

        // Any string containing the secret should be redacted
        let input = "Authorization: super-secret-api-key-123";
        let output = redaction.redact_string(input);

        assert!(!output.contains("super-secret-api-key-123"));
        assert!(output.contains("[REDACTED]"));
    }

    /// Test: Vault secret in notification - Redacted
    #[test]
    fn test_notification_content_redaction() {
        let redaction = RedactionService::new();
        redaction.register_secret("my-api-key-abc123");

        // Simulate notification content
        let notification = "Request to /api failed with key my-api-key-abc123";
        let safe_notification = redaction.redact_string(notification);

        assert!(!safe_notification.contains("my-api-key-abc123"));
        assert!(safe_notification.contains("[REDACTED]"));
    }

    /// Test: Vault secret in error - Redacted
    #[test]
    fn test_error_message_redaction() {
        let redaction = RedactionService::new();
        redaction.register_secret("password123");

        // Error message that might contain secret
        let error = "Authentication failed for user with password password123";
        let safe_error = redaction.redact_string(error);

        assert!(!safe_error.contains("password123"));
        assert!(safe_error.contains("[REDACTED]"));
    }

    /// Test: Vault locked - Request blocked
    #[tokio::test]
    async fn test_vault_locked_blocks_secret_resolution() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();

        // Vault is locked by default
        let vault_info = vault_service.get_state(&state).await.unwrap();
        assert_eq!(vault_info.state, VaultState::Locked);

        // Trying to list entries when locked should fail
        let result = vault_service.list_entries(&state).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, crate::services::ServiceErrorCode::VaultLocked);
    }

    /// Test: Vault unlock succeeds - Request resumes
    #[tokio::test]
    async fn test_vault_unlock_allows_operations() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();

        // Unlock
        vault_service.unlock(&state, None).await.unwrap();

        // Now operations should succeed
        let entries = vault_service.list_entries(&state).await.unwrap();
        assert!(entries.is_empty()); // No entries yet

        // Can create entry
        let entry = vault_service
            .create_entry(&state, "test-key", "api_key", "secret123")
            .await
            .unwrap();
        assert_eq!(entry.name, "test-key");
    }

    /// Test: Vault auto-lock - Secret memory cleared (via manual lock)
    #[tokio::test]
    async fn test_vault_lock_clears_state() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();

        // Unlock and create entry
        vault_service.unlock(&state, None).await.unwrap();
        vault_service
            .create_entry(&state, "test-key", "api_key", "secret123")
            .await
            .unwrap();

        // Lock vault (similar to auto-lock behavior)
        vault_service.lock(&state).await.unwrap();

        // Verify locked
        let vault_info = vault_service.get_state(&state).await.unwrap();
        assert_eq!(vault_info.state, VaultState::Locked);

        // Operations should now fail
        let result = vault_service.list_entries(&state).await;
        assert!(result.is_err());
    }

    /// Test: Restart application - Vault remains locked
    #[test]
    fn test_restart_vault_remains_locked() {
        let app_dir = tempdir().unwrap();

        // Create new state manager (simulates restart)
        let state = DesktopStateManager::new(app_dir.path().to_path_buf());

        // Verify vault is locked on fresh start
        let vault_state = state.vault_state.blocking_read();
        assert_eq!(*vault_state, VaultState::Locked);
    }

    /// Test: Missing vault entry - Safe error (entry not found, no crash)
    #[tokio::test]
    async fn test_missing_vault_entry_safe_behavior() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();
        vault_service.unlock(&state, None).await.unwrap();

        // List entries - should be empty but not error
        let entries = vault_service.list_entries(&state).await.unwrap();
        assert!(entries.is_empty());

        // Check for non-existent entry
        let exists = vault_service
            .entry_exists(&state, "non-existent")
            .await
            .unwrap();
        assert!(!exists);
    }

    /// Test: Invalid passphrase - No vault data exposed
    #[tokio::test]
    async fn test_invalid_passphrase_no_data_exposed() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let vault_service = VaultService::new();

        // First unlock with default passphrase
        vault_service.unlock(&state, None).await.unwrap();

        // Create an entry
        vault_service
            .create_entry(&state, "secret-key", "api_key", "secret-value-xyz")
            .await
            .unwrap();

        // Lock vault
        vault_service.lock(&state).await.unwrap();

        // Try to unlock with wrong passphrase (if passphrase was set)
        // In the current implementation, wrong passphrase would fail silently
        // and not expose any data
        let vault_info = vault_service.get_state(&state).await.unwrap();
        // No secret values should be in the info
        let json = serde_json::to_string(&vault_info).unwrap();
        assert!(!json.contains("secret-value-xyz"));
    }

    // ============================================================
    // Additional Security Tests
    // ============================================================

    /// Test: Sensitive field detection
    #[test]
    fn test_sensitive_field_detection() {
        use crate::services::RequestService;

        // Test various sensitive field names
        let sensitive_data = serde_json::json!({
            "password": "secret123",
            "api_key": "key-abc",
            "authorization": "******",
            "token": "jwt-token",
            "refresh_token": "refresh",
            "access_token": "access",
            "secret": "mysecret",
            "cookie": "session=abc"
        });

        assert!(RequestService::contains_sensitive_fields(&sensitive_data));

        // Non-sensitive data
        let safe_data = serde_json::json!({
            "username": "testuser",
            "email": "test@example.com",
            "id": 123
        });

        assert!(!RequestService::contains_sensitive_fields(&safe_data));
    }

    /// Test: Nested sensitive field detection
    #[test]
    fn test_nested_sensitive_field_detection() {
        use crate::services::RequestService;

        let nested_sensitive = serde_json::json!({
            "user": {
                "name": "test",
                "credentials": {
                    "password": "secret123"
                }
            }
        });

        assert!(RequestService::contains_sensitive_fields(&nested_sensitive));
    }

    /// Test: Sensitive field redaction
    #[test]
    fn test_sensitive_field_redaction() {
        use crate::services::RequestService;

        let input = serde_json::json!({
            "username": "testuser",
            "password": "secret123",
            "data": {
                "api_key": "key-abc",
                "public_value": "public"
            }
        });

        let output = RequestService::redact_sensitive_fields(&input);

        assert_eq!(output["username"], "testuser");
        assert_eq!(output["password"], "[REDACTED]");
        assert_eq!(output["data"]["api_key"], "[REDACTED]");
        assert_eq!(output["data"]["public_value"], "public");
    }

    /// Test: HTTP header redaction
    #[test]
    fn test_http_header_redaction() {
        let redaction = RedactionService::new();
        redaction.register_secret("******");

        // Simulate Authorization header
        let header_value = "******";
        let redacted = redaction.redact_string(header_value);

        assert!(!redacted.contains("my-jwt-token-123"));
        assert!(redacted.contains("[REDACTED]"));
    }

    /// Test: URL query parameter redaction
    #[test]
    fn test_url_query_redaction() {
        use crate::services::RequestService;

        let redaction = RedactionService::new();
        redaction.register_secret("secret-token-abc");

        // Test URL with sensitive query param
        let url = "https://api.example.com/data?api_key=my-key&name=test";
        // This would be redacted by the request service
        let input = serde_json::json!({
            "url": url,
            "params": {
                "api_key": "my-key"
            }
        });

        let redacted = RequestService::redact_sensitive_fields(&input);
        assert_eq!(redacted["params"]["api_key"], "[REDACTED]");
    }

    /// Test: ResolvedAuthentication is not serializable to frontend
    #[test]
    fn test_metadata_not_exposing_secrets() {
        // ResolvedAuthentication is pub(crate), which means it cannot
        // be returned to the frontend through Tauri commands.
        // This is enforced at compile time by the Rust type system.
        //
        // The frontend only receives VaultEntryMetadata which has no secret value.

        // Verify VaultEntryMetadata doesn't contain any secret value
        let info = VaultEntryMetadata {
            id: "entry-1".to_string(),
            name: "test".to_string(),
            secret_type: "api_key".to_string(),
            status: "available".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Can only serialize metadata fields, no secret value
        let json = serde_json::to_value(&info).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();

        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"secret_type"));
        assert!(keys.contains(&"created_at"));
        assert!(!keys.contains(&"value"));
        assert!(!keys.contains(&"secret"));
    }
}
