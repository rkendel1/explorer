import { useState } from 'react';
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

function Requests({ project: _project }: RequestsProps) {
  const [method, setMethod] = useState('GET');
  const [url, setUrl] = useState('{{baseUrl}}/work-orders');
  const [body, setBody] = useState('');
  const [activeTab, setActiveTab] = useState<'params' | 'headers' | 'body' | 'response'>('body');
  const [response, setResponse] = useState<RequestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const handleSend = async () => {
    setIsLoading(true);
    setError(null);
    try {
      let parsedBody: unknown = null;
      if (body.trim()) {
        try {
          parsedBody = JSON.parse(body);
        } catch {
          throw new Error('Request body is not valid JSON');
        }
      }

      const result = await invoke<RequestResult>('request_execute', {
        request: {
          method,
          url,
          headers: null,
          body: parsedBody,
          environmentId: null,
        },
      });
      setResponse(result);
      setActiveTab('response');
    } catch (err) {
      setError(errorMessage(err));
      setResponse(null);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div>
      <h2>Request Builder</h2>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      <div className="request-builder">
        <div className="request-url-bar">
          <select
            className="method-select"
            value={method}
            onChange={(e) => setMethod(e.target.value)}
          >
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="PATCH">PATCH</option>
            <option value="DELETE">DELETE</option>
          </select>
          <input
            type="text"
            className="url-input"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="Enter URL, e.g. {{baseUrl}}/users"
          />
          <button
            className="send-button"
            onClick={handleSend}
            disabled={isLoading}
          >
            {isLoading ? 'Sending...' : 'Send'}
          </button>
        </div>

        <div className="tabs">
          <button
            className={`tab ${activeTab === 'params' ? 'active' : ''}`}
            onClick={() => setActiveTab('params')}
          >
            Parameters
          </button>
          <button
            className={`tab ${activeTab === 'headers' ? 'active' : ''}`}
            onClick={() => setActiveTab('headers')}
          >
            Headers
          </button>
          <button
            className={`tab ${activeTab === 'body' ? 'active' : ''}`}
            onClick={() => setActiveTab('body')}
          >
            Body
          </button>
          <button
            className={`tab ${activeTab === 'response' ? 'active' : ''}`}
            onClick={() => setActiveTab('response')}
          >
            Response
          </button>
        </div>

        {activeTab === 'body' && (
          <textarea
            className="request-body"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder='{\n  "customerId": "cust-001"\n}'
          />
        )}

        {activeTab === 'response' && response && (
          <div className="response-viewer">
            <div className="response-header">
              <span
                className={`response-status ${
                  response.status >= 200 && response.status < 300
                    ? 'success'
                    : 'error'
                }`}
              >
                {response.status}
              </span>
              <span className="response-meta">{response.duration_ms} ms</span>
              <span className="response-meta">{response.body_size} B</span>
            </div>
            <pre className="response-body">{JSON.stringify(response.body, null, 2)}</pre>
          </div>
        )}
        {activeTab === 'response' && !response && (
          <div style={{ padding: '1rem', color: '#6c757d' }}>Send a request to see the response.</div>
        )}

        {activeTab === 'params' && (
          <div style={{ padding: '1rem', color: '#6c757d' }}>
            No parameters defined
          </div>
        )}

        {activeTab === 'headers' && (
          <div style={{ padding: '1rem' }}>
            {response ? (
              response.headers.map(([name, value]) => (
                <div key={name} style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.5rem' }}>
                  <span style={{ fontWeight: 500, minWidth: '150px' }}>{name}:</span>
                  <span>{value}</span>
                </div>
              ))
            ) : (
              <span style={{ color: '#6c757d' }}>Send a request to see response headers.</span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default Requests;
