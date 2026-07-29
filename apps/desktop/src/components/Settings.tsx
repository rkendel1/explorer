import type { Project } from '../App';

interface SettingsProps {
  project: Project;
  onCloseProject: () => void;
}

function Settings({ project, onCloseProject }: SettingsProps) {
  return (
    <div>
      <h2>Settings</h2>

      <div style={{ marginBottom: '2rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>Project</h3>
        <div style={{ padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <div style={{ marginBottom: '0.5rem' }}>
            <strong>Name:</strong> {project.name}
          </div>
          <div style={{ marginBottom: '0.5rem' }}>
            <strong>Path:</strong> {project.path}
          </div>
          <div style={{ marginBottom: '1rem' }}>
            <strong>Endpoints:</strong> {project.endpointCount ?? 'Unknown'}
          </div>
          <button className="control-button danger" onClick={onCloseProject}>
            Close Project
          </button>
        </div>
      </div>

      <div style={{ marginBottom: '2rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>Vault</h3>
        <div style={{ padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <div style={{ marginBottom: '0.5rem' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
              <strong>Auto-lock timeout:</strong>
              <select className="method-select" defaultValue="15">
                <option value="5">5 minutes</option>
                <option value="15">15 minutes</option>
                <option value="30">30 minutes</option>
                <option value="60">1 hour</option>
                <option value="0">Never</option>
              </select>
            </label>
          </div>
          <p style={{ fontSize: '0.875rem', color: '#6c757d', marginTop: '0.5rem' }}>
            The vault will automatically lock after this period of inactivity.
          </p>
        </div>
      </div>

      <div style={{ marginBottom: '2rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>Production Safety</h3>
        <div style={{ padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <input type="checkbox" defaultChecked />
            <span>Require confirmation for production requests</span>
          </label>
          <p style={{ fontSize: '0.875rem', color: '#6c757d', marginTop: '0.5rem' }}>
            When enabled, POST/PUT/PATCH/DELETE requests to production environments require explicit confirmation.
          </p>
        </div>
      </div>

      <div>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.75rem' }}>About</h3>
        <div style={{ padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <div><strong>Repo API Desktop</strong></div>
          <div style={{ fontSize: '0.875rem', color: '#6c757d' }}>Version 0.1.0</div>
          <p style={{ fontSize: '0.875rem', color: '#6c757d', marginTop: '0.5rem' }}>
            A local-first API development environment with repository-aware API discovery.
          </p>
        </div>
      </div>
    </div>
  );
}

export default Settings;
