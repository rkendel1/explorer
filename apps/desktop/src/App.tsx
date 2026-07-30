import { useState } from 'react';
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

function App() {
  const [currentProject, setCurrentProject] = useState<Project | null>(null);
  const [activeNav, setActiveNav] = useState<NavigationItem>('projects');
  const [openError, setOpenError] = useState<string | null>(null);
  const [isOpening, setIsOpening] = useState(false);

  const handleOpenProject = async (path: string) => {
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
      setActiveNav('workflows');
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
        {currentProject && (
          <div className="status-badge">
            <span className="status-dot running"></span>
            Running
          </div>
        )}
      </header>

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
