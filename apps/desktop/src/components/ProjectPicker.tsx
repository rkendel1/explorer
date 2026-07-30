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
  const [localError, setLocalError] = useState<string | null>(null);

  useEffect(() => {
    const loadRecentProjects = async () => {
      try {
        const projects = await invoke<RecentProject[]>('project_list');
        setRecentProjects(projects);
      } catch (err) {
        setLocalError(errorMessage(err));
      }
    };

    void loadRecentProjects();
  }, []);

  const handleOpenRepository = () => {
    const path = repositoryPath.trim();
    if (!path) {
      setLocalError('Enter a repository path to continue.');
      return;
    }

    if (path.includes('://')) {
      setLocalError('Git URL detected. Clone the repository locally, then enter its local folder path.');
      return;
    }

    setLocalError(null);
    onOpenProject(path);
  };

  return (
    <div className="project-picker">
      <h2>Repo API</h2>
      <p className="subtitle">
        Connect a repository and turn its API into a working, testable environment.
      </p>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        Discover APIs in source code • Run safe requests • Start a mock API • Validate with tests
      </p>

      <div className="onboarding-preview" role="region" aria-label="Onboarding steps">
        <h3>Complete Onboarding</h3>
        <p>1. Connect repository</p>
        <p>2. Review discovered API</p>
        <p>3. Send first request</p>
        <p>4. Start mock API</p>
        <p>5. Run first test</p>
      </div>

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

      <div className="path-form" aria-label="Repository path form">
        <input
          className="path-input"
          placeholder="/absolute/path/to/repository"
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
          {isOpening ? 'Opening...' : 'Connect Repository'}
        </button>
        <p className="path-hint">
          Use a local folder path. GitHub URLs are not opened directly in this step.
        </p>
      </div>

      {recentProjects.length > 0 && (
        <div className="recent-projects">
          <h3>Recent Repositories</h3>
          <div className="project-list">
            {recentProjects.map((project) => (
              <button
                key={project.path}
                className="project-item"
                onClick={() => onOpenProject(project.path)}
                disabled={isOpening}
              >
                <span className="name">{project.name}</span>
                <span className="path">{project.path}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default ProjectPicker;
