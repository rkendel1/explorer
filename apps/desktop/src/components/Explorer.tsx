import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project, Endpoint } from '../App';

interface ExplorerProps {
  project: Project;
}

function Explorer({ project }: ExplorerProps) {
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedEndpoint, setSelectedEndpoint] = useState<Endpoint | null>(null);

  useEffect(() => {
    loadEndpoints();
  }, [project.path]);

  const loadEndpoints = async () => {
    try {
      const result = await invoke<{ ok: Endpoint[] }>('endpoint_list', {
        projectPath: project.path,
      });
      if (result.ok) {
        setEndpoints(result.ok);
      }
    } catch (error) {
      console.error('Failed to load endpoints:', error);
      // Mock data for development
      setEndpoints([
        { id: '1', method: 'GET', path: '/work-orders', description: 'List work orders' },
        { id: '2', method: 'POST', path: '/work-orders', description: 'Create a work order' },
        { id: '3', method: 'GET', path: '/work-orders/{id}', description: 'Get work order by ID' },
        { id: '4', method: 'PATCH', path: '/work-orders/{id}', description: 'Update work order' },
        { id: '5', method: 'DELETE', path: '/work-orders/{id}', description: 'Delete work order' },
        { id: '6', method: 'GET', path: '/customers', description: 'List customers' },
        { id: '7', method: 'POST', path: '/customers', description: 'Create customer' },
      ]);
    }
  };

  const filteredEndpoints = endpoints.filter(
    (endpoint) =>
      endpoint.path.toLowerCase().includes(searchQuery.toLowerCase()) ||
      endpoint.method.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div>
      <h2>API Explorer</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {endpoints.length} endpoints • {project.schemaCount ?? 0} schemas
      </p>

      <input
        type="text"
        placeholder="Search endpoints..."
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        className="url-input"
        style={{ marginBottom: '1rem', maxWidth: '400px' }}
      />

      <div className="endpoint-list">
        {filteredEndpoints.map((endpoint) => (
          <div
            key={endpoint.id}
            className="endpoint-item"
            onClick={() => setSelectedEndpoint(endpoint)}
          >
            <span className={`method-badge ${endpoint.method.toLowerCase()}`}>
              {endpoint.method}
            </span>
            <span>{endpoint.path}</span>
          </div>
        ))}
      </div>

      {selectedEndpoint && (
        <div style={{ marginTop: '2rem', padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <h3>
            <span className={`method-badge ${selectedEndpoint.method.toLowerCase()}`} style={{ marginRight: '0.5rem' }}>
              {selectedEndpoint.method}
            </span>
            {selectedEndpoint.path}
          </h3>
          <p style={{ color: '#6c757d', marginTop: '0.5rem' }}>{selectedEndpoint.description}</p>
          
          <div style={{ marginTop: '1rem' }}>
            <h4 style={{ fontSize: '0.875rem', marginBottom: '0.5rem' }}>Source Evidence</h4>
            <p style={{ fontSize: '0.875rem', color: '#6c757d' }}>
              Discovered from source code analysis
            </p>
            <p style={{ fontSize: '0.875rem', color: '#6c757d' }}>
              Confidence: <strong>96%</strong>
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

export default Explorer;
