import { useEffect, useMemo, useState } from 'react';
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
  semantic_intent: EndpointSemanticIntent | null;
}

interface EndpointSemanticIntent {
  primary_intent: string;
  rationale: string;
  confidence: number;
  capabilities: string[];
  domain_concepts: string[];
  data_models: string[];
  behavioral_fingerprint: BehavioralFingerprint;
}

interface BehavioralFingerprint {
  dominant: string;
  scores: BehaviorScore[];
}

interface BehaviorScore {
  behavior: string;
  score: number;
}

interface MeaningGraphNode {
  id: string;
  label: string;
  node_type: string;
}

interface MeaningGraphEdge {
  source: string;
  target: string;
  relation: string;
  weight: number;
}

interface MeaningGraphEndpointIntent {
  endpoint_id: string;
  method: string;
  path: string;
  primary_intent: string;
  confidence: number;
  capabilities: string[];
  domain_concepts: string[];
  data_models: string[];
  behavioral_fingerprint: BehavioralFingerprint;
}

interface ApiMeaningGraph {
  cache_key: string;
  generated_at: string;
  nodes: MeaningGraphNode[];
  edges: MeaningGraphEdge[];
  endpoint_intents: MeaningGraphEndpointIntent[];
  behavioral_fingerprint: BehavioralFingerprint;
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
  const [meaningGraph, setMeaningGraph] = useState<ApiMeaningGraph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingEndpoint, setIsLoadingEndpoint] = useState(false);
  const [isLoadingSchema, setIsLoadingSchema] = useState(false);
  const [isRescanning, setIsRescanning] = useState(false);
  const [isLoadingMeaningGraph, setIsLoadingMeaningGraph] = useState(false);

  const graphLayout = useMemo(() => {
    if (!meaningGraph || meaningGraph.nodes.length === 0) {
      return { positions: {} as Record<string, { x: number; y: number }>, width: 920, height: 420 };
    }

    const width = 920;
    const height = 420;
    const centerX = width / 2;
    const centerY = height / 2;
    const radius = Math.min(width, height) * 0.35;
    const positions: Record<string, { x: number; y: number }> = {};

    meaningGraph.nodes.forEach((node, idx) => {
      const angle = (idx / meaningGraph.nodes.length) * Math.PI * 2;
      positions[node.id] = {
        x: centerX + Math.cos(angle) * radius,
        y: centerY + Math.sin(angle) * radius,
      };
    });

    return { positions, width, height };
  }, [meaningGraph]);

  const behaviorBadgeColor = (behavior: string): string => {
    switch (behavior) {
      case 'crud_ish':
        return '#065f46';
      case 'workflow_driven':
        return '#1d4ed8';
      case 'event_sourced':
        return '#7c2d12';
      case 'rpc_style':
        return '#4c1d95';
      default:
        return '#334155';
    }
  };

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
      await loadMeaningGraph();
      if (!selectedEndpointId && endpointResult.length > 0) {
        setSelectedEndpointId(endpointResult[0].id);
      }
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoading(false);
    }
  };

  const loadMeaningGraph = async () => {
    setIsLoadingMeaningGraph(true);
    try {
      const result = await invoke<ApiMeaningGraph>('contract_meaning_graph');
      setMeaningGraph(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsLoadingMeaningGraph(false);
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

          <h4 style={{ marginTop: '1rem', marginBottom: '0.5rem' }}>Semantic Intent</h4>
          {selectedEndpoint.semantic_intent ? (
            <div style={{ background: '#f8fafc', border: '1px solid #e2e8f0', borderRadius: '0.5rem', padding: '0.75rem' }}>
              <p style={{ marginBottom: '0.4rem' }}>
                <strong>Primary intent:</strong> {selectedEndpoint.semantic_intent.primary_intent}
              </p>
              <p style={{ color: '#6c757d', marginBottom: '0.4rem' }}>
                {selectedEndpoint.semantic_intent.rationale}
              </p>
              <p style={{ color: '#6c757d', marginBottom: '0.4rem' }}>
                Semantic confidence: {(selectedEndpoint.semantic_intent.confidence * 100).toFixed(0)}%
              </p>

              <div style={{ marginBottom: '0.35rem' }}>
                <strong>Capabilities:</strong>
                <div style={{ display: 'flex', gap: '0.4rem', flexWrap: 'wrap', marginTop: '0.25rem' }}>
                  {selectedEndpoint.semantic_intent.capabilities.map((capability) => (
                    <span key={capability} className="change-type added">{capability}</span>
                  ))}
                </div>
              </div>

              <div style={{ marginBottom: '0.35rem' }}>
                <strong>Domain concepts:</strong>
                <div style={{ display: 'flex', gap: '0.4rem', flexWrap: 'wrap', marginTop: '0.25rem' }}>
                  {selectedEndpoint.semantic_intent.domain_concepts.map((concept) => (
                    <span key={concept} className="change-type modified">{concept}</span>
                  ))}
                </div>
              </div>

              <div style={{ marginBottom: '0.35rem' }}>
                <strong>Behavioral fingerprint:</strong>
                <div style={{ display: 'flex', gap: '0.4rem', flexWrap: 'wrap', marginTop: '0.25rem' }}>
                  <span
                    style={{
                      background: '#eef2ff',
                      color: behaviorBadgeColor(selectedEndpoint.semantic_intent.behavioral_fingerprint.dominant),
                      borderRadius: '999px',
                      padding: '0.1rem 0.5rem',
                      fontSize: '0.78rem',
                      fontWeight: 600,
                    }}
                  >
                    {selectedEndpoint.semantic_intent.behavioral_fingerprint.dominant}
                  </span>
                  {selectedEndpoint.semantic_intent.behavioral_fingerprint.scores.map((score) => (
                    <span key={score.behavior} style={{ color: '#475569', fontSize: '0.78rem' }}>
                      {score.behavior}: {(score.score * 100).toFixed(0)}%
                    </span>
                  ))}
                </div>
              </div>

              <div>
                <strong>Data models:</strong>
                <div style={{ display: 'flex', gap: '0.4rem', flexWrap: 'wrap', marginTop: '0.25rem' }}>
                  {selectedEndpoint.semantic_intent.data_models.length === 0 ? (
                    <span style={{ color: '#6c757d' }}>No model links inferred</span>
                  ) : (
                    selectedEndpoint.semantic_intent.data_models.map((model) => (
                      <button key={model} className="control-button" onClick={() => void loadSchemaDetail(model)}>
                        {model}
                      </button>
                    ))
                  )}
                </div>
              </div>
            </div>
          ) : (
            <p style={{ color: '#6c757d' }}>No semantic intent available for this endpoint.</p>
          )}

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

      <div style={{ marginTop: '2rem', padding: '1rem', border: '1px solid #e9ecef', borderRadius: '0.5rem' }}>
        <h3>API Meaning Graph</h3>
        {isLoadingMeaningGraph ? (
          <p style={{ color: '#6c757d' }}>Building semantic meaning graph...</p>
        ) : !meaningGraph ? (
          <p style={{ color: '#6c757d' }}>No meaning graph available.</p>
        ) : (
          <>
            <p style={{ color: '#6c757d', marginBottom: '0.75rem' }}>
              {meaningGraph.nodes.length} nodes • {meaningGraph.edges.length} links • {meaningGraph.endpoint_intents.length} endpoint intents
            </p>
            <p style={{ color: '#6c757d', marginBottom: '0.75rem' }}>
              Graph generated: {new Date(meaningGraph.generated_at).toLocaleString()} • cache key {meaningGraph.cache_key.slice(0, 24)}...
            </p>
            <div style={{ marginBottom: '0.75rem' }}>
              <strong>API behavioral fingerprint:</strong>{' '}
              <span style={{ color: behaviorBadgeColor(meaningGraph.behavioral_fingerprint.dominant), fontWeight: 700 }}>
                {meaningGraph.behavioral_fingerprint.dominant}
              </span>
              <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap', marginTop: '0.35rem' }}>
                {meaningGraph.behavioral_fingerprint.scores.map((score) => (
                  <span key={score.behavior} style={{ color: '#475569', fontSize: '0.82rem' }}>
                    {score.behavior}: {(score.score * 100).toFixed(0)}%
                  </span>
                ))}
              </div>
            </div>

            <div style={{ overflowX: 'auto', marginBottom: '0.9rem' }}>
              <svg
                width={graphLayout.width}
                height={graphLayout.height}
                style={{ border: '1px solid #e5e7eb', borderRadius: '0.5rem', background: 'linear-gradient(180deg,#f8fafc,#ffffff)' }}
              >
                {meaningGraph.edges.map((edge, idx) => {
                  const from = graphLayout.positions[edge.source];
                  const to = graphLayout.positions[edge.target];
                  if (!from || !to) {
                    return null;
                  }
                  return (
                    <g key={`${edge.source}-${edge.target}-${idx}`}>
                      <line
                        x1={from.x}
                        y1={from.y}
                        x2={to.x}
                        y2={to.y}
                        stroke="#94a3b8"
                        strokeWidth={Math.max(1, edge.weight * 2)}
                        opacity={0.7}
                      />
                    </g>
                  );
                })}
                {meaningGraph.nodes.map((node) => {
                  const pos = graphLayout.positions[node.id];
                  if (!pos) {
                    return null;
                  }
                  const fill =
                    node.node_type === 'endpoint'
                      ? '#bfdbfe'
                      : node.node_type === 'capability'
                        ? '#bbf7d0'
                        : node.node_type === 'concept'
                          ? '#fde68a'
                          : '#e9d5ff';
                  return (
                    <g key={node.id}>
                      <circle cx={pos.x} cy={pos.y} r={16} fill={fill} stroke="#64748b" strokeWidth={1.2} />
                      <text x={pos.x} y={pos.y + 28} fontSize="10" textAnchor="middle" fill="#334155">
                        {node.label.length > 24 ? `${node.label.slice(0, 24)}...` : node.label}
                      </text>
                    </g>
                  );
                })}
              </svg>
            </div>

            <div style={{ display: 'grid', gap: '0.5rem' }}>
              {meaningGraph.endpoint_intents.map((intent) => (
                <div key={intent.endpoint_id} style={{ border: '1px solid #e9ecef', borderRadius: '0.5rem', padding: '0.6rem' }}>
                  <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', flexWrap: 'wrap' }}>
                    <span className={`method-badge ${intent.method.toLowerCase()}`}>{intent.method}</span>
                    <strong>{intent.path}</strong>
                    <span style={{ color: '#6c757d' }}>Intent: {intent.primary_intent}</span>
                    <span style={{ color: behaviorBadgeColor(intent.behavioral_fingerprint.dominant) }}>
                      Behavior: {intent.behavioral_fingerprint.dominant}
                    </span>
                  </div>
                  <div style={{ marginTop: '0.35rem', display: 'flex', gap: '0.35rem', flexWrap: 'wrap' }}>
                    {intent.capabilities.map((capability) => (
                      <span key={`${intent.endpoint_id}:cap:${capability}`} className="change-type added">
                        {capability}
                      </span>
                    ))}
                    {intent.domain_concepts.map((concept) => (
                      <span key={`${intent.endpoint_id}:concept:${concept}`} className="change-type modified">
                        {concept}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>

      {isLoadingEndpoint && <p style={{ color: '#6c757d', marginTop: '1rem' }}>Loading endpoint details...</p>}
    </div>
  );
}

export default Explorer;
