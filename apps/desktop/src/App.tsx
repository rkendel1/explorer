import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from './lib/errors';
import ProjectPicker from './components/ProjectPicker';
import Sidebar from './components/Sidebar';
import Explorer from './components/Explorer';
import Requests from './components/Requests';
import Workflows from './components/Workflows';
import Vault from './components/Vault';
import Runtime from './components/Runtime';
import Tests from './components/Tests';
import Changes from './components/Changes';
import Settings from './components/Settings';

export interface Project {
  name: string;
  path: string;
  endpointCount?: number;
  schemaCount?: number;
  environmentCount?: number;
}

export interface Endpoint {
  id: string;
  method: string;
  path: string;
  description?: string;
}

export interface WorkflowStep {
  id: string;
  name: string;
  description: string;
  completed: boolean;
  current: boolean;
}

export interface VaultEntry {
  name: string;
  entryType: string;
  status: string;
}

export interface Environment {
  id: string;
  name: string;
}

export interface RuntimeStatus {
  running: boolean;
  address: string;
  requests: number;
  validationFailures: number;
}

type NavigationItem =
  | 'projects'
  | 'explorer'
  | 'requests'
  | 'workflows'
  | 'tests'
  | 'runtime'
  | 'vault'
  | 'changes'
  | 'settings';

interface ProjectSummary {
  name: string;
  path: string;
  endpoint_count: number;
  schema_count: number;
  environment_count: number;
  has_contract: boolean;
}

interface RecentProject {
  path: string;
  name: string;
  last_opened: string;
}

function App() {
  const [currentProject, setCurrentProject] = useState<Project | null>(null);
  const [activeNav, setActiveNav] = useState<NavigationItem>('projects');
  const [openError, setOpenError] = useState<string | null>(null);
  const [isOpening, setIsOpening] = useState(false);
  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([]);

  const loadRecentProjects = async () => {
    try {
      const projects = await invoke<RecentProject[]>('project_list');
      setRecentProjects(projects);
    } catch {
      // Keep UI usable even if recent list fails.
      setRecentProjects([]);
    }
  };

  useEffect(() => {
    void loadRecentProjects();
  }, []);

  const handleOpenProject = async (path: string, options?: { keepCurrentView?: boolean }) => {
    setIsOpening(true);
    setOpenError(null);
    try {
      const result = await invoke<ProjectSummary>('project_open', {
        request: { path },
      });
      setCurrentProject({
        name: result.name,
        path: result.path,
        endpointCount: result.endpoint_count,
        schemaCount: result.schema_count,
        environmentCount: result.environment_count,
      });
      await loadRecentProjects();

      if (!options?.keepCurrentView) {
        setActiveNav('workflows');
      } else if (activeNav === 'projects') {
        setActiveNav('workflows');
      }
    } catch (error) {
      setOpenError(errorMessage(error));
    } finally {
      setIsOpening(false);
    }
  };

  const handleCloseProject = async () => {
    try {
      await invoke('project_close');
    } catch (error) {
      console.error('Failed to close project cleanly:', errorMessage(error));
    }
    setCurrentProject(null);
    setActiveNav('projects');
    await loadRecentProjects();
  };

  const handleContextSwitch = async (path: string) => {
    if (!path || currentProject?.path === path) {
      return;
    }
    await handleOpenProject(path, { keepCurrentView: true });
  };

  const renderContent = () => {
    if (activeNav === 'projects') {
      return (
        <ProjectPicker
          onOpenProject={handleOpenProject}
          isOpening={isOpening}
          error={openError}
        />
      );
    }

    if (!currentProject) {
      return (
        <ProjectPicker
          onOpenProject={handleOpenProject}
          isOpening={isOpening}
          error={openError}
        />
      );
    }

    switch (activeNav) {
      case 'explorer':
        return <Explorer project={currentProject} />;
      case 'requests':
        return <Requests project={currentProject} />;
      case 'workflows':
        return <Workflows project={currentProject} onNavigate={setActiveNav} />;
      case 'tests':
        return <Tests project={currentProject} />;
      case 'runtime':
        return <Runtime project={currentProject} />;
      case 'vault':
        return <Vault project={currentProject} />;
      case 'changes':
        return <Changes project={currentProject} />;
      case 'settings':
        return <Settings project={currentProject} onCloseProject={handleCloseProject} />;
      default:
        return <Explorer project={currentProject} />;
    }
  };

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>Repo API</h1>
        <div className="header-context-controls">
          <button
            className="control-button"
            onClick={() => setActiveNav('projects')}
            disabled={activeNav === 'projects'}
          >
            Projects
          </button>

          <select
            className="method-select"
            value={currentProject?.path ?? ''}
            onChange={(event) => void handleContextSwitch(event.target.value)}
            disabled={isOpening || recentProjects.length === 0}
            style={{ minWidth: '320px' }}
          >
            {!currentProject && <option value="">Select project context...</option>}
            {recentProjects.map((project) => (
              <option key={project.path} value={project.path}>
                {project.name} - {project.path}
              </option>
            ))}
          </select>

          {currentProject && (
            <div className="status-badge">
              <span className="status-dot running"></span>
              Context Active
            </div>
          )}
        </div>
      </header>

      {currentProject && (
        <div className="project-context-strip">
          <div className="project-context-title">Project Context: {currentProject.name}</div>
          <div className="project-context-path">{currentProject.path}</div>
          <div className="project-context-metrics">
            <span>{currentProject.endpointCount ?? 0} endpoints</span>
            <span>{currentProject.schemaCount ?? 0} schemas</span>
            <span>{currentProject.environmentCount ?? 0} environments</span>
          </div>
        </div>
      )}

      <div className="app-body">
        {currentProject && (
          <Sidebar
            activeNav={activeNav}
            onNavigate={setActiveNav}
            project={currentProject}
          />
        )}
        <main className="main-content">{renderContent()}</main>
      </div>
    </div>
  );
}

export default App;
