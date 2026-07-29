import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project, RuntimeStatus } from '../App';

interface RuntimeProps {
  project: Project;
}

function Runtime({ project }: RuntimeProps) {
  const [status, setStatus] = useState<RuntimeStatus>({
    running: false,
    address: 'http://localhost:4010',
    requests: 0,
    validationFailures: 0,
  });
  const [events, setEvents] = useState<{ time: string; type: string; message: string }[]>([]);

  useEffect(() => {
    loadRuntimeStatus();
  }, [project.path]);

  const loadRuntimeStatus = async () => {
    try {
      const result = await invoke<{ ok: RuntimeStatus }>('runtime_status', {
        projectPath: project.path,
      });
      if (result.ok) {
        setStatus(result.ok);
      }
    } catch (error) {
      console.error('Failed to load runtime status:', error);
      // Mock data for development
      setStatus({
        running: true,
        address: 'http://localhost:4010',
        requests: 382,
        validationFailures: 4,
      });
      setEvents([
        { time: '12:42:31', type: 'request', message: 'POST /work-orders' },
        { time: '12:42:31', type: 'match', message: 'Scenario matched: Create Work Order' },
        { time: '12:42:31', type: 'response', message: '201 Created' },
        { time: '12:41:15', type: 'request', message: 'GET /work-orders' },
        { time: '12:41:15', type: 'response', message: '200 OK' },
      ]);
    }
  };

  const handleStart = async () => {
    try {
      await invoke('runtime_start', { projectPath: project.path });
      setStatus({ ...status, running: true });
    } catch {
      setStatus({ ...status, running: true });
    }
  };

  const handleStop = async () => {
    try {
      await invoke('runtime_stop', { projectPath: project.path });
      setStatus({ ...status, running: false });
    } catch {
      setStatus({ ...status, running: false });
    }
  };

  const handleRestart = async () => {
    await handleStop();
    setTimeout(() => handleStart(), 500);
  };

  const handleReset = async () => {
    try {
      await invoke('runtime_reset', { projectPath: project.path });
      setStatus({ ...status, requests: 0, validationFailures: 0 });
      setEvents([]);
    } catch {
      setStatus({ ...status, requests: 0, validationFailures: 0 });
      setEvents([]);
    }
  };

  return (
    <div>
      <h2>Mock Runtime</h2>

      <div className="runtime-status">
        <div className="runtime-card">
          <div className="runtime-info">
            <div className="runtime-stat">
              <span className="runtime-stat-label">Status</span>
              <span className="runtime-stat-value">
                {status.running ? (
                  <span style={{ color: '#22c55e' }}>Running</span>
                ) : (
                  <span style={{ color: '#6c757d' }}>Stopped</span>
                )}
              </span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Address</span>
              <span className="runtime-stat-value" style={{ fontSize: '0.875rem' }}>
                {status.address}
              </span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Requests</span>
              <span className="runtime-stat-value">{status.requests}</span>
            </div>
            <div className="runtime-stat">
              <span className="runtime-stat-label">Validation Failures</span>
              <span className="runtime-stat-value" style={{ color: status.validationFailures > 0 ? '#ef4444' : undefined }}>
                {status.validationFailures}
              </span>
            </div>
          </div>

          <div className="runtime-controls">
            {status.running ? (
              <button className="control-button danger" onClick={handleStop}>
                Stop
              </button>
            ) : (
              <button className="control-button primary" onClick={handleStart}>
                Start
              </button>
            )}
            <button className="control-button" onClick={handleRestart} disabled={!status.running}>
              Restart
            </button>
            <button className="control-button" onClick={handleReset}>
              Reset State
            </button>
          </div>
        </div>

        <div>
          <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>Activity</h3>
          <div style={{ border: '1px solid #e9ecef', borderRadius: '0.5rem', overflow: 'hidden' }}>
            {events.map((event, index) => (
              <div
                key={index}
                style={{
                  padding: '0.75rem 1rem',
                  borderBottom: index < events.length - 1 ? '1px solid #e9ecef' : undefined,
                  display: 'flex',
                  gap: '1rem',
                  fontSize: '0.875rem',
                }}
              >
                <span style={{ color: '#6c757d', fontFamily: 'monospace' }}>{event.time}</span>
                <span>{event.message}</span>
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
