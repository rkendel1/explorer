//! Desktop application service layer.
//!
//! This module provides explicit service abstractions that separate
//! Tauri command handling from platform business logic.
//!
//! Services delegate to canonical platform crates:
//! - `api-core` for contract models
//! - `api-compiler` for contract compilation
//! - `api-storage` for persistence
//! - `api-client` for request execution
//! - `api-vault` for secret management
//! - `api-workflows` for guided workflows
//! - `api-runtime-events` for runtime observability
//! - `api-testing` for API testing

pub mod changes_service;
pub mod customer_journey_service;
pub mod environment_service;
pub mod explorer_service;
pub mod project_service;
pub mod request_service;
pub mod runtime_service;
#[cfg(test)]
mod security_tests;
pub mod testing_service;
pub mod vault_service;
pub mod workflow_service;

pub use changes_service::ChangesService;
pub use customer_journey_service::CustomerJourneyService;
pub use environment_service::EnvironmentService;
pub use explorer_service::ExplorerService;
pub use project_service::ProjectService;
pub use request_service::RequestService;
pub use runtime_service::RuntimeService;
pub use testing_service::TestingService;
pub use vault_service::VaultService;
pub use workflow_service::WorkflowService;

use serde::{Deserialize, Serialize};

/// Service error type for user-safe error reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceError {
    /// Error code for programmatic handling
    pub code: ServiceErrorCode,
    /// User-readable message (safe for display)
    pub message: String,
    /// Optional recovery action hint
    pub recovery_hint: Option<String>,
}

/// Error codes for service operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    /// No project is currently open
    NoProjectOpen,
    /// Project or resource not found
    NotFound,
    /// Vault is locked
    VaultLocked,
    /// Vault entry not found
    VaultEntryMissing,
    /// Vault unlock failed
    VaultUnlockFailed,
    /// Environment not found
    EnvironmentMissing,
    /// Request execution failed
    RequestFailed,
    /// Runtime operation failed
    RuntimeFailed,
    /// Test execution failed
    TestFailed,
    /// Contract change review failed
    ContractReviewFailed,
    /// Repository access error
    RepositoryError,
    /// Validation error
    ValidationError,
    /// Internal error
    InternalError,
}

impl ServiceError {
    pub fn no_project() -> Self {
        Self {
            code: ServiceErrorCode::NoProjectOpen,
            message: "No project is currently open".to_string(),
            recovery_hint: Some("Open a project to continue".to_string()),
        }
    }

    pub fn not_found(resource: &str) -> Self {
        Self {
            code: ServiceErrorCode::NotFound,
            message: format!("{} not found", resource),
            recovery_hint: None,
        }
    }

    pub fn vault_locked() -> Self {
        Self {
            code: ServiceErrorCode::VaultLocked,
            message: "Vault is locked".to_string(),
            recovery_hint: Some("Unlock the vault to access credentials".to_string()),
        }
    }

    pub fn vault_entry_missing(name: &str) -> Self {
        Self {
            code: ServiceErrorCode::VaultEntryMissing,
            message: format!("Vault entry '{}' not found", name),
            recovery_hint: Some("Create the required credential in the vault".to_string()),
        }
    }

    pub fn environment_missing(name: &str) -> Self {
        Self {
            code: ServiceErrorCode::EnvironmentMissing,
            message: format!("Environment '{}' not found", name),
            recovery_hint: Some("Create the environment or select an existing one".to_string()),
        }
    }

    pub fn request_failed(reason: &str) -> Self {
        Self {
            code: ServiceErrorCode::RequestFailed,
            message: format!("Request failed: {}", reason),
            recovery_hint: Some("Check the request configuration and try again".to_string()),
        }
    }

    pub fn runtime_failed(reason: &str) -> Self {
        Self {
            code: ServiceErrorCode::RuntimeFailed,
            message: format!("Runtime error: {}", reason),
            recovery_hint: Some("Check runtime configuration and port availability".to_string()),
        }
    }

    pub fn internal(reason: &str) -> Self {
        Self {
            code: ServiceErrorCode::InternalError,
            message: "An internal error occurred".to_string(),
            recovery_hint: Some(format!("Technical details: {}", reason)),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            code: ServiceErrorCode::ValidationError,
            message: message.into(),
            recovery_hint: None,
        }
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ServiceError {}

impl From<anyhow::Error> for ServiceError {
    fn from(err: anyhow::Error) -> Self {
        ServiceError::internal(&err.to_string())
    }
}

/// Service result type
pub type ServiceResult<T> = Result<T, ServiceError>;

#[cfg(test)]
pub(crate) mod test_helpers {
    use api_projects::{
        ApiProject, ContractReference, EnvironmentReference, EnvironmentSafety,
        RepositoryReference, RuntimeProfile, RuntimeTarget, VaultReference,
    };
    use chrono::Utc;
    use std::path::Path;

    /// Create a minimal test project
    pub fn create_test_project(root: &Path) -> ApiProject {
        ApiProject {
            id: "test-project-id".to_string(),
            name: "test".to_string(),
            repository: RepositoryReference {
                root: root.display().to_string(),
            },
            contract: ContractReference {
                path: ".repo-api/contract/effective.json".to_string(),
            },
            environments: vec![EnvironmentReference {
                name: "mock".to_string(),
            }],
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
}
