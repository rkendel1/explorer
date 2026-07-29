//! Vault commands

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use api_vault::VaultState;

use crate::VaultEntryMetadata;

use super::{AppState, CommandResult};

/// Create vault entry request
#[derive(Debug, Deserialize)]
pub struct CreateVaultEntryRequest {
    pub name: String,
    pub secret_type: String, // "api_key" | "bearer_token"
    pub value: String,       // This is the only place where secret values are accepted
}

/// Update vault entry request
#[derive(Debug, Deserialize)]
pub struct UpdateVaultEntryRequest {
    pub id: String,
    pub name: Option<String>,
    pub value: Option<String>,
}

/// Delete vault entry request
#[derive(Debug, Deserialize)]
pub struct DeleteVaultEntryRequest {
    pub id: String,
}

/// Unlock vault request
#[derive(Debug, Deserialize)]
pub struct UnlockVaultRequest {
    pub passphrase: Option<String>,
}

/// Vault state response
#[derive(Debug, Serialize)]
pub struct VaultStateResponse {
    pub state: String,
    pub auto_lock_seconds: Option<u64>,
}

/// Reveal secret request (requires explicit user action)
#[derive(Debug, Deserialize)]
pub struct RevealSecretRequest {
    pub id: String,
}

/// Reveal secret response (value is auto-cleared after 30 seconds)
#[derive(Debug, Serialize)]
pub struct RevealSecretResponse {
    pub id: String,
    pub value: String,
    pub expires_in_seconds: u64,
}

/// List vault entries
pub async fn vault_list(state: AppState<'_>) -> CommandResult<Vec<VaultEntryMetadata>> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    CommandResult::ok(Vec::new())
}

/// Create a vault entry
pub async fn vault_create(
    state: AppState<'_>,
    request: CreateVaultEntryRequest,
) -> CommandResult<VaultEntryMetadata> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    if state.get_vault_state().await != VaultState::Unlocked {
        return CommandResult::unauthorized("Vault is locked");
    }

    let now = Utc::now();
    let metadata = VaultEntryMetadata {
        id: Uuid::new_v4().to_string(),
        name: request.name,
        secret_type: request.secret_type,
        status: "available".to_string(),
        created_at: now,
        updated_at: now,
    };

    CommandResult::ok(metadata)
}

/// Update a vault entry
pub async fn vault_update(
    state: AppState<'_>,
    request: UpdateVaultEntryRequest,
) -> CommandResult<VaultEntryMetadata> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    if state.get_vault_state().await != VaultState::Unlocked {
        return CommandResult::unauthorized("Vault is locked");
    }

    let now = Utc::now();
    let metadata = VaultEntryMetadata {
        id: request.id,
        name: request.name.unwrap_or_default(),
        secret_type: "bearer_token".to_string(),
        status: "available".to_string(),
        created_at: now,
        updated_at: now,
    };

    CommandResult::ok(metadata)
}

/// Delete a vault entry
pub async fn vault_delete(
    state: AppState<'_>,
    _request: DeleteVaultEntryRequest,
) -> CommandResult<()> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    if state.get_vault_state().await != VaultState::Unlocked {
        return CommandResult::unauthorized("Vault is locked");
    }

    CommandResult::ok(())
}

/// Unlock the vault
pub async fn vault_unlock(
    state: AppState<'_>,
    _request: UnlockVaultRequest,
) -> CommandResult<VaultStateResponse> {
    // In production, this would derive key from keychain or passphrase
    state.set_vault_state(VaultState::Unlocked).await;

    CommandResult::ok(VaultStateResponse {
        state: "unlocked".to_string(),
        auto_lock_seconds: Some(900), // 15 minutes
    })
}

/// Lock the vault
pub async fn vault_lock(state: AppState<'_>) -> CommandResult<VaultStateResponse> {
    state.set_vault_state(VaultState::Locked).await;

    CommandResult::ok(VaultStateResponse {
        state: "locked".to_string(),
        auto_lock_seconds: None,
    })
}

/// Get vault state
pub async fn vault_state(state: AppState<'_>) -> CommandResult<VaultStateResponse> {
    let vault_state = state.get_vault_state().await;

    CommandResult::ok(VaultStateResponse {
        state: match vault_state {
            VaultState::Locked => "locked".to_string(),
            VaultState::Unlocking => "unlocking".to_string(),
            VaultState::Unlocked => "unlocked".to_string(),
            VaultState::Error => "error".to_string(),
        },
        auto_lock_seconds: if vault_state == VaultState::Unlocked {
            Some(900)
        } else {
            None
        },
    })
}

/// Reveal a secret value (requires unlocked vault)
pub async fn vault_reveal(
    state: AppState<'_>,
    request: RevealSecretRequest,
) -> CommandResult<RevealSecretResponse> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    if state.get_vault_state().await != VaultState::Unlocked {
        return CommandResult::unauthorized("Vault is locked");
    }

    CommandResult::ok(RevealSecretResponse {
        id: request.id,
        value: "[REVEALED_SECRET]".to_string(),
        expires_in_seconds: 30,
    })
}
