import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

interface ChangesProps {
  project: Project;
}

interface ChangeEntry {
  kind: string;
  description: string;
  path: string | null;
}

interface ContractChangeSummary {
  total_changes: number;
  added: ChangeEntry[];
  modified: ChangeEntry[];
  removed: ChangeEntry[];
  potentially_breaking: ChangeEntry[];
}

function Changes({ project: _project }: ChangesProps) {
  const [summary, setSummary] = useState<ContractChangeSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    loadChanges();
  }, [_project.path]);

  const loadChanges = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<ContractChangeSummary>('change_list');
      setSummary(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleAccept = async (changeId: string) => {
    setError(null);
    try {
      await invoke('change_accept', { request: { changeId } });
      await loadChanges();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const handleReject = async (changeId: string) => {
    setError(null);
    try {
      await invoke('change_reject', { request: { changeId, reason: null } });
      await loadChanges();
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const breakingPaths = new Set((summary?.potentially_breaking ?? []).map((c) => c.path));
  const allChanges = [
    ...(summary?.added ?? []),
    ...(summary?.modified ?? []),
    ...(summary?.removed ?? []),
  ];

  return (
    <div>
      <h2>Contract Changes</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {isLoading
          ? 'Loading...'
          : `${summary?.total_changes ?? 0} changes detected`}
        {summary && summary.potentially_breaking.length > 0 && (
          <span style={{ color: '#991b1b', marginLeft: '0.5rem' }}>
            • {summary.potentially_breaking.length} potentially breaking
          </span>
        )}
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      {!isLoading && allChanges.length === 0 ? (
        <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
          No pending changes. Your contract is up to date.
        </div>
      ) : (
        <div className="change-list">
          {allChanges.map((change) => (
            <div key={change.path ?? change.description} className="change-item">
              <div className="change-info">
                <span className={`change-type ${change.kind}`}>{change.kind}</span>
                <div>
                  <div style={{ fontWeight: 500 }}>{change.path ?? change.description}</div>
                  <div style={{ fontSize: '0.875rem', color: '#6c757d' }}>
                    {change.description}
                    {change.path && breakingPaths.has(change.path) && (
                      <span style={{ color: '#991b1b', marginLeft: '0.5rem' }}>
                        ⚠ Breaking
                      </span>
                    )}
                  </div>
                </div>
              </div>
              {change.path && (
                <div className="change-actions">
                  <button className="control-button" onClick={() => handleAccept(change.path!)}>
                    Accept
                  </button>
                  <button className="control-button danger" onClick={() => handleReject(change.path!)}>
                    Reject
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default Changes;
