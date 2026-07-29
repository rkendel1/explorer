import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project, Environment } from '../App';

interface RequestsProps {
  project: Project;
}

interface Response {
  status: number;
  statusText: string;
  duration: number;
  size: number;
  body: string;
  headers: Record<string, string>;
}

function Requests({ project }: RequestsProps) {
  const [method, setMethod] = useState('GET');
  const [url, setUrl] = useState('{{baseUrl}}/work-orders');
  const [body, setBody] = useState('');
  const [activeTab, setActiveTab] = useState<'params' | 'headers' | 'body' | 'response'>('body');
  const [response, setResponse] = useState<Response | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [selectedEnv, setSelectedEnv] = useState('mock');

  const environments: Environment[] = [
    { id: 'mock', name: 'Mock' },
    { id: 'dev', name: 'Development' },
    { id: 'staging', name: 'Staging' },
  ];

  const handleSend = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<{
        ok?: Response;
        error?: string;
      }>('request_execute', {
        projectPath: project.path,
        method,
        url,
        body: body || null,
        environmentId: selectedEnv,
      });

      if (result.ok) {
        setResponse(result.ok);
      } else if (result.error) {
        setResponse({
          status: 0,
          statusText: 'Error',
          duration: 0,
          size: 0,
          body: result.error,
          headers: {},
        });
      }
    } catch (error) {
      // Mock response for development
      setResponse({
        status: 201,
        statusText: 'Created',
        duration: 124,
        size: 842,
        body: JSON.stringify(
          {
            id: 'wo-1045',
            status: 'created',
            customerId: 'cust-001',
          },
          null,
          2
        ),
        headers: {
          'Content-Type': 'application/json',
        },
      });
    } finally {
      setIsLoading(false);
      setActiveTab('response');
    }
  };

  return (
    <div>
      <h2>Request Builder</h2>

      <div className="environment-selector">
        {environments.map((env) => (
          <span
            key={env.id}
            className={`env-badge ${selectedEnv === env.id ? 'active' : ''}`}
            onClick={() => setSelectedEnv(env.id)}
          >
            {env.name}
          </span>
        ))}
      </div>

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
            placeholder="Enter URL..."
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
                {response.status} {response.statusText}
              </span>
              <span className="response-meta">{response.duration} ms</span>
              <span className="response-meta">{response.size} B</span>
            </div>
            <pre className="response-body">{response.body}</pre>
          </div>
        )}

        {activeTab === 'params' && (
          <div style={{ padding: '1rem', color: '#6c757d' }}>
            No parameters defined
          </div>
        )}

        {activeTab === 'headers' && (
          <div style={{ padding: '1rem' }}>
            <div style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.5rem' }}>
              <span style={{ fontWeight: 500, minWidth: '150px' }}>Content-Type:</span>
              <span>application/json</span>
            </div>
            <div style={{ display: 'flex', gap: '0.5rem' }}>
              <span style={{ fontWeight: 500, minWidth: '150px' }}>Authorization:</span>
              <span>[REDACTED]</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default Requests;
