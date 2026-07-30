//! Vault commands

use serde::{Deserialize, Serialize};

use crate::VaultEntryMetadata;
use crate::services::VaultService;
use crate::services::vault_service::{EnvImportReport, EnvPreviewReport};

use super::{AppState, CommandResult, from_service, state_handle};

/// Create vault entry request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultEntryRequest {
    pub name: String,
    pub secret_type: String, // "api_key" | "bearer_token" | "basic_auth" | "oauth_token"
    pub value: String,       // This is the only place where secret values are accepted
}

/// Delete vault entry request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVaultEntryRequest {
    pub name: String,
}

/// Unlock vault request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockVaultRequest {
    pub passphrase: Option<String>,
}

/// Import dotenv secrets into vault request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportVaultEnvRequest {
    pub path: Option<String>,
    pub include_all: Option<bool>,
}

/// Vault state response
#[derive(Debug, Serialize)]
pub struct VaultStateResponse {
    pub state: String,
    pub entry_count: usize,
    pub auto_lock_seconds: Option<u64>,
}

impl From<crate::services::vault_service::VaultStateInfo> for VaultStateResponse {
    fn from(info: crate::services::vault_service::VaultStateInfo) -> Self {
        Self {
            state: match info.state {
                api_vault::VaultState::Locked => "locked".to_string(),
                api_vault::VaultState::Unlocking => "unlocking".to_string(),
                api_vault::VaultState::Unlocked => "unlocked".to_string(),
                api_vault::VaultState::Error => "error".to_string(),
            },
            entry_count: info.entry_count,
            auto_lock_seconds: info.auto_lock_seconds,
        }
    }
}

fn vault_service() -> VaultService {
    VaultService::new()
}

/// List vault entries
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_list(state: AppState<'_>) -> CommandResult<Vec<VaultEntryMetadata>> {
    let state = state_handle(&state);
    from_service(vault_service().list_entries(&state).await)
}

/// Create a vault entry
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_create(
    state: AppState<'_>,
    request: CreateVaultEntryRequest,
) -> CommandResult<VaultEntryMetadata> {
    let state = state_handle(&state);
    from_service(
        vault_service()
            .create_entry(&state, &request.name, &request.secret_type, &request.value)
            .await,
    )
}

/// Delete a vault entry
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_delete(
    state: AppState<'_>,
    request: DeleteVaultEntryRequest,
) -> CommandResult<bool> {
    let state = state_handle(&state);
    from_service(vault_service().delete_entry(&state, &request.name).await)
}

/// Unlock the vault
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_unlock(
    state: AppState<'_>,
    request: UnlockVaultRequest,
) -> CommandResult<VaultStateResponse> {
    let state = state_handle(&state);
    from_service(
        vault_service()
            .unlock(&state, request.passphrase.as_deref())
            .await,
    )
    .map(Into::into)
}

/// Lock the vault
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_lock(state: AppState<'_>) -> CommandResult<VaultStateResponse> {
    let state = state_handle(&state);
    from_service(vault_service().lock(&state).await).map(Into::into)
}

/// Get vault state
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_state(state: AppState<'_>) -> CommandResult<VaultStateResponse> {
    let state = state_handle(&state);
    from_service(vault_service().get_state(&state).await).map(Into::into)
}

/// Import auth-focused dotenv values into vault
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_import_env(
    state: AppState<'_>,
    request: ImportVaultEnvRequest,
) -> CommandResult<EnvImportReport> {
    let state = state_handle(&state);
    from_service(
        vault_service()
            .import_env_auth_entries(
                &state,
                request.path.as_deref(),
                request.include_all.unwrap_or(false),
            )
            .await,
    )
}

/// Preview auth-focused dotenv values that would be imported
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn vault_preview_env(
    state: AppState<'_>,
    request: ImportVaultEnvRequest,
) -> CommandResult<EnvPreviewReport> {
    let state = state_handle(&state);
    from_service(
        vault_service()
            .preview_env_auth_entries(
                &state,
                request.path.as_deref(),
                request.include_all.unwrap_or(false),
            )
            .await,
    )
}
