import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project, Endpoint } from '../App';

interface ExplorerProps {
  project: Project;
}

interface EndpointSummary {
  id: string;
  method: string;
  path: string;
  summary: string | null;
  confidence: number;
}

function Explorer({ project }: ExplorerProps) {
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedEndpoint, setSelectedEndpoint] = useState<Endpoint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isRescanning, setIsRescanning] = useState(false);

  useEffect(() => {
    loadEndpoints();
  }, [project.path]);

  const loadEndpoints = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<EndpointSummary[]>('endpoint_list', { request: null });
      setEndpoints(
        result.map((ep) => ({
          id: ep.id,
          method: ep.method,
          path: ep.path,
          description: ep.summary ?? undefined,
        }))
      );
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleRescan = async () => {
    setIsRescanning(true);
    setError(null);
    try {
      await invoke('contract_rescan');
      await loadEndpoints();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsRescanning(false);
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
        {isLoading ? 'Loading...' : `${endpoints.length} endpoints discovered`}
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '1rem' }}>
        <input
          type="text"
          placeholder="Search endpoints..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="url-input"
          style={{ maxWidth: '400px' }}
        />
        <button className="control-button" onClick={handleRescan} disabled={isRescanning}>
          {isRescanning ? 'Rescanning...' : 'Rescan'}
        </button>
      </div>

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
        {!isLoading && filteredEndpoints.length === 0 && (
          <div style={{ padding: '2rem', textAlign: 'center', color: '#6c757d' }}>
            No endpoints found. Try Rescan if you've changed the repository.
          </div>
        )}
      </div>

      {selectedEndpoint && (
        <div style={{ marginTop: '2rem', padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
          <h3>
            <span className={`method-badge ${selectedEndpoint.method.toLowerCase()}`} style={{ marginRight: '0.5rem' }}>
              {selectedEndpoint.method}
            </span>
            {selectedEndpoint.path}
          </h3>
          {selectedEndpoint.description && (
            <p style={{ color: '#6c757d', marginTop: '0.5rem' }}>{selectedEndpoint.description}</p>
          )}
        </div>
      )}
    </div>
  );
}

export default Explorer;
