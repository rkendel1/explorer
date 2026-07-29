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
  { id: 'explorer', label: 'Explorer' },
  { id: 'requests', label: 'Requests' },
  { id: 'workflows', label: 'Workflows' },
  { id: 'tests', label: 'Tests' },
  { id: 'runtime', label: 'Runtime' },
  { id: 'vault', label: 'Vault' },
  { id: 'changes', label: 'Changes' },
  { id: 'settings', label: 'Settings' },
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
