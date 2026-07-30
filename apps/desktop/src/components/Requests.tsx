import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

interface RequestsProps {
  project: Project;
}

interface RequestResult {
  status: number;
  duration_ms: number;
  body_size: number;
  headers: [string, string][];
  body: unknown;
  validation: { valid: boolean; issues: { severity: string; message: string; path: string | null }[] };
}

interface EndpointSummary {
  id: string;
  method: string;
  path: string;
  summary: string | null;
}

interface EndpointDetail {
  id: string;
  method: string;
  path: string;
  request_body: { example: unknown } | null;
  parameters: Array<{ name: string; location: string; required: boolean }>;
}

interface SavedRequestInfo {
  name: string;
  method: string;
  url: string | null;
}

interface EnvironmentConfig {
  id: string;
  name: string;
  is_active: boolean;
}

interface EnvironmentVariableInfo {
  key: string;
  value: string;
}

interface VaultEntryMetadata {
  name: string;
  secret_type: string;
}

type BuilderTab = 'presets' | 'params' | 'headers' | 'auth' | 'body' | 'response';
type AuthMode = 'none' | 'bearer' | 'api_key';

interface KeyValueRow {
  id: number;
  key: string;
  value: string;
  enabled: boolean;
}

const createRow = (): KeyValueRow => ({
  id: Date.now() + Math.floor(Math.random() * 100000),
  key: '',
  value: '',
  enabled: true,
});

function Requests({ project: _project }: RequestsProps) {
  const [method, setMethod] = useState('GET');
  const [url, setUrl] = useState('{{baseUrl}}/health');
  const [body, setBody] = useState('');
  const [activeTab, setActiveTab] = useState<BuilderTab>('presets');

  const [params, setParams] = useState<KeyValueRow[]>([createRow()]);
  const [headers, setHeaders] = useState<KeyValueRow[]>([createRow()]);

  const [authMode, setAuthMode] = useState<AuthMode>('none');
  const [authVaultEntry, setAuthVaultEntry] = useState('');
  const [authHeaderName, setAuthHeaderName] = useState('X-API-Key');
  const [authPrefix, setAuthPrefix] = useState('Bearer');

  const [saveName, setSaveName] = useState('');
  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState<string | null>(null);
  const [selectedEndpointId, setSelectedEndpointId] = useState('');
  const [newTokenName, setNewTokenName] = useState('generated-token');
  const [newTokenValue, setNewTokenValue] = useState('');

  const [endpoints, setEndpoints] = useState<EndpointSummary[]>([]);
  const [savedRequests, setSavedRequests] = useState<SavedRequestInfo[]>([]);
  const [environments, setEnvironments] = useState<EnvironmentConfig[]>([]);
  const [environmentVars, setEnvironmentVars] = useState<EnvironmentVariableInfo[]>([]);
  const [vaultEntries, setVaultEntries] = useState<VaultEntryMetadata[]>([]);
  const [vaultUnlocked, setVaultUnlocked] = useState(false);

  const [response, setResponse] = useState<RequestResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const setRow = (
    rows: KeyValueRow[],
    setter: React.Dispatch<React.SetStateAction<KeyValueRow[]>>,
    id: number,
    patch: Partial<KeyValueRow>
  ) => {
    setter(rows.map((row) => (row.id === id ? { ...row, ...patch } : row)));
  };

  const deleteRow = (
    rows: KeyValueRow[],
    setter: React.Dispatch<React.SetStateAction<KeyValueRow[]>>,
    id: number
  ) => {
    const next = rows.filter((row) => row.id !== id);
    setter(next.length ? next : [createRow()]);
  };

  const composedUrl = useMemo(() => {
    const [base, existingQuery = ''] = url.split('?');
    const searchParams = new URLSearchParams(existingQuery);
    params
      .filter((row) => row.enabled && row.key.trim())
      .forEach((row) => {
        searchParams.set(row.key.trim(), row.value);
      });
    const query = searchParams.toString();
    return query ? `${base}?${query}` : base;
  }, [url, params]);

  const enabledHeaders = useMemo(() => {
    return headers
      .filter((row) => row.enabled && row.key.trim())
      .reduce<Record<string, string>>((acc, row) => {
        acc[row.key.trim()] = row.value;
        return acc;
      }, {});
  }, [headers]);

  const parseBody = () => {
    if (!body.trim()) {
      return null;
    }
    try {
      return JSON.parse(body);
    } catch {
      throw new Error('Request body is not valid JSON');
    }
  };

  const buildAuthentication = () => {
    if (authMode === 'none') {
      return null;
    }
    if (!authVaultEntry.trim()) {
      throw new Error('Select a vault entry for authentication');
    }

    if (authMode === 'bearer') {
      return {
        auth_type: 'bearer_token',
        vault_entry_name: authVaultEntry,
        location: null,
        header_name: null,
        prefix: authPrefix.trim() || 'Bearer',
      };
    }

    return {
      auth_type: 'api_key',
      vault_entry_name: authVaultEntry,
      location: 'header',
      header_name: authHeaderName.trim() || 'X-API-Key',
      prefix: null,
    };
  };

  const insertVariableToken = (key: string) => {
    const token = `{{${key}}}`;
    if (activeTab === 'body') {
      setBody((prev) => `${prev}${token}`);
    } else {
      setUrl((prev) => `${prev}${token}`);
    }
  };

  const loadBuilderData = async () => {
    setError(null);
    try {
      const [endpointResult, savedResult, envResult, vaultState] = await Promise.all([
        invoke<EndpointSummary[]>('endpoint_list', { request: null }),
        invoke<SavedRequestInfo[]>('request_saved_list'),
        invoke<EnvironmentConfig[]>('environment_list'),
        invoke<{ state: string }>('vault_state'),
      ]);

      setEndpoints(endpointResult);
      setSavedRequests(savedResult);
      setEnvironments(envResult);

      const unlocked = vaultState.state === 'unlocked';
      setVaultUnlocked(unlocked);
      if (unlocked) {
        const entries = await invoke<VaultEntryMetadata[]>('vault_list');
        setVaultEntries(entries);
      } else {
        setVaultEntries([]);
      }

      const activeEnv = envResult.find((env) => env.is_active)?.id ?? envResult[0]?.id ?? null;
      setSelectedEnvironmentId(activeEnv);
      if (activeEnv) {
        const vars = await invoke<EnvironmentVariableInfo[]>('environment_variables', {
          request: { id: activeEnv },
        });
        setEnvironmentVars(vars);
      } else {
        setEnvironmentVars([]);
      }
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  useEffect(() => {
    void loadBuilderData();
  }, [_project.path]);

  useEffect(() => {
    if (!selectedEnvironmentId) {
      setEnvironmentVars([]);
      return;
    }
    const loadVars = async () => {
      try {
        const vars = await invoke<EnvironmentVariableInfo[]>('environment_variables', {
          request: { id: selectedEnvironmentId },
        });
        setEnvironmentVars(vars);
      } catch (err) {
        setError(errorMessage(err));
      }
    };

    void loadVars();
  }, [selectedEnvironmentId]);

  const handleSend = async () => {
    setIsLoading(true);
    setError(null);
    setNotice(null);
    try {
      const result = await invoke<RequestResult>('request_execute', {
        request: {
          method,
          url: composedUrl,
          headers: Object.keys(enabledHeaders).length ? enabledHeaders : null,
          body: parseBody(),
          environmentId: selectedEnvironmentId,
          authentication: buildAuthentication(),
        },
      });
      setResponse(result);
      setActiveTab('response');
      setNotice(`Request returned ${result.status}.`);
    } catch (err) {
      setError(errorMessage(err));
      setResponse(null);
    } finally {
      setIsLoading(false);
    }
  };

  const handleSave = async () => {
    const name = saveName.trim();
    if (!name) {
      setError('Enter a name to save this request.');
      return;
    }

    setError(null);
    setNotice(null);
    try {
      await invoke('request_save', {
        request: {
          name,
          method,
          url: composedUrl,
          headers: Object.keys(enabledHeaders).length ? enabledHeaders : null,
          body: parseBody(),
        },
      });
      setSaveName('');
      setNotice(`Saved request '${name}'.`);
      const refreshed = await invoke<SavedRequestInfo[]>('request_saved_list');
      setSavedRequests(refreshed);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const applyEndpointPreset = async () => {
    if (!selectedEndpointId) {
      setError('Select an endpoint preset first.');
      return;
    }

    setError(null);
    try {
      const detail = await invoke<EndpointDetail>('endpoint_get', {
        request: { id: selectedEndpointId },
      });

      setMethod(detail.method.toUpperCase());
      setUrl(`{{baseUrl}}${detail.path}`);
      const mappedParams = detail.parameters
        .filter((parameter) => parameter.location === 'query' || parameter.location === 'path')
        .map<KeyValueRow>((parameter) => ({
          id: Date.now() + Math.floor(Math.random() * 100000),
          key: parameter.name,
          value: parameter.location === 'path' ? `{${parameter.name}}` : '',
          enabled: true,
        }));
      setParams(mappedParams.length ? mappedParams : [createRow()]);
      if (detail.request_body?.example != null) {
        setBody(JSON.stringify(detail.request_body.example, null, 2));
      } else {
        setBody('');
      }
      setNotice('Applied endpoint preset to builder.');
      setActiveTab('body');
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const loadSavedPreset = (saved: SavedRequestInfo) => {
    setMethod(saved.method.toUpperCase());
    if (saved.url) {
      setUrl(saved.url);
    }
    setNotice(`Loaded saved request '${saved.name}'.`);
  };

  const generateLocalToken = () => {
    const generated = `tok_${crypto.randomUUID().replace(/-/g, '')}`;
    setNewTokenValue(generated);
    setNotice('Generated token value. Save it to Vault to use it in auth mode.');
  };

  const saveGeneratedTokenToVault = async () => {
    const name = newTokenName.trim();
    const value = newTokenValue.trim();
    if (!name || !value) {
      setError('Provide token name and value before saving to Vault.');
      return;
    }

    try {
      await invoke('vault_create', {
        request: {
          name,
          secretType: 'bearer_token',
          value,
        },
      });
      const entries = await invoke<VaultEntryMetadata[]>('vault_list');
      setVaultEntries(entries);
      setVaultUnlocked(true);
      setAuthVaultEntry(name);
      setNotice(`Saved token '${name}' to Vault.`);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  return (
    <div>
      <h2>Request Builder</h2>
      <p style={{ color: '#6c757d', marginBottom: '0.75rem' }}>
        Build calls with presets, parameters, headers, auth from Vault, environment tokens, and quick save.
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      {notice && (
        <div className="success-banner">
          <span>{notice}</span>
          <button onClick={() => setNotice(null)}>&times;</button>
        </div>
      )}

      <div className="request-builder">
        <div className="request-url-bar">
          <select className="method-select" value={method} onChange={(event) => setMethod(event.target.value)}>
            {['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'].map((item) => (
              <option key={item} value={item}>
                {item}
              </option>
            ))}
          </select>
          <input
            type="text"
            className="url-input"
            value={composedUrl}
            onChange={(event) => setUrl(event.target.value)}
            placeholder="Enter URL, e.g. {{baseUrl}}/users"
          />
          <button className="send-button" onClick={handleSend} disabled={isLoading}>
            {isLoading ? 'Sending...' : 'Send'}
          </button>
        </div>

        <div
          style={{
            display: 'flex',
            gap: '0.5rem',
            alignItems: 'center',
            padding: '0.75rem 1rem',
            borderBottom: '1px solid #e9ecef',
            flexWrap: 'wrap',
          }}
        >
          <span style={{ color: '#6c757d', fontSize: '0.85rem' }}>Environment:</span>
          <select
            className="method-select"
            style={{ minWidth: '180px' }}
            value={selectedEnvironmentId ?? ''}
            onChange={(event) => setSelectedEnvironmentId(event.target.value || null)}
          >
            <option value="">Auto</option>
            {environments.map((environment) => (
              <option key={environment.id} value={environment.id}>
                {environment.name}
                {environment.is_active ? ' (active)' : ''}
              </option>
            ))}
          </select>

          <input
            type="text"
            className="url-input"
            style={{ maxWidth: '220px' }}
            value={saveName}
            onChange={(event) => setSaveName(event.target.value)}
            placeholder="save request as..."
          />
          <button className="control-button" onClick={handleSave}>
            Save Request
          </button>
        </div>

        <div className="tabs">
          <button className={`tab ${activeTab === 'presets' ? 'active' : ''}`} onClick={() => setActiveTab('presets')}>
            Presets
          </button>
          <button className={`tab ${activeTab === 'params' ? 'active' : ''}`} onClick={() => setActiveTab('params')}>
            Parameters
          </button>
          <button className={`tab ${activeTab === 'headers' ? 'active' : ''}`} onClick={() => setActiveTab('headers')}>
            Headers
          </button>
          <button className={`tab ${activeTab === 'auth' ? 'active' : ''}`} onClick={() => setActiveTab('auth')}>
            Auth
          </button>
          <button className={`tab ${activeTab === 'body' ? 'active' : ''}`} onClick={() => setActiveTab('body')}>
            Body
          </button>
          <button className={`tab ${activeTab === 'response' ? 'active' : ''}`} onClick={() => setActiveTab('response')}>
            Response
          </button>
        </div>

        {activeTab === 'presets' && (
          <div style={{ padding: '1rem', display: 'grid', gap: '0.75rem' }}>
            <div>
              <h4 style={{ marginBottom: '0.4rem' }}>Endpoint presets</h4>
              <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
                <select
                  className="method-select"
                  style={{ minWidth: '320px' }}
                  value={selectedEndpointId}
                  onChange={(event) => setSelectedEndpointId(event.target.value)}
                >
                  <option value="">Select discovered endpoint...</option>
                  {endpoints.map((endpoint) => (
                    <option key={endpoint.id} value={endpoint.id}>
                      {endpoint.method} {endpoint.path}
                      {endpoint.summary ? ` - ${endpoint.summary}` : ''}
                    </option>
                  ))}
                </select>
                <button className="control-button" onClick={applyEndpointPreset}>
                  Apply Endpoint Preset
                </button>
              </div>
            </div>

            <div>
              <h4 style={{ marginBottom: '0.4rem' }}>Quick templates</h4>
              <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
                <button
                  className="control-button"
                  onClick={() => {
                    setMethod('GET');
                    setUrl('{{baseUrl}}/health');
                    setBody('');
                    setNotice('Loaded health-check template.');
                  }}
                >
                  Health Check GET
                </button>
                <button
                  className="control-button"
                  onClick={() => {
                    setMethod('POST');
                    setUrl('{{baseUrl}}/auth/token');
                    setBody(
                      JSON.stringify(
                        { client_id: '{{CLIENT_ID}}', client_secret: '{{CLIENT_SECRET}}' },
                        null,
                        2
                      )
                    );
                    setActiveTab('body');
                    setNotice('Loaded token request template.');
                  }}
                >
                  Token Request POST
                </button>
              </div>
            </div>

            <div>
              <h4 style={{ marginBottom: '0.4rem' }}>Saved requests</h4>
              {savedRequests.length === 0 ? (
                <p style={{ color: '#6c757d' }}>No saved requests yet.</p>
              ) : (
                <div style={{ display: 'grid', gap: '0.4rem' }}>
                  {savedRequests.map((saved) => (
                    <button
                      key={saved.name}
                      className="control-button"
                      style={{ textAlign: 'left' }}
                      onClick={() => loadSavedPreset(saved)}
                    >
                      {saved.method} {saved.name} {saved.url ? `-> ${saved.url}` : ''}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div>
              <h4 style={{ marginBottom: '0.4rem' }}>Environment variable tokens</h4>
              {environmentVars.length === 0 ? (
                <p style={{ color: '#6c757d' }}>No variables found for this environment.</p>
              ) : (
                <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
                  {environmentVars.map((variable) => (
                    <button
                      key={variable.key}
                      className="control-button"
                      onClick={() => insertVariableToken(variable.key)}
                      title={`Current value: ${variable.value}`}
                    >
                      {`{{${variable.key}}}`}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}

        {activeTab === 'params' && (
          <div style={{ padding: '1rem', display: 'grid', gap: '0.5rem' }}>
            {params.map((row) => (
              <div
                key={row.id}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '28px 1fr 1fr auto',
                  gap: '0.5rem',
                  alignItems: 'center',
                }}
              >
                <input
                  type="checkbox"
                  checked={row.enabled}
                  onChange={(event) => setRow(params, setParams, row.id, { enabled: event.target.checked })}
                />
                <input
                  className="url-input"
                  placeholder="param"
                  value={row.key}
                  onChange={(event) => setRow(params, setParams, row.id, { key: event.target.value })}
                />
                <input
                  className="url-input"
                  placeholder="value"
                  value={row.value}
                  onChange={(event) => setRow(params, setParams, row.id, { value: event.target.value })}
                />
                <button className="control-button" onClick={() => deleteRow(params, setParams, row.id)}>
                  Remove
                </button>
              </div>
            ))}
            <button className="control-button" onClick={() => setParams([...params, createRow()])}>
              Add Parameter
            </button>
          </div>
        )}

        {activeTab === 'headers' && (
          <div style={{ padding: '1rem', display: 'grid', gap: '0.5rem' }}>
            {headers.map((row) => (
              <div
                key={row.id}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '28px 1fr 1fr auto',
                  gap: '0.5rem',
                  alignItems: 'center',
                }}
              >
                <input
                  type="checkbox"
                  checked={row.enabled}
                  onChange={(event) => setRow(headers, setHeaders, row.id, { enabled: event.target.checked })}
                />
                <input
                  className="url-input"
                  placeholder="Header-Name"
                  value={row.key}
                  onChange={(event) => setRow(headers, setHeaders, row.id, { key: event.target.value })}
                />
                <input
                  className="url-input"
                  placeholder="Header value"
                  value={row.value}
                  onChange={(event) => setRow(headers, setHeaders, row.id, { value: event.target.value })}
                />
                <button className="control-button" onClick={() => deleteRow(headers, setHeaders, row.id)}>
                  Remove
                </button>
              </div>
            ))}
            <button className="control-button" onClick={() => setHeaders([...headers, createRow()])}>
              Add Header
            </button>
          </div>
        )}

        {activeTab === 'auth' && (
          <div style={{ padding: '1rem', display: 'grid', gap: '0.75rem' }}>
            <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
              <button className="control-button" onClick={() => setAuthMode('none')}>
                None
              </button>
              <button className="control-button" onClick={() => setAuthMode('bearer')}>
                Bearer Token
              </button>
              <button className="control-button" onClick={() => setAuthMode('api_key')}>
                API Key
              </button>
            </div>

            {authMode !== 'none' && (
              <>
                {vaultUnlocked ? (
                  <select
                    className="method-select"
                    style={{ minWidth: '320px' }}
                    value={authVaultEntry}
                    onChange={(event) => setAuthVaultEntry(event.target.value)}
                  >
                    <option value="">Select vault credential...</option>
                    {vaultEntries.map((entry) => (
                      <option key={entry.name} value={entry.name}>
                        {entry.name} ({entry.secret_type})
                      </option>
                    ))}
                  </select>
                ) : (
                  <p style={{ color: '#6c757d' }}>
                    Vault is locked. Unlock it in Vault view to use stored credentials.
                  </p>
                )}

                {authMode === 'api_key' && (
                  <input
                    className="url-input"
                    style={{ maxWidth: '260px' }}
                    value={authHeaderName}
                    onChange={(event) => setAuthHeaderName(event.target.value)}
                    placeholder="API key header name"
                  />
                )}

                {authMode === 'bearer' && (
                  <input
                    className="url-input"
                    style={{ maxWidth: '260px' }}
                    value={authPrefix}
                    onChange={(event) => setAuthPrefix(event.target.value)}
                    placeholder="Bearer prefix"
                  />
                )}
              </>
            )}

            <div style={{ borderTop: '1px solid #e9ecef', paddingTop: '0.75rem' }}>
              <h4 style={{ marginBottom: '0.4rem' }}>Token helpers</h4>
              <div style={{ display: 'grid', gap: '0.5rem', maxWidth: '560px' }}>
                <input
                  className="url-input"
                  value={newTokenName}
                  onChange={(event) => setNewTokenName(event.target.value)}
                  placeholder="Vault entry name"
                />
                <input
                  className="url-input"
                  value={newTokenValue}
                  onChange={(event) => setNewTokenValue(event.target.value)}
                  placeholder="Token value"
                />
                <div style={{ display: 'flex', gap: '0.5rem' }}>
                  <button className="control-button" onClick={generateLocalToken}>
                    Generate Token Value
                  </button>
                  <button className="control-button" onClick={() => void saveGeneratedTokenToVault()}>
                    Save To Vault
                  </button>
                </div>
              </div>
            </div>
          </div>
        )}

        {activeTab === 'body' && (
          <textarea
            className="request-body"
            value={body}
            onChange={(event) => setBody(event.target.value)}
            placeholder='{
  "example": "value",
  "token": "{{AUTH_TOKEN}}"
}'
          />
        )}

        {activeTab === 'response' && response && (
          <div className="response-viewer">
            <div className="response-header">
              <span
                className={`response-status ${
                  response.status >= 200 && response.status < 300 ? 'success' : 'error'
                }`}
              >
                {response.status}
              </span>
              <span className="response-meta">{response.duration_ms} ms</span>
              <span className="response-meta">{response.body_size} B</span>
            </div>
            <pre className="response-body">{JSON.stringify(response.body, null, 2)}</pre>
            {response.validation.issues.length > 0 && (
              <div style={{ marginTop: '0.75rem' }}>
                <h4 style={{ marginBottom: '0.35rem' }}>Validation issues</h4>
                {response.validation.issues.map((issue, idx) => (
                  <div key={idx} style={{ color: issue.severity === 'error' ? '#991b1b' : '#6c757d' }}>
                    {issue.message}
                    {issue.path ? ` (${issue.path})` : ''}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {activeTab === 'response' && !response && (
          <div style={{ padding: '1rem', color: '#6c757d' }}>Send a request to see the response.</div>
        )}
      </div>
    </div>
  );
}

export default Requests;
