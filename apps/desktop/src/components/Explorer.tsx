import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

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

interface ParameterInfo {
  name: string;
  location: string;
  required: boolean;
  schema_type: string;
  schema_ref: string | null;
}

interface RequestBodyInfo {
  content_type: string;
  required: boolean;
  schema_ref: string | null;
  example: unknown;
}

interface ResponseInfo {
  status: number;
  content_type: string | null;
  schema_ref: string | null;
  example: unknown;
}

interface EvidenceInfo {
  file: string;
  line_start: number | null;
  line_end: number | null;
}

interface EndpointDetail {
  id: string;
  method: string;
  path: string;
  summary: string | null;
  description: string | null;
  parameters: ParameterInfo[];
  request_body: RequestBodyInfo | null;
  responses: ResponseInfo[];
  security: string[];
  confidence: number;
  evidence: EvidenceInfo[];
}

interface SchemaSummary {
  name: string;
  schema_type: string;
  properties: string[];
}

interface SchemaProperty {
  name: string;
  property_type: string;
  description: string | null;
  required: boolean;
  format: string | null;
}

interface SchemaDetail {
  name: string;
  schema_type: string;
  description: string | null;
  properties: SchemaProperty[];
  required: string[];
  example: unknown;
}

const schemaNameFromRef = (schemaRef: string | null): string | null => {
  if (!schemaRef) {
    return null;
  }
  const parts = schemaRef.split('/');
  return parts.length > 0 ? parts[parts.length - 1] : null;
};

const pretty = (value: unknown): string => JSON.stringify(value, null, 2);

function Explorer({ project }: ExplorerProps) {
  const [endpoints, setEndpoints] = useState<EndpointSummary[]>([]);
  const [schemas, setSchemas] = useState<SchemaSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedEndpointId, setSelectedEndpointId] = useState<string | null>(null);
  const [selectedEndpoint, setSelectedEndpoint] = useState<EndpointDetail | null>(null);
  const [selectedSchema, setSelectedSchema] = useState<SchemaDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingEndpoint, setIsLoadingEndpoint] = useState(false);
  const [isLoadingSchema, setIsLoadingSchema] = useState(false);
  const [isRescanning, setIsRescanning] = useState(false);

  useEffect(() => {
    void loadExplorerData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project.path]);

  useEffect(() => {
    if (selectedEndpointId) {
      void loadEndpointDetail(selectedEndpointId);
    }
  }, [selectedEndpointId]);

  const loadExplorerData = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [endpointResult, schemaResult] = await Promise.all([
        invoke<EndpointSummary[]>('endpoint_list', { request: null }),
        invoke<SchemaSummary[]>('schema_list'),
      ]);
      setEndpoints(endpointResult);
      setSchemas(schemaResult);
      if (!selectedEndpointId && endpointResult.length > 0) {
        setSelectedEndpointId(endpointResult[0].id);
      }
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoading(false);
    }
  };

  const loadEndpointDetail = async (id: string) => {
    setIsLoadingEndpoint(true);
    setError(null);
    try {
      const detail = await invoke<EndpointDetail>('endpoint_get', { request: { id } });
      setSelectedEndpoint(detail);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoadingEndpoint(false);
    }
  };

  const loadSchemaDetail = async (name: string) => {
    setIsLoadingSchema(true);
    setError(null);
    try {
      const detail = await invoke<SchemaDetail>('schema_get', { request: { name } });
      setSelectedSchema(detail);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoadingSchema(false);
    }
  };

  const handleRescan = async () => {
    setIsRescanning(true);
    setError(null);
    try {
      await invoke('contract_rescan');
      setSelectedEndpoint(null);
      setSelectedSchema(null);
      await loadExplorerData();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsRescanning(false);
    }
  };

  const filteredEndpoints = endpoints.filter(
    (endpoint) =>
      endpoint.path.toLowerCase().includes(searchQuery.toLowerCase()) ||
      endpoint.method.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (endpoint.summary ?? '').toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <div>
      <h2>API Explorer</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {isLoading
          ? 'Loading...'
          : `${endpoints.length} endpoints discovered • ${schemas.length} schemas`}
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
            onClick={() => setSelectedEndpointId(endpoint.id)}
            style={{
              borderColor: selectedEndpointId === endpoint.id ? '#93c5fd' : undefined,
              background: selectedEndpointId === endpoint.id ? '#eff6ff' : undefined,
            }}
          >
            <span className={`method-badge ${endpoint.method.toLowerCase()}`}>
              {endpoint.method}
            </span>
            <div>
              <div>{endpoint.path}</div>
              {endpoint.summary && (
                <div style={{ fontSize: '0.8rem', color: '#6c757d' }}>{endpoint.summary}</div>
              )}
            </div>
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
          {(selectedEndpoint.summary || selectedEndpoint.description) && (
            <p style={{ color: '#6c757d', marginTop: '0.5rem' }}>
              {selectedEndpoint.summary ?? selectedEndpoint.description}
            </p>
          )}

          <p style={{ color: '#6c757d', marginTop: '0.5rem' }}>
            Confidence: {(selectedEndpoint.confidence * 100).toFixed(0)}%
          </p>

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Parameters</h4>
          {selectedEndpoint.parameters.length === 0 ? (
            <p style={{ color: '#6c757d' }}>No parameters</p>
          ) : (
            <div>
              {selectedEndpoint.parameters.map((param) => (
                <div key={`${param.location}:${param.name}`} style={{ marginBottom: '0.5rem' }}>
                  <strong>{param.name}</strong> ({param.location}) • {param.required ? 'required' : 'optional'}
                  {param.schema_ref && (
                    <>
                      {' '}
                      •
                      <button
                        className="control-button"
                        style={{ marginLeft: '0.5rem' }}
                        onClick={() => {
                          const name = schemaNameFromRef(param.schema_ref);
                          if (name) {
                            void loadSchemaDetail(name);
                          }
                        }}
                      >
                        View Schema
                      </button>
                    </>
                  )}
                </div>
              ))}
            </div>
          )}

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Request Body</h4>
          {!selectedEndpoint.request_body ? (
            <p style={{ color: '#6c757d' }}>No request body</p>
          ) : (
            <div>
              <p>
                <strong>Content Type:</strong> {selectedEndpoint.request_body.content_type} •{' '}
                {selectedEndpoint.request_body.required ? 'required' : 'optional'}
              </p>
              {selectedEndpoint.request_body.schema_ref && (
                <p>
                  <button
                    className="control-button"
                    onClick={() => {
                      const name = schemaNameFromRef(selectedEndpoint.request_body?.schema_ref ?? null);
                      if (name) {
                        void loadSchemaDetail(name);
                      }
                    }}
                  >
                    View Request Schema
                  </button>
                </p>
              )}
              {selectedEndpoint.request_body.example != null && (
                <pre style={{ background: '#f8fafc', border: '1px solid #e2e8f0', padding: '0.75rem', borderRadius: '0.5rem', overflowX: 'auto', fontSize: '0.8rem' }}>
                  {pretty(selectedEndpoint.request_body.example)}
                </pre>
              )}
            </div>
          )}

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Responses</h4>
          {selectedEndpoint.responses.length === 0 ? (
            <p style={{ color: '#6c757d' }}>No response definitions</p>
          ) : (
            <div>
              {selectedEndpoint.responses.map((response, idx) => (
                <div key={`${response.status}-${idx}`} style={{ borderTop: idx === 0 ? 'none' : '1px solid #e9ecef', paddingTop: idx === 0 ? 0 : '0.75rem', marginTop: idx === 0 ? 0 : '0.75rem' }}>
                  <p>
                    <strong>{response.status}</strong>
                    {response.content_type ? ` • ${response.content_type}` : ''}
                    {response.schema_ref && (
                      <button
                        className="control-button"
                        style={{ marginLeft: '0.5rem' }}
                        onClick={() => {
                          const name = schemaNameFromRef(response.schema_ref);
                          if (name) {
                            void loadSchemaDetail(name);
                          }
                        }}
                      >
                        View Schema
                      </button>
                    )}
                  </p>
                  {response.example != null && (
                    <pre style={{ background: '#f8fafc', border: '1px solid #e2e8f0', padding: '0.75rem', borderRadius: '0.5rem', overflowX: 'auto', fontSize: '0.8rem' }}>
                      {pretty(response.example)}
                    </pre>
                  )}
                </div>
              ))}
            </div>
          )}

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Security</h4>
          {selectedEndpoint.security.length === 0 ? (
            <p style={{ color: '#6c757d' }}>No authentication requirements detected</p>
          ) : (
            <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
              {selectedEndpoint.security.map((scheme) => (
                <span key={scheme} className="change-type modified">
                  {scheme}
                </span>
              ))}
            </div>
          )}

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Source Evidence</h4>
          {selectedEndpoint.evidence.length === 0 ? (
            <p style={{ color: '#6c757d' }}>No source evidence attached</p>
          ) : (
            <div>
              {selectedEndpoint.evidence.map((item, idx) => (
                <div key={`${item.file}-${idx}`} style={{ fontFamily: 'monospace', fontSize: '0.82rem' }}>
                  {item.file}
                  {item.line_start ? `:${item.line_start}` : ''}
                  {item.line_end && item.line_end !== item.line_start ? `-${item.line_end}` : ''}
                </div>
              ))}
            </div>
          )}

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Schemas</h4>
          <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap', marginBottom: '0.75rem' }}>
            {schemas.map((schema) => (
              <button
                key={schema.name}
                className="control-button"
                onClick={() => {
                  void loadSchemaDetail(schema.name);
                }}
              >
                {schema.name}
              </button>
            ))}
          </div>

          {isLoadingSchema && <p style={{ color: '#6c757d' }}>Loading schema detail...</p>}
          {selectedSchema && (
            <div style={{ border: '1px solid #e9ecef', borderRadius: '0.5rem', padding: '0.75rem' }}>
              <h5 style={{ marginBottom: '0.5rem' }}>{selectedSchema.name}</h5>
              <p style={{ color: '#6c757d', marginBottom: '0.5rem' }}>
                Type: {selectedSchema.schema_type}
              </p>
              {selectedSchema.properties.length === 0 ? (
                <p style={{ color: '#6c757d' }}>No properties</p>
              ) : (
                <div>
                  {selectedSchema.properties.map((property) => (
                    <div key={property.name} style={{ marginBottom: '0.4rem' }}>
                      <strong>{property.name}</strong> • {property.property_type}
                      {property.required ? ' • required' : ''}
                    </div>
                  ))}
                </div>
              )}
              {selectedSchema.example != null && (
                <pre style={{ marginTop: '0.75rem', background: '#f8fafc', border: '1px solid #e2e8f0', padding: '0.75rem', borderRadius: '0.5rem', overflowX: 'auto', fontSize: '0.8rem' }}>
                  {pretty(selectedSchema.example)}
                </pre>
              )}
            </div>
          )}
        </div>
      )}

      {isLoadingEndpoint && <p style={{ color: '#6c757d', marginTop: '1rem' }}>Loading endpoint details...</p>}
    </div>
  );
}

export default Explorer;
