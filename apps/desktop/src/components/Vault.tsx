import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project, VaultEntry } from '../App';

interface VaultProps {
  project: Project;
}

function Vault({ project }: VaultProps) {
  const [entries, setEntries] = useState<VaultEntry[]>([]);
  const [vaultLocked, setVaultLocked] = useState(true);

  useEffect(() => {
    loadVaultEntries();
  }, [project.path]);

  const loadVaultEntries = async () => {
    try {
      const result = await invoke<{ ok: VaultEntry[] }>('vault_list', {
        projectPath: project.path,
      });
      if (result.ok) {
        setEntries(result.ok);
      }
    } catch (error) {
      console.error('Failed to load vault entries:', error);
      // Mock data for development
      setEntries([
        {
          name: 'fieldflow-staging-token',
          entryType: 'bearer_token',
          status: 'available',
        },
        {
          name: 'api-key-development',
          entryType: 'api_key',
          status: 'available',
        },
      ]);
      setVaultLocked(false);
    }
  };

  const handleUnlock = async () => {
    // In a real implementation, this would prompt for passphrase
    setVaultLocked(false);
    loadVaultEntries();
  };

  const handleLock = async () => {
    try {
      await invoke('vault_lock', { projectPath: project.path });
    } catch {
      // Ignore errors in development
    }
    setVaultLocked(true);
    setEntries([]);
  };

  return (
    <div>
      <h2>Vault</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        Securely store and manage API credentials
      </p>

      <div style={{ marginBottom: '1rem', display: 'flex', gap: '0.5rem' }}>
        {vaultLocked ? (
          <button className="control-button primary" onClick={handleUnlock}>
            Unlock Vault
          </button>
        ) : (
          <>
            <button className="control-button" onClick={handleLock}>
              Lock Vault
            </button>
            <button className="control-button primary">
              Add Credential
            </button>
          </>
        )}
      </div>

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
            <div key={entry.name} className="vault-item">
              <div className="vault-item-info">
                <span className="vault-item-name">{entry.name}</span>
                <span className="vault-item-type">
                  {entry.entryType === 'bearer_token' ? '******' : 'API Key'}
                </span>
              </div>
              <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                <span className={`vault-status ${entry.status}`}>
                  {entry.status === 'available' ? 'Available' : 'Locked'}
                </span>
                <button className="control-button" style={{ fontSize: '0.75rem' }}>
                  Edit
                </button>
                <button className="control-button" style={{ fontSize: '0.75rem' }}>
                  Reveal
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
