import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

interface RuntimeProps {
  project: Project;
}

interface RuntimeStatusInfo {
  status: 'stopped' | 'starting' | 'running' | 'stopping' | 'error';
  address: string | null;
  metrics: {
    total_requests: number;
    validation_failures: number;
  };
}

interface RuntimeEventInfo {
  event_id: string;
  timestamp: string;
  event_type: string;
  method: string | null;
  path: string | null;
  status: number | null;
  details: string | null;
}

function Runtime({ project: _project }: RuntimeProps) {
  const [status, setStatus] = useState<RuntimeStatusInfo>({
    status: 'stopped',
    address: null,
    metrics: { total_requests: 0, validation_failures: 0 },
  });
  const [events, setEvents] = useState<RuntimeEventInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);

  useEffect(() => {
    loadRuntimeStatus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [_project.path]);

  const loadRuntimeStatus = async () => {
    try {
      const result = await invoke<RuntimeStatusInfo>('runtime_status');
      setStatus(result);
      if (result.status === 'running') {
        loadEvents();
      }
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const loadEvents = async () => {
    try {
      const result = await invoke<RuntimeEventInfo[]>('runtime_events', {
        request: { filter: null, limit: 50 },
      });
      setEvents(result);
    } catch (err) {
      console.error('Failed to load runtime events:', errorMessage(err));
    }
  };

  const handleStart = async () => {
    setIsBusy(true);
    setError(null);
    try {
      const result = await invoke<RuntimeStatusInfo>('runtime_start', {
        request: { port: 4010, profileId: null },
      });
      setStatus(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handleStop = async () => {
    setIsBusy(true);
    setError(null);
    try {
      const result = await invoke<RuntimeStatusInfo>('runtime_stop');
      setStatus(result);
      setEvents([]);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handleRestart = async () => {
    setIsBusy(true);
    setError(null);
    try {
      const result = await invoke<RuntimeStatusInfo>('runtime_restart', {
        request: { port: 4010, profileId: null },
      });
      setStatus(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const handleReset = async () => {
    setIsBusy(true);
    setError(null);
    try {
      const result = await invoke<RuntimeStatusInfo>('runtime_reset');
      setStatus(result);
      setEvents([]);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const running = status.status === 'running';

  return (
    <div>
      <h2>Mock Runtime</h2>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      <div className="runtime-status">
        <div className="runtime-card">
          <div className="runtime-info">
            <div className="runtime-stat">
              <span className="runtime-stat-label">Status</span>
              <span className="runtime-stat-value">
                {running ? (
                  <span style={{ color: '#22c55e' }}>Running</span>
                ) : status.status === 'error' ? (
                  <span style={{ color: '#ef4444' }}>Error</span>
                ) : (
                  <span style={{ color: '#6c757d' }}>{status.status}</span>
                )}
              </span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Address</span>
              <span className="runtime-stat-value" style={{ fontSize: '0.875rem' }}>
                {status.address ?? '—'}
              </span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Requests</span>
              <span className="runtime-stat-value">{status.metrics.total_requests}</span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Validation Failures</span>
              <span
                className="runtime-stat-value"
                style={{ color: status.metrics.validation_failures > 0 ? '#ef4444' : undefined }}
              >
                {status.metrics.validation_failures}
              </span>
            </div>
          </div>

          <div className="runtime-controls">
            {running ? (
              <button className="control-button danger" onClick={handleStop} disabled={isBusy}>
                Stop
              </button>
            ) : (
              <button className="control-button primary" onClick={handleStart} disabled={isBusy}>
                Start
              </button>
            )}
            <button className="control-button" onClick={handleRestart} disabled={!running || isBusy}>
              Restart
            </button>
            <button className="control-button" onClick={handleReset} disabled={isBusy}>
              Reset State
            </button>
          </div>
        </div>

        <div>
          <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>Activity</h3>
          <div style={{ border: '1px solid #e9ecef', borderRadius: '0.5rem', overflow: 'hidden' }}>
            {events.map((event) => (
              <div
                key={event.event_id}
                style={{
                  padding: '0.75rem 1rem',
                  borderBottom: '1px solid #e9ecef',
                  display: 'flex',
                  gap: '1rem',
                  fontSize: '0.875rem',
                }}
              >
                <span style={{ color: '#6c757d', fontFamily: 'monospace' }}>
                  {new Date(event.timestamp).toLocaleTimeString()}
                </span>
                <span>
                  {event.method && event.path
                    ? `${event.method} ${event.path}`
                    : event.status
                    ? `${event.status}`
                    : event.details ?? event.event_type}
                </span>
              </div>
            ))}
            {events.length === 0 && (
              <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
                No activity yet
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default Runtime;
