//! Environment service for variable resolution.
//!
//! This service handles:
//! - Environment CRUD operations
//! - Variable resolution (literal, generated, vault)
//! - Environment-to-vault linking
//! - Authentication configuration per environment

use std::collections::BTreeMap;
use std::sync::Arc;

use api_projects::EnvironmentReference;
use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

/// Environment variable definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvironmentVariable {
    /// Literal string value
    Literal { value: String },
    /// Generated value (uuid, timestamp, etc.)
    Generated { generator: VariableGenerator },
    /// Reference to a vault entry
    Vault { entry_name: String },
}

/// Variable generators
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableGenerator {
    Uuid,
    Timestamp,
    RandomString { length: usize },
    Counter { prefix: String },
}

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub id: String,
    pub name: String,
    pub variables: BTreeMap<String, EnvironmentVariable>,
    pub authentication: Option<EnvironmentAuthentication>,
    pub is_active: bool,
}

/// Environment authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentAuthentication {
    pub auth_type: String,
    pub vault_entry_name: String,
    pub header_name: Option<String>,
    pub prefix: Option<String>,
}

/// Resolved variables (without vault secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedVariables {
    /// Resolved literal and generated values
    pub resolved: BTreeMap<String, String>,
    /// Vault references that need runtime resolution
    pub vault_refs: Vec<String>,
}

/// Environment service implementation
pub struct EnvironmentService;

impl EnvironmentService {
    /// List all environments
    pub async fn list(state: &Arc<DesktopStateManager>) -> ServiceResult<Vec<EnvironmentConfig>> {
        let project = state.project.read().await;
        let active_env = state.active_environment.read().await;

        if let Some(project) = project.as_ref() {
            let environments: Vec<EnvironmentConfig> = project
                .environments
                .iter()
                .map(|env| EnvironmentConfig {
                    id: env.name.clone(),
                    name: env.name.clone(),
                    variables: BTreeMap::new(),
                    authentication: None,
                    is_active: active_env.as_ref() == Some(&env.name),
                })
                .collect();

            Ok(environments)
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Get environment by ID
    pub async fn get(
        state: &Arc<DesktopStateManager>,
        id: &str,
    ) -> ServiceResult<EnvironmentConfig> {
        let project = state.project.read().await;
        let active_env = state.active_environment.read().await;

        if let Some(project) = project.as_ref() {
            project
                .environments
                .iter()
                .find(|e| e.name == id)
                .map(|env| EnvironmentConfig {
                    id: env.name.clone(),
                    name: env.name.clone(),
                    variables: BTreeMap::new(),
                    authentication: None,
                    is_active: active_env.as_ref() == Some(&env.name),
                })
                .ok_or_else(|| ServiceError::environment_missing(id))
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Select an environment as active
    pub async fn select(
        state: &Arc<DesktopStateManager>,
        id: &str,
    ) -> ServiceResult<EnvironmentConfig> {
        let project = state.project.read().await;

        if let Some(project) = project.as_ref() {
            if project.environments.iter().any(|e| e.name == id) {
                *state.active_environment.write().await = Some(id.to_string());

                Ok(EnvironmentConfig {
                    id: id.to_string(),
                    name: id.to_string(),
                    variables: BTreeMap::new(),
                    authentication: None,
                    is_active: true,
                })
            } else {
                Err(ServiceError::environment_missing(id))
            }
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Create a new environment
    pub async fn create(
        state: &Arc<DesktopStateManager>,
        name: &str,
    ) -> ServiceResult<EnvironmentConfig> {
        let mut project = state.project.write().await;

        if let Some(project) = project.as_mut() {
            if project.environments.iter().any(|e| e.name == name) {
                return Err(ServiceError::validation(
                    "Environment with this name already exists",
                ));
            }

            let env = EnvironmentReference {
                name: name.to_string(),
            };
            project.environments.push(env);

            Ok(EnvironmentConfig {
                id: name.to_string(),
                name: name.to_string(),
                variables: BTreeMap::new(),
                authentication: None,
                is_active: false,
            })
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Delete an environment
    pub async fn delete(state: &Arc<DesktopStateManager>, id: &str) -> ServiceResult<bool> {
        let mut project = state.project.write().await;

        if let Some(project) = project.as_mut() {
            let initial_len = project.environments.len();
            project.environments.retain(|e| e.name != id);

            if project.environments.len() < initial_len {
                // Clear active if deleted
                let mut active_env = state.active_environment.write().await;
                if active_env.as_ref() == Some(&id.to_string()) {
                    *active_env = None;
                }
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(ServiceError::no_project())
        }
    }

    /// Resolve variables for an environment (without vault secrets)
    pub async fn resolve_variables(
        state: &Arc<DesktopStateManager>,
        _environment_id: &str,
        variables: &BTreeMap<String, EnvironmentVariable>,
    ) -> ServiceResult<ResolvedVariables> {
        let _project = state.project.read().await;

        let mut resolved = BTreeMap::new();
        let mut vault_refs = Vec::new();

        for (key, var) in variables {
            match var {
                EnvironmentVariable::Literal { value } => {
                    resolved.insert(key.clone(), value.clone());
                }
                EnvironmentVariable::Generated { generator } => {
                    let value = Self::generate_value(generator);
                    resolved.insert(key.clone(), value);
                }
                EnvironmentVariable::Vault { entry_name } => {
                    // Don't resolve vault values here - they must be resolved
                    // at request execution time through VaultService
                    vault_refs.push(entry_name.clone());
                    resolved.insert(key.clone(), format!("{{{{vault:{}}}}}", entry_name));
                }
            }
        }

        Ok(ResolvedVariables {
            resolved,
            vault_refs,
        })
    }

    /// Link a vault credential to an environment
    pub async fn link_vault_credential(
        state: &Arc<DesktopStateManager>,
        environment_id: &str,
        auth_type: &str,
        vault_entry_name: &str,
        header_name: Option<&str>,
        prefix: Option<&str>,
    ) -> ServiceResult<EnvironmentAuthentication> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let project = project.as_ref().unwrap();
        if !project
            .environments
            .iter()
            .any(|e| e.name == environment_id)
        {
            return Err(ServiceError::environment_missing(environment_id));
        }

        // In production, this would persist the link
        Ok(EnvironmentAuthentication {
            auth_type: auth_type.to_string(),
            vault_entry_name: vault_entry_name.to_string(),
            header_name: header_name.map(String::from),
            prefix: prefix.map(String::from),
        })
    }

    /// Get the active environment
    pub async fn get_active(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Option<EnvironmentConfig>> {
        let active_env = state.active_environment.read().await;

        if let Some(env_name) = active_env.as_ref() {
            Self::get(state, env_name).await.map(Some)
        } else {
            Ok(None)
        }
    }

    // Private helpers

    fn generate_value(generator: &VariableGenerator) -> String {
        match generator {
            VariableGenerator::Uuid => uuid::Uuid::new_v4().to_string(),
            VariableGenerator::Timestamp => chrono::Utc::now().to_rfc3339(),
            VariableGenerator::RandomString { length } => {
                // Use a simple approach without external rand
                use std::time::{SystemTime, UNIX_EPOCH};
                const CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let mut result = String::with_capacity(*length);
                for i in 0..*length {
                    let idx = ((seed >> (i % 64)) as usize + i) % CHARSET.len();
                    result.push(CHARSET[idx] as char);
                }
                result
            }
            VariableGenerator::Counter { prefix } => {
                // In production, this would maintain a counter
                format!("{}-1", prefix)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_projects::{
        ApiProject, ContractReference, EnvironmentReference, EnvironmentSafety,
        RepositoryReference, RuntimeProfile, RuntimeTarget, VaultReference,
    };
    use chrono::Utc;
    use tempfile::tempdir;

    fn create_test_project_with_envs(envs: Vec<&str>) -> ApiProject {
        ApiProject {
            id: "test-project-id".to_string(),
            name: "test".to_string(),
            repository: RepositoryReference {
                root: "/tmp/test".to_string(),
            },
            contract: ContractReference {
                path: ".repo-api/contract/effective.json".to_string(),
            },
            environments: envs
                .iter()
                .map(|e| EnvironmentReference {
                    name: e.to_string(),
                })
                .collect(),
            vault: VaultReference {
                path: ".repo-api/vault/encrypted.db".to_string(),
            },
            workflows: vec![],
            runtime_profiles: vec![RuntimeProfile {
                id: "profile_mock".to_string(),
                name: "Mock".to_string(),
                environment: "mock".to_string(),
                target: RuntimeTarget::MockRuntime,
                safety: EnvironmentSafety::Safe,
            }],
            created_at: Utc::now(),
            active_environment: Some("mock".to_string()),
            active_runtime_profile: Some("profile_mock".to_string()),
        }
    }

    #[tokio::test]
    async fn test_environment_crud() {
        let app_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.project.write().await = Some(create_test_project_with_envs(vec!["staging"]));

        // List environments
        let envs = EnvironmentService::list(&state).await.unwrap();
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].name, "staging");

        // Create new environment
        let new_env = EnvironmentService::create(&state, "production")
            .await
            .unwrap();
        assert_eq!(new_env.name, "production");

        // List again
        let envs = EnvironmentService::list(&state).await.unwrap();
        assert_eq!(envs.len(), 2);

        // Select environment
        EnvironmentService::select(&state, "production")
            .await
            .unwrap();

        let active = EnvironmentService::get_active(&state).await.unwrap();
        assert!(active.is_some());
        assert_eq!(active.unwrap().name, "production");

        // Delete environment
        let deleted = EnvironmentService::delete(&state, "staging").await.unwrap();
        assert!(deleted);

        let envs = EnvironmentService::list(&state).await.unwrap();
        assert_eq!(envs.len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_variables() {
        let app_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.project.write().await = Some(create_test_project_with_envs(vec![]));

        let mut variables = BTreeMap::new();
        variables.insert(
            "baseUrl".to_string(),
            EnvironmentVariable::Literal {
                value: "https://api.example.com".to_string(),
            },
        );
        variables.insert(
            "requestId".to_string(),
            EnvironmentVariable::Generated {
                generator: VariableGenerator::Uuid,
            },
        );
        variables.insert(
            "token".to_string(),
            EnvironmentVariable::Vault {
                entry_name: "staging-token".to_string(),
            },
        );

        let resolved = EnvironmentService::resolve_variables(&state, "staging", &variables)
            .await
            .unwrap();

        assert_eq!(
            resolved.resolved.get("baseUrl").unwrap(),
            "https://api.example.com"
        );
        assert!(!resolved.resolved.get("requestId").unwrap().is_empty());
        // Vault refs should be tracked but not resolved
        assert_eq!(resolved.vault_refs, vec!["staging-token"]);
        assert!(resolved.resolved.get("token").unwrap().contains("vault:"));
    }

    #[test]
    fn test_variable_generators() {
        // UUID
        let uuid = EnvironmentService::generate_value(&VariableGenerator::Uuid);
        assert!(uuid.len() == 36); // UUID format

        // Timestamp
        let ts = EnvironmentService::generate_value(&VariableGenerator::Timestamp);
        assert!(ts.contains("T")); // ISO format

        // Random string
        let rand =
            EnvironmentService::generate_value(&VariableGenerator::RandomString { length: 16 });
        assert_eq!(rand.len(), 16);

        // Counter
        let counter = EnvironmentService::generate_value(&VariableGenerator::Counter {
            prefix: "req".to_string(),
        });
        assert!(counter.starts_with("req-"));
    }

    #[tokio::test]
    async fn test_duplicate_environment_name() {
        let app_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.project.write().await = Some(create_test_project_with_envs(vec!["staging"]));

        // Try to create duplicate
        let result = EnvironmentService::create(&state, "staging").await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code,
            super::super::ServiceErrorCode::ValidationError
        );
    }
}
