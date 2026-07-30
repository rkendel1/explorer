//! Vault service for secure credential management.
//!
//! This service handles:
//! - Vault state management (lock/unlock)
//! - Secret storage and retrieval
//! - Secret resolution for request execution
//! - Automatic secret redaction
//!
//! Security guarantees:
//! - Secrets are never returned to the frontend
//! - Secrets are redacted from all events, logs, and history
//! - Vault state does not restore as unlocked after restart

use std::path::Path;
use std::sync::Arc;

use api_vault::{RedactionService, SecretType, VaultState, VaultStore};
use serde::{Deserialize, Serialize};

use crate::VaultEntryMetadata;
use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

/// Vault service configuration
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Auto-lock timeout in seconds (default: 15 minutes)
    pub auto_lock_seconds: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            auto_lock_seconds: 900,
        }
    }
}

/// Vault state response (safe for frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStateInfo {
    pub state: VaultState,
    pub entry_count: usize,
    pub auto_lock_seconds: Option<u64>,
}

/// Result of importing auth secrets from a dotenv file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvImportReport {
    pub file_path: String,
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

/// Preview of dotenv auth values that would be imported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvPreviewReport {
    pub file_path: String,
    pub will_import: Vec<EnvPreviewEntry>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvPreviewEntry {
    pub env_key: String,
    pub vault_entry_name: String,
    pub secret_type: String,
}

/// Authentication configuration (references vault, no secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationConfig {
    pub auth_type: AuthenticationType,
    pub vault_entry_name: String,
    /// For API key: header or query
    pub location: Option<String>,
    /// For API key: custom header name (default: X-API-Key)
    pub header_name: Option<String>,
    /// For Bearer: custom prefix (default: Bearer)
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationType {
    ApiKey,
    BearerToken,
}

/// Resolved authentication for request execution (internal only)
/// This struct contains actual secret values and must NEVER be serialized to frontend
#[allow(dead_code)]
pub(crate) struct ResolvedAuthentication {
    pub auth_type: AuthenticationType,
    pub header_name: String,
    pub header_value: String,
}

/// Vault service implementation
pub struct VaultService {
    redaction: RedactionService,
}

impl VaultService {
    pub fn new() -> Self {
        Self {
            redaction: RedactionService::new(),
        }
    }

    /// Get the redaction service for external use
    pub fn redaction_service(&self) -> &RedactionService {
        &self.redaction
    }

    /// Get vault state information
    pub async fn get_state(
        &self,
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<VaultStateInfo> {
        let vault_state = *state.vault_state.read().await;
        let root = state.active_root.read().await;

        let entry_count = if let Some(root) = root.as_ref()
            && vault_state == VaultState::Unlocked
        {
            self.get_entry_count(root)?
        } else {
            0
        };

        Ok(VaultStateInfo {
            state: vault_state,
            entry_count,
            auto_lock_seconds: if vault_state == VaultState::Unlocked {
                Some(VaultConfig::default().auto_lock_seconds)
            } else {
                None
            },
        })
    }

    /// Unlock the vault
    pub async fn unlock(
        &self,
        state: &Arc<DesktopStateManager>,
        _passphrase: Option<&str>,
    ) -> ServiceResult<VaultStateInfo> {
        let root = state.active_root.read().await;

        if root.is_none() {
            return Err(ServiceError::no_project());
        }

        // Set unlocking state
        *state.vault_state.write().await = VaultState::Unlocking;

        // In production, this would:
        // 1. Try OS keychain first
        // 2. Fall back to Argon2id key derivation from passphrase
        // For now, we simply mark as unlocked
        *state.vault_state.write().await = VaultState::Unlocked;

        self.get_state(state).await
    }

    /// Lock the vault
    pub async fn lock(&self, state: &Arc<DesktopStateManager>) -> ServiceResult<VaultStateInfo> {
        // Clear any registered secrets from redaction service
        self.redaction.clear_secrets();

        *state.vault_state.write().await = VaultState::Locked;

        self.get_state(state).await
    }

    /// List vault entries (metadata only, no secrets)
    pub async fn list_entries(
        &self,
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<VaultEntryMetadata>> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let vault_state = *state.vault_state.read().await;
        if vault_state != VaultState::Unlocked {
            return Err(ServiceError::vault_locked());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;

        let entries: Vec<VaultEntryMetadata> = store
            .list_entries()
            .map_err(|e| ServiceError::internal(&e.to_string()))?
            .into_iter()
            .map(|e| VaultEntryMetadata {
                id: e.id,
                name: e.name,
                secret_type: format!("{:?}", e.secret_type).to_lowercase(),
                status: "available".to_string(),
                created_at: e.created_at,
                updated_at: e.updated_at,
            })
            .collect();

        Ok(entries)
    }

    /// Create a new vault entry
    pub async fn create_entry(
        &self,
        state: &Arc<DesktopStateManager>,
        name: &str,
        secret_type: &str,
        secret_value: &str,
    ) -> ServiceResult<VaultEntryMetadata> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let vault_state = *state.vault_state.read().await;
        if vault_state != VaultState::Unlocked {
            return Err(ServiceError::vault_locked());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;

        let secret_type_enum = match secret_type {
            "api_key" => SecretType::ApiKey,
            "bearer_token" => SecretType::BearerToken,
            "basic_auth" => SecretType::BasicAuth,
            "oauth_token" => SecretType::OAuthToken,
            _ => SecretType::Custom,
        };

        let entry = store
            .upsert_secret(name, secret_type_enum, secret_value)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        // Register secret for redaction
        self.redaction.register_secret(secret_value);

        Ok(VaultEntryMetadata {
            id: entry.id,
            name: entry.name,
            secret_type: secret_type.to_string(),
            status: "available".to_string(),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        })
    }

    /// Import authentication-related entries from a dotenv file into the vault.
    ///
    /// By default this loads `<project-root>/.env`. A custom path may be
    /// absolute or relative to the project root.
    pub async fn import_env_auth_entries(
        &self,
        state: &Arc<DesktopStateManager>,
        env_path: Option<&str>,
        include_all: bool,
    ) -> ServiceResult<EnvImportReport> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let vault_state = *state.vault_state.read().await;
        if vault_state != VaultState::Unlocked {
            return Err(ServiceError::vault_locked());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let path = if let Some(custom) = env_path {
            let candidate = std::path::PathBuf::from(custom);
            if candidate.is_absolute() {
                candidate
            } else {
                root.join(candidate)
            }
        } else {
            root.join(".env")
        };

        let content = Self::read_dotenv_content(&path)?;

        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;

        let mut imported = Vec::new();
        let mut skipped = Vec::new();

        for raw_line in content.lines() {
            let Some((key, value)) = Self::parse_env_line(raw_line) else {
                continue;
            };

            let secret_type = if include_all {
                Self::classify_any_secret_type(&key, &value)
            } else {
                let Some(secret_type) = Self::classify_auth_secret_type(&key, &value) else {
                    skipped.push(key);
                    continue;
                };
                secret_type
            };

            let entry_name = Self::to_vault_entry_name(&key);
            let entry = store
                .upsert_secret(&entry_name, secret_type, &value)
                .map_err(|e| ServiceError::internal(&e.to_string()))?;

            self.redaction.register_secret(&value);
            imported.push(entry.name);
        }

        Ok(EnvImportReport {
            file_path: path.display().to_string(),
            imported,
            skipped,
        })
    }

    /// Preview authentication-related dotenv values that would be imported.
    pub async fn preview_env_auth_entries(
        &self,
        state: &Arc<DesktopStateManager>,
        env_path: Option<&str>,
        include_all: bool,
    ) -> ServiceResult<EnvPreviewReport> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;
        let path = Self::resolve_env_path(root, env_path);
        let content = Self::read_dotenv_content(&path)?;

        let mut will_import = Vec::new();
        let mut skipped = Vec::new();

        for raw_line in content.lines() {
            let Some((key, value)) = Self::parse_env_line(raw_line) else {
                continue;
            };

            let secret_type = if include_all {
                Self::classify_any_secret_type(&key, &value)
            } else {
                let Some(secret_type) = Self::classify_auth_secret_type(&key, &value) else {
                    skipped.push(key);
                    continue;
                };
                secret_type
            };

            will_import.push(EnvPreviewEntry {
                env_key: key.clone(),
                vault_entry_name: Self::to_vault_entry_name(&key),
                secret_type: Self::secret_type_label(secret_type).to_string(),
            });
        }

        Ok(EnvPreviewReport {
            file_path: path.display().to_string(),
            will_import,
            skipped,
        })
    }

    /// Delete a vault entry
    pub async fn delete_entry(
        &self,
        state: &Arc<DesktopStateManager>,
        name: &str,
    ) -> ServiceResult<bool> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let vault_state = *state.vault_state.read().await;
        if vault_state != VaultState::Unlocked {
            return Err(ServiceError::vault_locked());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;

        let deleted = store
            .delete_secret(name)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(deleted)
    }

    /// Resolve authentication for request execution (internal only)
    /// Returns the resolved authentication header, which must NEVER be sent to frontend
    pub(crate) async fn resolve_authentication(
        &self,
        state: &Arc<DesktopStateManager>,
        config: &AuthenticationConfig,
    ) -> ServiceResult<ResolvedAuthentication> {
        let vault_state = *state.vault_state.read().await;
        if vault_state != VaultState::Unlocked {
            return Err(ServiceError::vault_locked());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;

        let secret_value = store
            .resolve_secret(&config.vault_entry_name)
            .map_err(|_| ServiceError::vault_entry_missing(&config.vault_entry_name))?;

        // Register for redaction
        self.redaction.register_secret(&secret_value);

        let (header_name, header_value) = match config.auth_type {
            AuthenticationType::ApiKey => {
                let name = config
                    .header_name
                    .clone()
                    .unwrap_or_else(|| "X-API-Key".to_string());
                (name, secret_value)
            }
            AuthenticationType::BearerToken => {
                let prefix = config
                    .prefix
                    .clone()
                    .unwrap_or_else(|| "Bearer".to_string());
                (
                    "Authorization".to_string(),
                    format!("{} {}", prefix, secret_value),
                )
            }
        };

        Ok(ResolvedAuthentication {
            auth_type: config.auth_type.clone(),
            header_name,
            header_value,
        })
    }

    /// Check if a vault entry exists
    pub async fn entry_exists(
        &self,
        state: &Arc<DesktopStateManager>,
        name: &str,
    ) -> ServiceResult<bool> {
        let entries = self.list_entries(state).await?;
        Ok(entries.iter().any(|e| e.name == name))
    }

    // Private helpers

    fn get_entry_count(&self, root: &Path) -> ServiceResult<usize> {
        let store = VaultStore::open(root).map_err(|e| ServiceError::internal(&e.to_string()))?;
        let entries = store
            .list_entries()
            .map_err(|e| ServiceError::internal(&e.to_string()))?;
        Ok(entries.len())
    }

    fn resolve_env_path(root: &Path, env_path: Option<&str>) -> std::path::PathBuf {
        if let Some(custom) = env_path {
            let candidate = std::path::PathBuf::from(custom);
            if candidate.is_absolute() {
                candidate
            } else {
                root.join(candidate)
            }
        } else {
            root.join(".env")
        }
    }

    fn read_dotenv_content(path: &Path) -> ServiceResult<String> {
        std::fs::read_to_string(path).map_err(|e| {
            ServiceError::internal(&format!(
                "Unable to read dotenv file '{}': {e}",
                path.display()
            ))
        })
    }

    fn parse_env_line(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        let stripped = trimmed
            .strip_prefix("export ")
            .unwrap_or(trimmed)
            .trim();

        let (raw_key, raw_value) = stripped.split_once('=')?;
        let key = raw_key.trim();
        if key.is_empty() {
            return None;
        }

        let value_with_comment = raw_value.trim();
        let mut value = value_with_comment.to_string();

        if ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
            && value.len() >= 2
        {
            value = value[1..value.len() - 1].to_string();
        } else if let Some((before_hash, _)) = value.split_once(" #") {
            value = before_hash.trim().to_string();
        }

        Some((key.to_string(), value))
    }

    fn classify_auth_secret_type(key: &str, value: &str) -> Option<SecretType> {
        let upper = key.to_uppercase();
        if upper.contains("OAUTH") {
            return Some(SecretType::OAuthToken);
        }
        if upper.contains("BASIC") && upper.contains("AUTH") {
            return Some(SecretType::BasicAuth);
        }
        if upper.contains("API_KEY") || upper.contains("APIKEY") {
            return Some(SecretType::ApiKey);
        }
        if upper.contains("TOKEN")
            || upper == "AUTHORIZATION"
            || value.starts_with("Bearer ")
            || value.starts_with("bearer ")
        {
            return Some(SecretType::BearerToken);
        }
        None
    }

    fn classify_any_secret_type(key: &str, value: &str) -> SecretType {
        if let Some(auth_type) = Self::classify_auth_secret_type(key, value) {
            return auth_type;
        }

        let upper = key.to_uppercase();
        if upper.contains("PASSWORD") || upper.contains("POSTGRES") || upper.contains("DATABASE") {
            return SecretType::DatabaseCredential;
        }
        if upper.contains("CERT") || upper.contains("PEM") || upper.contains("TLS") {
            return SecretType::Certificate;
        }

        SecretType::Custom
    }

    fn secret_type_label(secret_type: SecretType) -> &'static str {
        match secret_type {
            SecretType::ApiKey => "api_key",
            SecretType::BearerToken => "bearer_token",
            SecretType::BasicAuth => "basic_auth",
            SecretType::OAuthToken => "oauth_token",
            SecretType::DatabaseCredential => "database_credential",
            SecretType::Certificate => "certificate",
            SecretType::Custom => "custom",
        }
    }

    fn to_vault_entry_name(key: &str) -> String {
        key.to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }
}

impl Default for VaultService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::test_helpers::create_test_project;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_vault_lock_unlock() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();

        // Initially locked
        let info = service.get_state(&state).await.unwrap();
        assert_eq!(info.state, VaultState::Locked);

        // Unlock
        let info = service.unlock(&state, None).await.unwrap();
        assert_eq!(info.state, VaultState::Unlocked);

        // Lock
        let info = service.lock(&state).await.unwrap();
        assert_eq!(info.state, VaultState::Locked);
    }

    #[tokio::test]
    async fn test_vault_requires_unlock() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();

        // List entries should fail when locked
        let result = service.list_entries(&state).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, super::super::ServiceErrorCode::VaultLocked);
    }

    #[tokio::test]
    async fn test_create_and_list_entries() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();
        service.unlock(&state, None).await.unwrap();

        // Create entry
        let entry = service
            .create_entry(&state, "staging-token", "bearer_token", "secret-value-123")
            .await
            .unwrap();

        assert_eq!(entry.name, "staging-token");
        assert_eq!(entry.secret_type, "bearer_token");

        // List entries (no secret values)
        let entries = service.list_entries(&state).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "staging-token");
    }

    #[test]
    fn test_redaction_service() {
        let service = VaultService::new();

        // Register a secret
        service.redaction.register_secret("super-secret-123");

        // Redact string
        let input = "Auth: super-secret-123";
        let output = service.redaction.redact_string(input);
        assert!(!output.contains("super-secret-123"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn test_vault_state_never_restored_unlocked() {
        // This test verifies that vault state defaults to locked
        // on new state manager creation (simulating restart)
        let app_dir = tempdir().unwrap();
        let state = DesktopStateManager::new(app_dir.path().to_path_buf());

        // Use blocking API for sync test
        let vault_state = state.vault_state.blocking_read();
        assert_eq!(*vault_state, VaultState::Locked);
    }

    #[tokio::test]
    async fn test_import_env_auth_entries() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        std::fs::write(
            project_dir.path().join(".env"),
            "API_KEY=abc123\nINTERNAL_FLAG=true\nAUTH_TOKEN=token-xyz\n",
        )
        .unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();
        service.unlock(&state, None).await.unwrap();

        let report = service
            .import_env_auth_entries(&state, None, false)
            .await
            .unwrap();

        assert!(report.imported.iter().any(|n| n == "api-key"));
        assert!(report.imported.iter().any(|n| n == "auth-token"));
        assert!(report.skipped.iter().any(|n| n == "INTERNAL_FLAG"));
    }

    #[tokio::test]
    async fn test_preview_env_auth_entries() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        std::fs::write(
            project_dir.path().join(".env"),
            "API_KEY=abc123\nFEATURE=true\nBEARER_TOKEN=my-token\n",
        )
        .unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();
        let preview = service
            .preview_env_auth_entries(&state, None, false)
            .await
            .unwrap();

        assert!(preview.will_import.iter().any(|e| e.vault_entry_name == "api-key"));
        assert!(preview.skipped.iter().any(|n| n == "FEATURE"));
    }

    #[tokio::test]
    async fn test_import_env_include_all_variables() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        std::fs::write(
            project_dir.path().join(".env"),
            "NODE_ENV=development\nAPP_PORT=8000\nAUTH_TOKEN=token-xyz\n",
        )
        .unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(create_test_project(project_dir.path()));

        let service = VaultService::new();
        service.unlock(&state, None).await.unwrap();

        let report = service
            .import_env_auth_entries(&state, None, true)
            .await
            .unwrap();

        assert!(report.imported.iter().any(|n| n == "node-env"));
        assert!(report.imported.iter().any(|n| n == "app-port"));
        assert!(report.imported.iter().any(|n| n == "auth-token"));
        assert!(report.skipped.is_empty());
    }
}
