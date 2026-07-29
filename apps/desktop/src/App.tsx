import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
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

function App() {
  const [currentProject, setCurrentProject] = useState<Project | null>(null);
  const [activeNav, setActiveNav] = useState<NavigationItem>('projects');

  const handleOpenProject = async (path: string) => {
    try {
      const result = await invoke<{ ok: Project }>('project_open', { path });
      if (result.ok) {
        setCurrentProject({ ...result.ok, path });
        setActiveNav('workflows');
      }
    } catch (error) {
      console.error('Failed to open project:', error);
      // For demo/development, create a mock project
      setCurrentProject({
        name: path.split('/').pop() || 'Project',
        path,
        endpointCount: 42,
        schemaCount: 18,
        environmentCount: 3,
      });
      setActiveNav('workflows');
    }
  };

  const handleOpenDemoProject = () => {
    setCurrentProject({
      name: 'FieldFlow API',
      path: '/demo/fieldflow',
      endpointCount: 42,
      schemaCount: 18,
      environmentCount: 3,
    });
    setActiveNav('workflows');
  };

  const handleCloseProject = () => {
    setCurrentProject(null);
    setActiveNav('projects');
  };

  const renderContent = () => {
    if (!currentProject) {
      return (
        <ProjectPicker
          onOpenProject={handleOpenProject}
          onOpenDemoProject={handleOpenDemoProject}
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
