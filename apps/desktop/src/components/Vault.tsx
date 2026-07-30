import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

interface VaultProps {
  project: Project;
}

interface VaultEntryMetadata {
  id: string;
  name: string;
  secret_type: string;
  status: string;
}

interface EnvPreviewEntry {
  env_key: string;
  vault_entry_name: string;
  secret_type: string;
}

interface EnvPreviewReport {
  file_path: string;
  will_import: EnvPreviewEntry[];
  skipped: string[];
}

function Vault({ project: _project }: VaultProps) {
  const [entries, setEntries] = useState<VaultEntryMetadata[]>([]);
  const [vaultLocked, setVaultLocked] = useState(true);
  const [envPath, setEnvPath] = useState('.env');
  const [notice, setNotice] = useState<string | null>(null);
  const [preview, setPreview] = useState<EnvPreviewReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);

  useEffect(() => {
    checkVaultState();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [_project.path]);

  const checkVaultState = async () => {
    try {
      const state = await invoke<{ state: string }>('vault_state');
      setVaultLocked(state.state !== 'unlocked');
      if (state.state === 'unlocked') {
        loadVaultEntries();
      }
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const loadVaultEntries = async () => {
    setError(null);
    try {
      const result = await invoke<VaultEntryMetadata[]>('vault_list');
      setEntries(result);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const handleUnlock = async () => {
    setIsBusy(true);
    setError(null);
    try {
      await invoke('vault_unlock', { request: { passphrase: null } });
      setVaultLocked(false);
      await loadVaultEntries();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handleLock = async () => {
    setIsBusy(true);
    setError(null);
    try {
      await invoke('vault_lock');
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setVaultLocked(true);
      setEntries([]);
      setIsBusy(false);
    }
  };

  const handleAddCredential = async () => {
    const name = window.prompt('Credential name (e.g. staging-token):');
    if (!name) return;
    const secretType = window.prompt(
      'Secret type (api_key, bearer_token, basic_auth, oauth_token):',
      'bearer_token'
    );
    if (!secretType) return;
    const value = window.prompt('Secret value:');
    if (!value) return;

    setError(null);
    try {
      await invoke('vault_create', { request: { name, secretType, value } });
      await loadVaultEntries();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const handleDelete = async (name: string) => {
    setError(null);
    try {
      await invoke('vault_delete', { request: { name } });
      await loadVaultEntries();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const handleImportEnv = async () => {
    setIsBusy(true);
    setError(null);
    setNotice(null);
    try {
      const report = await invoke<{
        file_path: string;
        imported: string[];
        skipped: string[];
      }>('vault_import_env', {
        request: { path: envPath.trim() || null },
      });

      await loadVaultEntries();

      setNotice(
        `Imported ${report.imported.length} auth secret(s) from ${report.file_path}.` +
          (report.skipped.length > 0
            ? ` Skipped ${report.skipped.length} non-auth variable(s).`
            : '')
      );
      setPreview(null);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handlePreviewEnv = async () => {
    setIsBusy(true);
    setError(null);
    setNotice(null);
    try {
      const report = await invoke<EnvPreviewReport>('vault_preview_env', {
        request: { path: envPath.trim() || null },
      });
      setPreview(report);
      setNotice(
        `Preview loaded from ${report.file_path}: ${report.will_import.length} import candidate(s).`
      );
    } catch (err) {
      setError(errorMessage(err));
      setPreview(null);
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <div>
      <h2>Vault</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        Securely store and manage API credentials
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      {notice && (
        <div className="success-banner">
          <span>{notice}</span>
          <button onClick={() => setNotice(null)}>&times;</button>
        </div>
      )}

      <div style={{ marginBottom: '1rem', display: 'flex', gap: '0.5rem' }}>
        {vaultLocked ? (
          <button className="control-button primary" onClick={handleUnlock} disabled={isBusy}>
            Unlock Vault
          </button>
        ) : (
          <>
            <button className="control-button" onClick={handleLock} disabled={isBusy}>
              Lock Vault
            </button>
            <button className="control-button primary" onClick={handleAddCredential}>
              Add Credential
            </button>
          </>
        )}
      </div>

      {!vaultLocked && (
        <div className="runtime-card" style={{ marginBottom: '1rem' }}>
          <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>Import From .env</h3>
          <p style={{ color: '#6c757d', marginBottom: '0.5rem' }}>
            Loads auth-style variables (API keys/tokens) into Vault entries.
          </p>
          <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', flexWrap: 'wrap' }}>
            <input
              className="url-input"
              style={{ maxWidth: '420px' }}
              value={envPath}
              onChange={(event) => setEnvPath(event.target.value)}
              placeholder=".env or config/.env.local"
            />
            <button className="control-button" onClick={handlePreviewEnv} disabled={isBusy}>
              Preview Import
            </button>
            <button className="control-button" onClick={handleImportEnv} disabled={isBusy}>
              Import Auth Secrets
            </button>
          </div>

          {preview && (
            <div style={{ marginTop: '0.75rem' }}>
              <p style={{ color: '#6c757d', marginBottom: '0.5rem' }}>
                Will import {preview.will_import.length} variable(s):
              </p>
              {preview.will_import.length === 0 ? (
                <p style={{ color: '#6c757d' }}>No auth variables detected.</p>
              ) : (
                <div style={{ display: 'grid', gap: '0.4rem' }}>
                  {preview.will_import.map((item) => (
                    <div key={`${item.env_key}:${item.vault_entry_name}`} style={{ fontSize: '0.85rem' }}>
                      <strong>{item.env_key}</strong> maps to {item.vault_entry_name} ({item.secret_type})
                    </div>
                  ))}
                </div>
              )}
              {preview.skipped.length > 0 && (
                <p style={{ color: '#6c757d', marginTop: '0.5rem' }}>
                  Skipped non-auth vars: {preview.skipped.join(', ')}
                </p>
              )}
            </div>
          )}
        </div>
      )}

      {vaultLocked ? (
        <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
          <p>Vault is locked. Unlock to view credentials.</p>
          <p style={{ fontSize: '0.875rem', marginTop: '0.5rem' }}>
            Auto-lock: 15 minutes of inactivity
          </p>
        </div>
      ) : (
        <div className="vault-list">
          {entries.map((entry) => (
            <div key={entry.id} className="vault-item">
              <div className="vault-item-info">
                <span className="vault-item-name">{entry.name}</span>
                <span className="vault-item-type">{entry.secret_type}</span>
              </div>
              <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                <span className={`vault-status ${entry.status}`}>
                  {entry.status === 'available' ? 'Available' : entry.status}
                </span>
                <button
                  className="control-button"
                  style={{ fontSize: '0.75rem' }}
                  onClick={() => handleDelete(entry.name)}
                >
                  Delete
                </button>
              </div>
            </div>
          ))}

          {entries.length === 0 && (
            <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
              No credentials stored. Add a credential to get started.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default Vault;
