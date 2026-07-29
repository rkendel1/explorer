use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use uuid::Uuid;

const SERVICE_NAME: &str = "repo-api-vault";
const LOCAL_KEY_FILE: &str = ".repo-api/vault/.master_key";
const VAULT_DB_FILE: &str = ".repo-api/vault/encrypted.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub id: String,
    pub name: String,
    pub secret_type: SecretType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretType {
    ApiKey,
    OAuthToken,
    BearerToken,
    BasicAuth,
    DatabaseCredential,
    Certificate,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredVault {
    entries: Vec<StoredVaultEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredVaultEntry {
    entry: VaultEntry,
    nonce: String,
    ciphertext: String,
}

pub struct VaultStore {
    root: std::path::PathBuf,
    key: [u8; 32],
}

impl VaultStore {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        let key = load_or_create_master_key(root)?;
        let db_path = root.join(VAULT_DB_FILE);
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !db_path.exists() {
            let empty = StoredVault { entries: vec![] };
            fs::write(&db_path, serde_json::to_vec_pretty(&empty)?)?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            key,
        })
    }

    pub fn list_entries(&self) -> anyhow::Result<Vec<VaultEntry>> {
        let db = self.load_db()?;
        Ok(db.entries.into_iter().map(|record| record.entry).collect())
    }

    pub fn upsert_secret(
        &self,
        name: &str,
        secret_type: SecretType,
        secret: &str,
    ) -> anyhow::Result<VaultEntry> {
        let mut db = self.load_db()?;
        let now = Utc::now();
        let encrypted = encrypt(&self.key, secret)?;

        if let Some(existing) = db
            .entries
            .iter_mut()
            .find(|record| record.entry.name == name)
        {
            existing.entry.secret_type = secret_type;
            existing.entry.updated_at = now;
            existing.nonce = encrypted.0;
            existing.ciphertext = encrypted.1;
            let entry = existing.entry.clone();
            self.save_db(&db)?;
            return Ok(entry);
        }

        let entry = VaultEntry {
            id: format!("sec_{}", Uuid::new_v4().simple()),
            name: name.to_string(),
            secret_type,
            created_at: now,
            updated_at: now,
        };

        db.entries.push(StoredVaultEntry {
            entry: entry.clone(),
            nonce: encrypted.0,
            ciphertext: encrypted.1,
        });

        self.save_db(&db)?;
        Ok(entry)
    }

    pub fn resolve_secret(&self, name: &str) -> anyhow::Result<String> {
        let db = self.load_db()?;
        let stored = db
            .entries
            .iter()
            .find(|record| record.entry.name == name)
            .ok_or_else(|| anyhow::anyhow!("vault entry '{name}' not found"))?;

        decrypt(&self.key, &stored.nonce, &stored.ciphertext)
    }

    pub fn delete_secret(&self, name: &str) -> anyhow::Result<bool> {
        let mut db = self.load_db()?;
        let before = db.entries.len();
        db.entries.retain(|record| record.entry.name != name);
        let removed = db.entries.len() != before;
        if removed {
            self.save_db(&db)?;
        }
        Ok(removed)
    }

    fn db_file(&self) -> std::path::PathBuf {
        self.root.join(VAULT_DB_FILE)
    }

    fn load_db(&self) -> anyhow::Result<StoredVault> {
        Ok(serde_json::from_slice(&fs::read(self.db_file())?)?)
    }

    fn save_db(&self, db: &StoredVault) -> anyhow::Result<()> {
        fs::write(self.db_file(), serde_json::to_vec_pretty(db)?)?;
        Ok(())
    }
}

fn encrypt(key: &[u8; 32], plaintext: &str) -> anyhow::Result<(String, String)> {
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("failed to encrypt vault secret"))?;
    Ok((
        STANDARD.encode(nonce_bytes),
        STANDARD.encode(ciphertext.as_slice()),
    ))
}

fn decrypt(key: &[u8; 32], nonce_b64: &str, ciphertext_b64: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let nonce_raw = STANDARD.decode(nonce_b64)?;
    let nonce = Nonce::from_slice(&nonce_raw);
    let ciphertext = STANDARD.decode(ciphertext_b64)?;
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("failed to decrypt vault secret"))?;
    Ok(String::from_utf8(plaintext)?)
}

fn load_or_create_master_key(root: &Path) -> anyhow::Result<[u8; 32]> {
    let account = build_account_name(root);
    if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, &account)
        && let Ok(encoded_key) = entry.get_password()
    {
        return decode_key(&encoded_key);
    }

    let fallback_file = root.join(LOCAL_KEY_FILE);
    if fallback_file.exists() {
        return decode_key(fs::read_to_string(fallback_file)?.trim());
    }

    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    let encoded = STANDARD.encode(key);

    if let Ok(entry) = keyring::Entry::new(SERVICE_NAME, &account) {
        let _ = entry.set_password(&encoded);
    }

    if let Some(parent) = fallback_file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&fallback_file, &encoded)?;

    Ok(key)
}

fn build_account_name(root: &Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    format!("workspace-{}", STANDARD.encode(&digest[..12]))
}

fn decode_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let decoded = STANDARD.decode(value)?;
    if decoded.len() != 32 {
        anyhow::bail!("invalid vault master key length");
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

pub fn redact(secret: &str) -> String {
    if secret.is_empty() {
        return "".into();
    }
    let visible = secret.chars().count().min(4);
    let suffix: String = secret
        .chars()
        .rev()
        .take(visible)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••{}", suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_encrypted_secrets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = VaultStore::open(dir.path()).expect("open");

        let entry = vault
            .upsert_secret("staging-token", SecretType::BearerToken, "token-value")
            .expect("upsert");
        assert_eq!(entry.name, "staging-token");

        let resolved = vault.resolve_secret("staging-token").expect("resolve");
        assert_eq!(resolved, "token-value");

        let db_content = fs::read_to_string(dir.path().join(VAULT_DB_FILE)).expect("read db");
        assert!(!db_content.contains("token-value"));
    }
}
