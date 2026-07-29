//! Key providers for vault encryption.
//!
//! This module provides:
//! - OS keychain integration (macOS Keychain, Windows Credential Manager, Linux Secret Service)
//! - Argon2id passphrase-based key derivation for fallback
//! - Key provider abstraction

use argon2::{Argon2, Params};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use thiserror::Error;

/// Vault metadata file name
pub const METADATA_FILE: &str = ".repo-api/vault/metadata.json";

/// Argon2id parameters for key derivation
const ARGON2_M_COST: u32 = 65536; // 64 MB
const ARGON2_T_COST: u32 = 3; // 3 iterations
const ARGON2_P_COST: u32 = 4; // 4 parallel lanes
const SALT_LENGTH: usize = 32;

/// Key provider errors
#[derive(Error, Debug)]
pub enum KeyProviderError {
    #[error("OS keychain unavailable")]
    KeychainUnavailable,
    #[error("passphrase required")]
    PassphraseRequired,
    #[error("invalid passphrase")]
    InvalidPassphrase,
    #[error("key derivation failed: {0}")]
    DerivationFailed(String),
    #[error("metadata corrupted")]
    MetadataCorrupted,
    #[error("migration required from legacy format")]
    MigrationRequired,
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Key provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyProviderType {
    /// OS keychain (preferred)
    SystemKeychain,
    /// Argon2id passphrase-derived key (fallback)
    Argon2id,
    /// Legacy plaintext key file (requires migration)
    LegacyFile,
}

/// Vault metadata stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultMetadata {
    pub version: u32,
    pub key_provider: KeyProviderType,
    /// Salt for Argon2id (only present when using passphrase)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub salt: Option<String>,
    /// Verification hash to check passphrase correctness
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_hash: Option<String>,
}

impl Default for VaultMetadata {
    fn default() -> Self {
        Self {
            version: 2,
            key_provider: KeyProviderType::SystemKeychain,
            salt: None,
            verification_hash: None,
        }
    }
}

/// Key provider trait for abstracting key storage
pub trait KeyProvider: Send + Sync {
    /// Get the encryption key
    fn get_key(&self) -> Result<[u8; 32], KeyProviderError>;
    
    /// Store a new encryption key
    fn store_key(&self, key: &[u8; 32]) -> Result<(), KeyProviderError>;
    
    /// Check if provider is available
    fn is_available(&self) -> bool;
    
    /// Get provider type
    fn provider_type(&self) -> KeyProviderType;
}

/// OS keychain key provider
pub struct KeychainProvider {
    service_name: String,
    account_name: String,
}

impl KeychainProvider {
    pub fn new(root: &Path) -> Self {
        let account = build_account_name(root);
        Self {
            service_name: "repo-api-vault".to_string(),
            account_name: account,
        }
    }
}

impl KeyProvider for KeychainProvider {
    fn get_key(&self) -> Result<[u8; 32], KeyProviderError> {
        let entry = keyring::Entry::new(&self.service_name, &self.account_name)
            .map_err(|_| KeyProviderError::KeychainUnavailable)?;
        
        let encoded = entry
            .get_password()
            .map_err(|_| KeyProviderError::KeychainUnavailable)?;
        
        decode_key(&encoded)
    }

    fn store_key(&self, key: &[u8; 32]) -> Result<(), KeyProviderError> {
        let entry = keyring::Entry::new(&self.service_name, &self.account_name)
            .map_err(|_| KeyProviderError::KeychainUnavailable)?;
        
        let encoded = STANDARD.encode(key);
        entry
            .set_password(&encoded)
            .map_err(|_| KeyProviderError::KeychainUnavailable)?;
        
        Ok(())
    }

    fn is_available(&self) -> bool {
        keyring::Entry::new(&self.service_name, &self.account_name).is_ok()
    }

    fn provider_type(&self) -> KeyProviderType {
        KeyProviderType::SystemKeychain
    }
}

/// Argon2id passphrase-based key provider
pub struct Argon2Provider {
    salt: [u8; SALT_LENGTH],
    verification_hash: String,
}

impl Argon2Provider {
    /// Create a new Argon2 provider with the given salt and verification
    pub fn new(salt: [u8; SALT_LENGTH], verification_hash: String) -> Self {
        Self {
            salt,
            verification_hash,
        }
    }

    /// Initialize a new Argon2 provider with a passphrase
    pub fn init(passphrase: &str) -> Result<(Self, [u8; 32]), KeyProviderError> {
        // Generate random salt
        let mut salt = [0u8; SALT_LENGTH];
        rand::rng().fill_bytes(&mut salt);

        // Derive key
        let key = derive_key_argon2(passphrase, &salt)?;
        
        // Create verification hash
        let verification = create_verification_hash(&key);

        let provider = Self {
            salt,
            verification_hash: verification,
        };

        Ok((provider, key))
    }

    /// Load provider from metadata and derive key
    pub fn from_metadata_and_passphrase(
        metadata: &VaultMetadata,
        passphrase: &str,
    ) -> Result<(Self, [u8; 32]), KeyProviderError> {
        let salt_encoded = metadata
            .salt
            .as_ref()
            .ok_or(KeyProviderError::MetadataCorrupted)?;
        
        let salt_bytes = STANDARD
            .decode(salt_encoded)
            .map_err(|_| KeyProviderError::MetadataCorrupted)?;
        
        if salt_bytes.len() != SALT_LENGTH {
            return Err(KeyProviderError::MetadataCorrupted);
        }
        
        let mut salt = [0u8; SALT_LENGTH];
        salt.copy_from_slice(&salt_bytes);

        // Derive key
        let key = derive_key_argon2(passphrase, &salt)?;
        
        // Verify passphrase
        let verification = create_verification_hash(&key);
        if metadata.verification_hash.as_ref() != Some(&verification) {
            return Err(KeyProviderError::InvalidPassphrase);
        }

        let provider = Self {
            salt,
            verification_hash: verification,
        };

        Ok((provider, key))
    }

    /// Get the salt as base64
    pub fn salt_base64(&self) -> String {
        STANDARD.encode(self.salt)
    }

    /// Get the verification hash
    pub fn verification_hash(&self) -> &str {
        &self.verification_hash
    }
}

impl KeyProvider for Argon2Provider {
    fn get_key(&self) -> Result<[u8; 32], KeyProviderError> {
        // This provider requires the passphrase to be provided externally
        // It cannot retrieve the key on its own
        Err(KeyProviderError::PassphraseRequired)
    }

    fn store_key(&self, _key: &[u8; 32]) -> Result<(), KeyProviderError> {
        // Key storage is not applicable for passphrase-derived keys
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn provider_type(&self) -> KeyProviderType {
        KeyProviderType::Argon2id
    }
}

/// Derive a key using Argon2id
pub fn derive_key_argon2(passphrase: &str, salt: &[u8; SALT_LENGTH]) -> Result<[u8; 32], KeyProviderError> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(32))
        .map_err(|e| KeyProviderError::DerivationFailed(e.to_string()))?;
    
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    
    let mut output_key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut output_key)
        .map_err(|e| KeyProviderError::DerivationFailed(e.to_string()))?;
    
    Ok(output_key)
}

/// Create a verification hash for checking passphrase correctness
fn create_verification_hash(key: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"repo-api-vault-verify:");
    hasher.update(key);
    let result = hasher.finalize();
    STANDARD.encode(&result[..16])
}

/// Build a unique account name for keychain storage
fn build_account_name(root: &Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    format!("workspace-{}", STANDARD.encode(&digest[..12]))
}

/// Decode a base64 key
fn decode_key(value: &str) -> Result<[u8; 32], KeyProviderError> {
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| KeyProviderError::MetadataCorrupted)?;
    if decoded.len() != 32 {
        return Err(KeyProviderError::MetadataCorrupted);
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

/// Load vault metadata from disk
pub fn load_metadata(root: &Path) -> Result<Option<VaultMetadata>, KeyProviderError> {
    let path = root.join(METADATA_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read(&path)?;
    let metadata: VaultMetadata = serde_json::from_slice(&content)?;
    Ok(Some(metadata))
}

/// Save vault metadata to disk
pub fn save_metadata(root: &Path, metadata: &VaultMetadata) -> Result<(), KeyProviderError> {
    let path = root.join(METADATA_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(metadata)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Check for legacy plaintext key file
pub fn has_legacy_key_file(root: &Path) -> bool {
    root.join(".repo-api/vault/.master_key").exists()
}

/// Migrate from legacy plaintext key file
pub fn migrate_legacy_key(root: &Path) -> Result<([u8; 32], KeyProviderType), KeyProviderError> {
    let legacy_path = root.join(".repo-api/vault/.master_key");
    if !legacy_path.exists() {
        return Err(KeyProviderError::KeychainUnavailable);
    }
    
    let encoded = fs::read_to_string(&legacy_path)?;
    let key = decode_key(encoded.trim())?;
    
    // Try to migrate to keychain first
    let keychain = KeychainProvider::new(root);
    if keychain.is_available() {
        if keychain.store_key(&key).is_ok() {
            // Remove legacy file securely
            let mut zeros = vec![0u8; encoded.len()];
            rand::rng().fill_bytes(&mut zeros);
            let _ = fs::write(&legacy_path, &zeros);
            let _ = fs::remove_file(&legacy_path);
            
            return Ok((key, KeyProviderType::SystemKeychain));
        }
    }
    
    // Cannot migrate automatically to Argon2id without user passphrase
    // Signal that migration is needed
    Err(KeyProviderError::MigrationRequired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_key_derivation() {
        let salt = [1u8; SALT_LENGTH];
        let key1 = derive_key_argon2("test-passphrase", &salt).unwrap();
        let key2 = derive_key_argon2("test-passphrase", &salt).unwrap();
        let key3 = derive_key_argon2("different-passphrase", &salt).unwrap();
        
        // Same passphrase + salt = same key
        assert_eq!(key1, key2);
        // Different passphrase = different key
        assert_ne!(key1, key3);
    }

    #[test]
    fn argon2_provider_init_and_verify() {
        let (provider, key) = Argon2Provider::init("my-secure-passphrase").unwrap();
        
        // Create metadata
        let metadata = VaultMetadata {
            version: 2,
            key_provider: KeyProviderType::Argon2id,
            salt: Some(provider.salt_base64()),
            verification_hash: Some(provider.verification_hash().to_string()),
        };
        
        // Correct passphrase should work
        let (_, key2) = Argon2Provider::from_metadata_and_passphrase(&metadata, "my-secure-passphrase").unwrap();
        assert_eq!(key, key2);
        
        // Wrong passphrase should fail
        let result = Argon2Provider::from_metadata_and_passphrase(&metadata, "wrong-passphrase");
        assert!(matches!(result, Err(KeyProviderError::InvalidPassphrase)));
    }

    #[test]
    fn verification_hash_is_consistent() {
        let key = [42u8; 32];
        let hash1 = create_verification_hash(&key);
        let hash2 = create_verification_hash(&key);
        assert_eq!(hash1, hash2);
        
        let different_key = [43u8; 32];
        let hash3 = create_verification_hash(&different_key);
        assert_ne!(hash1, hash3);
    }
}
