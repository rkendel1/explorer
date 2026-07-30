interface ProjectPickerProps {
  onOpenProject: (path: string) => void;
  isOpening: boolean;
  error: string | null;
}

function ProjectPicker({ onOpenProject, isOpening, error }: ProjectPickerProps) {
  const handleOpenRepository = () => {
    // In a real implementation, this would open a native file dialog.
    const path = window.prompt('Enter repository path:', '/path/to/repository');
    if (path) {
      onOpenProject(path);
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

      {error && (
        <div className="error-banner">
          <span>Couldn't open that repository: {error}</span>
        </div>
      )}

      <button
        className="action-button"
        onClick={handleOpenRepository}
        disabled={isOpening}
      >
        {isOpening ? 'Opening...' : 'Connect Repository'}
      </button>
    </div>
  );
}

export default ProjectPicker;
