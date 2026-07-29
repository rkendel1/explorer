//! Vault state management for secure credential handling.
//!
//! This module provides:
//! - Vault lock/unlock state machine
//! - Auto-lock functionality
//! - Session timeout management
//! - Secure memory clearing on lock

use crate::redaction::RedactionService;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use zeroize::Zeroize;

/// Default auto-lock timeout in minutes
pub const DEFAULT_AUTO_LOCK_MINUTES: i64 = 15;

/// Vault operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultState {
    /// Vault is locked, no secrets accessible
    Locked,
    /// Vault is in the process of unlocking
    Unlocking,
    /// Vault is unlocked and secrets are accessible
    Unlocked,
    /// Vault encountered an error
    Error,
}

impl Default for VaultState {
    fn default() -> Self {
        Self::Locked
    }
}

/// Error state details for vault operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultErrorState {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// Vault session tracking
#[derive(Debug)]
struct VaultSession {
    unlocked_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    auto_lock_minutes: i64,
    /// Encryption key (cleared on lock)
    key: Option<SecureKey>,
}

/// Secure key wrapper that zeroes memory on drop
#[derive(Debug)]
pub struct SecureKey {
    inner: [u8; 32],
}

impl SecureKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { inner: key }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.inner
    }
}

impl Drop for SecureKey {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

impl Clone for SecureKey {
    fn clone(&self) -> Self {
        Self { inner: self.inner }
    }
}

/// Vault state manager with auto-lock support
pub struct VaultStateManager {
    state: Arc<RwLock<VaultState>>,
    session: Arc<RwLock<Option<VaultSession>>>,
    state_sender: watch::Sender<VaultState>,
    state_receiver: watch::Receiver<VaultState>,
    redaction: RedactionService,
    auto_lock_minutes: i64,
}

impl VaultStateManager {
    /// Create a new vault state manager
    pub fn new(auto_lock_minutes: Option<i64>) -> Self {
        let (sender, receiver) = watch::channel(VaultState::Locked);
        Self {
            state: Arc::new(RwLock::new(VaultState::Locked)),
            session: Arc::new(RwLock::new(None)),
            state_sender: sender,
            state_receiver: receiver,
            redaction: RedactionService::new(),
            auto_lock_minutes: auto_lock_minutes.unwrap_or(DEFAULT_AUTO_LOCK_MINUTES),
        }
    }

    /// Get current vault state
    pub async fn state(&self) -> VaultState {
        *self.state.read().await
    }

    /// Subscribe to state changes
    pub fn subscribe(&self) -> watch::Receiver<VaultState> {
        self.state_receiver.clone()
    }

    /// Get the redaction service
    pub fn redaction(&self) -> &RedactionService {
        &self.redaction
    }

    /// Attempt to unlock the vault with the provided key
    pub async fn unlock(&self, key: SecureKey) -> Result<(), VaultErrorState> {
        let mut state = self.state.write().await;
        *state = VaultState::Unlocking;
        let _ = self.state_sender.send(VaultState::Unlocking);

        // Create session
        let now = Utc::now();
        let session = VaultSession {
            unlocked_at: now,
            last_activity: now,
            auto_lock_minutes: self.auto_lock_minutes,
            key: Some(key),
        };

        *self.session.write().await = Some(session);
        *state = VaultState::Unlocked;
        let _ = self.state_sender.send(VaultState::Unlocked);

        Ok(())
    }

    /// Lock the vault, clearing all secret material from memory
    pub async fn lock(&self) {
        let mut state = self.state.write().await;
        
        // Clear session and key
        if let Some(mut session) = self.session.write().await.take() {
            if let Some(key) = session.key.take() {
                // Key is dropped here, zeroizing memory
                drop(key);
            }
        }

        // Clear registered secrets from redaction service
        self.redaction.clear_secrets();

        *state = VaultState::Locked;
        let _ = self.state_sender.send(VaultState::Locked);
    }

    /// Record activity to reset auto-lock timer
    pub async fn record_activity(&self) {
        if let Some(ref mut session) = *self.session.write().await {
            session.last_activity = Utc::now();
        }
    }

    /// Check if auto-lock timeout has expired
    pub async fn should_auto_lock(&self) -> bool {
        if let Some(ref session) = *self.session.read().await {
            let timeout_secs = session.auto_lock_minutes * 60;
            let elapsed = Utc::now().signed_duration_since(session.last_activity);
            elapsed.num_seconds() > timeout_secs
        } else {
            false
        }
    }

    /// Get the encryption key if vault is unlocked
    pub async fn get_key(&self) -> Option<SecureKey> {
        let state = *self.state.read().await;
        if state != VaultState::Unlocked {
            return None;
        }
        
        // Record activity
        self.record_activity().await;
        
        self.session.read().await.as_ref().and_then(|s| s.key.clone())
    }

    /// Set auto-lock timeout in minutes
    pub async fn set_auto_lock_timeout(&self, minutes: i64) {
        if let Some(ref mut session) = *self.session.write().await {
            session.auto_lock_minutes = minutes;
        }
    }

    /// Get time until auto-lock in seconds
    pub async fn time_until_auto_lock(&self) -> Option<i64> {
        if let Some(ref session) = *self.session.read().await {
            let timeout_secs = session.auto_lock_minutes * 60;
            let elapsed = Utc::now().signed_duration_since(session.last_activity);
            let remaining = timeout_secs - elapsed.num_seconds();
            Some(remaining.max(0))
        } else {
            None
        }
    }

    /// Set error state
    pub async fn set_error(&self, _error: VaultErrorState) {
        let mut state = self.state.write().await;
        *state = VaultState::Error;
        let _ = self.state_sender.send(VaultState::Error);
    }

    /// Check if vault is unlocked
    pub async fn is_unlocked(&self) -> bool {
        *self.state.read().await == VaultState::Unlocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn vault_starts_locked() {
        let manager = VaultStateManager::new(None);
        assert_eq!(manager.state().await, VaultState::Locked);
    }

    #[tokio::test]
    async fn vault_unlock_and_lock() {
        let manager = VaultStateManager::new(None);
        
        let key = SecureKey::new([0u8; 32]);
        manager.unlock(key).await.unwrap();
        assert_eq!(manager.state().await, VaultState::Unlocked);
        
        manager.lock().await;
        assert_eq!(manager.state().await, VaultState::Locked);
    }

    #[tokio::test]
    async fn key_accessible_when_unlocked() {
        let manager = VaultStateManager::new(None);
        
        assert!(manager.get_key().await.is_none());
        
        let key = SecureKey::new([42u8; 32]);
        manager.unlock(key).await.unwrap();
        
        let retrieved = manager.get_key().await.unwrap();
        assert_eq!(retrieved.as_bytes()[0], 42);
    }

    #[tokio::test]
    async fn key_cleared_on_lock() {
        let manager = VaultStateManager::new(None);
        
        let key = SecureKey::new([42u8; 32]);
        manager.unlock(key).await.unwrap();
        
        manager.lock().await;
        assert!(manager.get_key().await.is_none());
    }
}
