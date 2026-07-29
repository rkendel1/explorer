import type { Project } from '../App';

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

interface SidebarProps {
  activeNav: NavigationItem;
  onNavigate: (item: NavigationItem) => void;
  project: Project;
}

const navItems: { id: NavigationItem; label: string }[] = [
  { id: 'workflows', label: 'Getting Started' },
  { id: 'explorer', label: 'Your API' },
  { id: 'requests', label: 'Requests' },
  { id: 'runtime', label: 'Mock API' },
  { id: 'tests', label: 'Tests' },
  { id: 'vault', label: 'Vault (More)' },
  { id: 'changes', label: 'Contract Changes (More)' },
  { id: 'settings', label: 'Settings (More)' },
];

function Sidebar({ activeNav, onNavigate, project }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div
        className="nav-item"
        style={{ fontWeight: 600, marginBottom: '0.5rem' }}
      >
        {project.name}
      </div>
      {navItems.map((item) => (
        <div
          key={item.id}
          className={`nav-item ${activeNav === item.id ? 'active' : ''}`}
          onClick={() => onNavigate(item.id)}
        >
          {item.label}
        </div>
      ))}
    </aside>
  );
}

export default Sidebar;
