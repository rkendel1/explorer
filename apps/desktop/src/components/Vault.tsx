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

interface EnvEditablePreviewEntry {
  source_file: string;
  env_key: string;
  vault_entry_name: string;
  secret_type: string;
  value: string;
}

interface EnvEditablePreviewReport {
  entries: EnvEditablePreviewEntry[];
  skipped: string[];
}

interface EnvFileCandidate {
  path: string;
  relative_path: string;
}

function Vault({ project: _project }: VaultProps) {
  const [entries, setEntries] = useState<VaultEntryMetadata[]>([]);
  const [vaultLocked, setVaultLocked] = useState(true);
  const [envPath, setEnvPath] = useState('.env');
  const [envCandidates, setEnvCandidates] = useState<EnvFileCandidate[]>([]);
  const [selectedEnvFiles, setSelectedEnvFiles] = useState<Record<string, boolean>>({});
  const [editablePreviewEntries, setEditablePreviewEntries] = useState<EnvEditablePreviewEntry[]>([]);
  const [selectedPreviewRows, setSelectedPreviewRows] = useState<Record<string, boolean>>({});
  const [includeAllEnvVars, setIncludeAllEnvVars] = useState(true);
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
      await loadEnvCandidates();
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

  const loadEnvCandidates = async () => {
    try {
      const files = await invoke<EnvFileCandidate[]>('vault_env_files');
      setEnvCandidates(files);

      const fileSelection: Record<string, boolean> = {};
      files.forEach((file) => {
        fileSelection[file.relative_path] = false;
      });

      if (files.length > 0) {
        const preferred =
          files.find((file) => file.relative_path === '.env') ??
          files.find((file) => file.relative_path.endsWith('/.env')) ??
          files.find((file) => file.relative_path.includes('.env.local')) ??
          files[0];

        fileSelection[preferred.relative_path] = true;
        setSelectedEnvFiles(fileSelection);

        setEnvPath((current) => {
          const trimmed = current.trim();
          if (!trimmed || trimmed === '.env') {
            return preferred.relative_path;
          }
          return current;
        });
      }
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const selectedFilePaths = () =>
    Object.entries(selectedEnvFiles)
      .filter(([, selected]) => selected)
      .map(([path]) => path);

  const previewRowId = (entry: EnvEditablePreviewEntry) =>
    `${entry.source_file}:${entry.env_key}:${entry.vault_entry_name}`;

  const handlePreviewSelectedFiles = async () => {
    const paths = selectedFilePaths();
    if (paths.length === 0) {
      setError('Select at least one env file location.');
      return;
    }

    setIsBusy(true);
    setError(null);
    setNotice(null);
    try {
      const report = await invoke<EnvEditablePreviewReport>('vault_preview_env_files', {
        request: { paths, includeAll: includeAllEnvVars },
      });

      setEditablePreviewEntries(report.entries);
      const rowSelection: Record<string, boolean> = {};
      report.entries.forEach((entry) => {
        rowSelection[previewRowId(entry)] = true;
      });
      setSelectedPreviewRows(rowSelection);
      setPreview(null);

      setNotice(
        `Loaded ${report.entries.length} variable(s) from ${paths.length} env file location(s).`
      );
    } catch (err) {
      setError(errorMessage(err));
      setEditablePreviewEntries([]);
    } finally {
      setIsBusy(false);
    }
  };

  const updateEditableEntry = (
    rowId: string,
    patch: Partial<EnvEditablePreviewEntry>
  ) => {
    setEditablePreviewEntries((current) =>
      current.map((entry) =>
        previewRowId(entry) === rowId ? { ...entry, ...patch } : entry
      )
    );
  };

  const handleImportSelectedPreviewRows = async () => {
    const rows = editablePreviewEntries.filter((entry) => selectedPreviewRows[previewRowId(entry)]);
    if (rows.length === 0) {
      setError('Select at least one variable row to import.');
      return;
    }

    setIsBusy(true);
    setError(null);
    setNotice(null);
    try {
      for (const entry of rows) {
        const name = entry.vault_entry_name.trim();
        if (!name) {
          throw new Error(`Vault entry name cannot be empty for ${entry.env_key}`);
        }
        await invoke('vault_create', {
          request: {
            name,
            secretType: entry.secret_type,
            value: entry.value,
          },
        });
      }
      await loadVaultEntries();
      setNotice(`Imported ${rows.length} variable(s) from selected rows.`);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
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
        request: { path: envPath.trim() || null, includeAll: includeAllEnvVars },
      });

      await loadVaultEntries();

      setNotice(
        `Imported ${report.imported.length} variable(s) from ${report.file_path}.` +
          (report.skipped.length > 0
            ? ` Skipped ${report.skipped.length} variable(s).`
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
        request: { path: envPath.trim() || null, includeAll: includeAllEnvVars },
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
            Load project variables into Vault entries. Known auth variables are typed automatically.
          </p>
          {envCandidates.length > 0 && (
            <div style={{ marginBottom: '0.5rem' }}>
              <p style={{ color: '#6c757d', marginBottom: '0.4rem' }}>Discovered env file locations:</p>
              <div style={{ display: 'grid', gap: '0.3rem', maxHeight: '180px', overflow: 'auto', border: '1px solid #e9ecef', borderRadius: '8px', padding: '0.5rem' }}>
                {envCandidates.map((candidate) => (
                  <label key={candidate.path} style={{ display: 'flex', alignItems: 'center', gap: '0.45rem' }}>
                    <input
                      type="checkbox"
                      checked={selectedEnvFiles[candidate.relative_path] ?? false}
                      onChange={(event) =>
                        setSelectedEnvFiles((prev) => ({
                          ...prev,
                          [candidate.relative_path]: event.target.checked,
                        }))
                      }
                    />
                    <span style={{ fontFamily: 'monospace', fontSize: '0.82rem' }}>
                      {candidate.relative_path}
                    </span>
                  </label>
                ))}
              </div>
              <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.5rem', flexWrap: 'wrap' }}>
                <button className="control-button" onClick={handlePreviewSelectedFiles} disabled={isBusy}>
                  Preview Selected Files
                </button>
                <button
                  className="control-button"
                  onClick={() => {
                    const all: Record<string, boolean> = {};
                    envCandidates.forEach((candidate) => {
                      all[candidate.relative_path] = true;
                    });
                    setSelectedEnvFiles(all);
                  }}
                  disabled={isBusy}
                >
                  Select All
                </button>
                <button
                  className="control-button"
                  onClick={() => {
                    const none: Record<string, boolean> = {};
                    envCandidates.forEach((candidate) => {
                      none[candidate.relative_path] = false;
                    });
                    setSelectedEnvFiles(none);
                  }}
                  disabled={isBusy}
                >
                  Clear Selection
                </button>
              </div>
            </div>
          )}

          <div style={{ marginBottom: '0.5rem', display: 'flex', gap: '0.5rem', alignItems: 'center', flexWrap: 'wrap' }}>
            <input
              className="url-input"
              style={{ maxWidth: '420px' }}
              value={envPath}
              onChange={(event) => setEnvPath(event.target.value)}
              placeholder=".env, .env.local, services/api/.env.example"
            />
            <button className="control-button" onClick={handlePreviewEnv} disabled={isBusy}>
              Preview Single File
            </button>
            <button className="control-button" onClick={handleImportEnv} disabled={isBusy}>
              Import Single File
            </button>
            <button className="control-button" onClick={loadEnvCandidates} disabled={isBusy}>
              Re-scan Env Files
            </button>
          </div>

          {editablePreviewEntries.length > 0 && (
            <div style={{ marginTop: '0.75rem' }}>
              <p style={{ color: '#6c757d', marginBottom: '0.5rem' }}>
                Select rows to import. You can manually edit vault name, secret type, and value before importing.
              </p>
              <div style={{ display: 'grid', gap: '0.4rem' }}>
                {editablePreviewEntries.map((entry) => {
                  const rowId = previewRowId(entry);
                  return (
                    <div
                      key={rowId}
                      style={{
                        display: 'grid',
                        gridTemplateColumns: '28px 1.2fr 1fr 1fr 1fr',
                        gap: '0.45rem',
                        alignItems: 'center',
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={selectedPreviewRows[rowId] ?? false}
                        onChange={(event) =>
                          setSelectedPreviewRows((prev) => ({
                            ...prev,
                            [rowId]: event.target.checked,
                          }))
                        }
                      />
                      <input className="url-input" value={entry.source_file} readOnly />
                      <input
                        className="url-input"
                        value={entry.vault_entry_name}
                        onChange={(event) =>
                          updateEditableEntry(rowId, { vault_entry_name: event.target.value })
                        }
                        title={entry.env_key}
                      />
                      <input
                        className="url-input"
                        value={entry.secret_type}
                        onChange={(event) =>
                          updateEditableEntry(rowId, { secret_type: event.target.value })
                        }
                      />
                      <input
                        className="url-input"
                        value={entry.value}
                        onChange={(event) =>
                          updateEditableEntry(rowId, { value: event.target.value })
                        }
                      />
                    </div>
                  );
                })}
              </div>
              <div style={{ marginTop: '0.6rem', display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
                <button className="control-button" onClick={handleImportSelectedPreviewRows} disabled={isBusy}>
                  Import Selected Rows
                </button>
                <button
                  className="control-button"
                  onClick={() => {
                    const allRows: Record<string, boolean> = {};
                    editablePreviewEntries.forEach((entry) => {
                      allRows[previewRowId(entry)] = true;
                    });
                    setSelectedPreviewRows(allRows);
                  }}
                  disabled={isBusy}
                >
                  Select All Rows
                </button>
                <button
                  className="control-button"
                  onClick={() => setSelectedPreviewRows({})}
                  disabled={isBusy}
                >
                  Clear Row Selection
                </button>
              </div>
            </div>
          )}
          <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.5rem' }}>
            <input
              type="checkbox"
              checked={includeAllEnvVars}
              onChange={(event) => setIncludeAllEnvVars(event.target.checked)}
            />
            Import all variables (not just auth)
          </label>

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
                  Skipped vars: {preview.skipped.join(', ')}
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
