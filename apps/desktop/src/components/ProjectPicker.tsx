import { useState } from 'react';
import type { Project } from '../App';

interface ProjectPickerProps {
  onOpenProject: (path: string) => void;
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

function ProjectPicker({ onOpenProject }: ProjectPickerProps) {
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
      <p className="subtitle">Open a repository-aware API workspace.</p>

      <button
        className="action-button"
        onClick={handleOpenRepository}
        disabled={isOpening}
      >
        {isOpening ? 'Opening...' : 'Open Repository'}
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
