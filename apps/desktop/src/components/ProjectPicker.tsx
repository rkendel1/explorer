import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';

interface ProjectPickerProps {
  onOpenProject: (path: string) => void;
  isOpening: boolean;
  error: string | null;
}

interface RecentProject {
  path: string;
  name: string;
  last_opened: string;
}

function ProjectPicker({ onOpenProject, isOpening, error }: ProjectPickerProps) {
  const [repositoryPath, setRepositoryPath] = useState('');
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);
  const [showConnectPanel, setShowConnectPanel] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const loadRecentProjects = async () => {
    try {
      const projects = await invoke<RecentProject[]>('project_list');
      setRecentProjects(projects);
    } catch (err) {
      setLocalError(errorMessage(err));
    }
  };

  useEffect(() => {
    void loadRecentProjects();
  }, []);

  const handleOpenRepository = () => {
    const path = repositoryPath.trim();
    if (!path) {
      setLocalError('Enter a repository path to continue.');
      return;
    }

    setLocalError(null);
    onOpenProject(path);
  };

  const handleRemoveRecent = async (path: string) => {
    setLocalError(null);
    try {
      await invoke('project_remove_recent', { path });
      await loadRecentProjects();
    } catch (err) {
      setLocalError(errorMessage(err));
    }
  };

  const formatLastOpened = (value: string) => {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) {
      return value;
    }
    return date.toLocaleString();
  };

  return (
    <div className="project-picker">
      <h2>Projects Dashboard</h2>
      <p className="subtitle">
        Open any repository and continue where you left off.
      </p>

      {error && (
        <div className="error-banner">
          <span>Couldn't open that repository: {error}</span>
        </div>
      )}

      {localError && !error && (
        <div className="error-banner">
          <span>{localError}</span>
        </div>
      )}

      <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'center', marginBottom: '1rem' }}>
        <button className="action-button" onClick={() => setShowConnectPanel((v) => !v)}>
          {showConnectPanel ? 'Hide Add Project' : 'Add / Connect Project'}
        </button>
      </div>

      {showConnectPanel && (
        <div className="path-form" aria-label="Repository path form" style={{ marginBottom: '1.25rem' }}>
          <input
            className="path-input"
            placeholder="/absolute/path/to/repository or https://github.com/owner/repo"
            value={repositoryPath}
            onChange={(event) => setRepositoryPath(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                handleOpenRepository();
              }
            }}
            disabled={isOpening}
          />

          <button
            className="action-button"
            onClick={handleOpenRepository}
            disabled={isOpening}
          >
            {isOpening ? 'Opening...' : 'Open Project'}
          </button>
          <p className="path-hint">
            Supports local folders and GitHub URLs.
          </p>
        </div>
      )}

      <div className="recent-projects">
        <h3>My Projects</h3>
        {recentProjects.length === 0 ? (
          <div className="onboarding-preview" role="region" aria-label="No projects">
            <p>No projects yet. Use Add / Connect Project to get started.</p>
          </div>
        ) : (
          <div className="project-list">
            {recentProjects.map((project) => (
              <div key={project.path} className="project-item" style={{ cursor: 'default' }}>
                <button
                  className="project-item-open"
                  onClick={() => onOpenProject(project.path)}
                  disabled={isOpening}
                >
                  <span className="name">{project.name}</span>
                  <span className="path">{project.path}</span>
                  <span className="path" style={{ fontSize: '0.8rem' }}>
                    Last opened: {formatLastOpened(project.last_opened)}
                  </span>
                </button>
                <button
                  className="control-button"
                  onClick={() => void handleRemoveRecent(project.path)}
                  disabled={isOpening}
                  style={{ alignSelf: 'flex-start' }}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="onboarding-preview" role="region" aria-label="Onboarding steps" style={{ marginTop: '1.25rem' }}>
        <h3>Quick Start (Optional)</h3>
        <p>1. Open project</p>
        <p>2. Review API details</p>
        <p>3. Send first request</p>
        <p>4. Start mock API</p>
        <p>5. Run first test</p>
      </div>
    </div>
  );
}

export default ProjectPicker;
