import { useState } from 'react';
import type { Project } from '../App';

interface ProjectPickerProps {
  onOpenProject: (path: string) => void;
  onOpenDemoProject: () => void;
}

const recentProjects: Project[] = [
  {
    name: 'FieldFlow API',
    path: '/customer/fieldflow',
  },
  {
    name: 'Customer Platform',
    path: '/customer/platform',
  },
];

function ProjectPicker({ onOpenProject, onOpenDemoProject }: ProjectPickerProps) {
  const [isOpening, setIsOpening] = useState(false);

  const handleOpenRepository = async () => {
    setIsOpening(true);
    try {
      // In a real implementation, this would open a native file dialog
      // For now, we'll simulate with a prompt or default path
      const path = window.prompt('Enter repository path:', '/path/to/repository');
      if (path) {
        onOpenProject(path);
      }
    } finally {
      setIsOpening(false);
    }
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

      <button
        className="action-button"
        onClick={handleOpenRepository}
        disabled={isOpening}
      >
        {isOpening ? 'Opening...' : 'Connect Repository'}
      </button>
      <button className="control-button" onClick={onOpenDemoProject} style={{ marginLeft: '0.75rem' }}>
        Explore Demo Project
      </button>

      <div className="recent-projects">
        <h3>Recent Projects</h3>
        <div className="project-list">
          {recentProjects.map((project) => (
            <div
              key={project.path}
              className="project-item"
              onClick={() => onOpenProject(project.path)}
            >
              <span className="name">{project.name}</span>
              <span className="path">{project.path}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default ProjectPicker;
