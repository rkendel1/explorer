import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project } from '../App';

interface ChangesProps {
  project: Project;
}

interface Change {
  id: string;
  changeType: 'added' | 'modified' | 'removed';
  path: string;
  description: string;
  breaking: boolean;
}

function Changes({ project }: ChangesProps) {
  const [changes, setChanges] = useState<Change[]>([]);

  useEffect(() => {
    loadChanges();
  }, [project.path]);

  const loadChanges = async () => {
    try {
      const result = await invoke<{ ok: Change[] }>('change_list', {
        projectPath: project.path,
      });
      if (result.ok) {
        setChanges(result.ok);
      }
    } catch (error) {
      console.error('Failed to load changes:', error);
      // Mock data for development
      setChanges([
        {
          id: '1',
          changeType: 'added',
          path: 'POST /customers',
          description: 'New endpoint for customer creation',
          breaking: false,
        },
        {
          id: '2',
          changeType: 'modified',
          path: 'PATCH /work-orders/{id}',
          description: 'Added new field: priority',
          breaking: false,
        },
        {
          id: '3',
          changeType: 'removed',
          path: 'Response field: customerName',
          description: 'Removed deprecated field',
          breaking: true,
        },
      ]);
    }
  };

  const handleAccept = async (changeId: string) => {
    try {
      await invoke('change_accept', { projectPath: project.path, changeId });
      setChanges(changes.filter((c) => c.id !== changeId));
    } catch {
      setChanges(changes.filter((c) => c.id !== changeId));
    }
  };

  const handleReject = async (changeId: string) => {
    try {
      await invoke('change_reject', { projectPath: project.path, changeId });
      setChanges(changes.filter((c) => c.id !== changeId));
    } catch {
      setChanges(changes.filter((c) => c.id !== changeId));
    }
  };

  const breakingCount = changes.filter((c) => c.breaking).length;

  return (
    <div>
      <h2>Contract Changes</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {changes.length} changes detected
        {breakingCount > 0 && (
          <span style={{ color: '#991b1b', marginLeft: '0.5rem' }}>
            • {breakingCount} potentially breaking
          </span>
        )}
      </p>

      {changes.length === 0 ? (
        <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
          No pending changes. Your contract is up to date.
        </div>
      ) : (
        <div className="change-list">
          {changes.map((change) => (
            <div key={change.id} className="change-item">
              <div className="change-info">
                <span className={`change-type ${change.changeType}`}>
                  {change.changeType === 'added'
                    ? 'Added'
                    : change.changeType === 'modified'
                    ? 'Modified'
                    : 'Removed'}
                </span>
                <div>
                  <div style={{ fontWeight: 500 }}>{change.path}</div>
                  <div style={{ fontSize: '0.875rem', color: '#6c757d' }}>
                    {change.description}
                    {change.breaking && (
                      <span style={{ color: '#991b1b', marginLeft: '0.5rem' }}>
                        ⚠ Breaking
                      </span>
                    )}
                  </div>
                </div>
              </div>
              <div className="change-actions">
                <button
                  className="control-button"
                  onClick={() => handleAccept(change.id)}
                >
                  Accept
                </button>
                <button
                  className="control-button danger"
                  onClick={() => handleReject(change.id)}
                >
                  Reject
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export default Changes;
