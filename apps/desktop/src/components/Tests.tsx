import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
import type { Project } from '../App';

interface TestsProps {
  project: Project;
}

interface TestSuiteSummary {
  id: string;
  name: string;
}

interface AssertionResult {
  passed: boolean;
  message: string;
  expected: string | null;
  actual: string | null;
}

interface TestResultDetail {
  test_id: string;
  test_name: string;
  passed: boolean;
  duration_ms: number;
  assertions: AssertionResult[];
}

interface TestRunResult {
  run_id: string;
  passed: number;
  failed: number;
  skipped: number;
  duration_ms: number;
  results: TestResultDetail[];
}

function Tests({ project: _project }: TestsProps) {
  const [suites, setSuites] = useState<TestSuiteSummary[]>([]);
  const [run, setRun] = useState<TestRunResult | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedTest, setSelectedTest] = useState<TestResultDetail | null>(null);

  useEffect(() => {
    loadSuites();
  }, [_project.path]);

  const loadSuites = async () => {
    setError(null);
    try {
      const result = await invoke<TestSuiteSummary[]>('test_list');
      setSuites(result);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const handleRunAll = async () => {
    setIsRunning(true);
    setError(null);
    try {
      const result = await invoke<TestRunResult>('test_run', {
        request: { suiteId: null, testIds: null },
      });
      setRun(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsRunning(false);
    }
  };

  const handleExportJunit = async () => {
    setError(null);
    try {
      const xml = await invoke<string>('test_export', {
        request: { format: 'junit', suiteId: null },
      });
      const blob = new Blob([xml], { type: 'application/xml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'test-results.xml';
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const results = run?.results ?? [];

  return (
    <div>
      <h2>API Tests</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {suites.length} suite{suites.length === 1 ? '' : 's'} found
        {run && ` • ${run.passed} passed • ${run.failed} failed • ${run.skipped} skipped`}
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      {suites.length === 0 && !error && (
        <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
          No test suites found under <code>.repo-api/tests/suites/</code>.
        </p>
      )}

      <div style={{ marginBottom: '1rem', display: 'flex', gap: '0.5rem' }}>
        <button className="control-button primary" onClick={handleRunAll} disabled={isRunning}>
          {isRunning ? 'Running...' : 'Run All'}
        </button>
        <button className="control-button" onClick={handleExportJunit} disabled={!run}>
          Export JUnit
        </button>
      </div>

      <div className="test-list">
        {results.map((test) => (
          <div
            key={test.test_id}
            className="test-item"
            onClick={() => setSelectedTest(test)}
            style={{ cursor: 'pointer' }}
          >
            <span className="test-name">{test.test_name}</span>
            <span className={`test-status ${test.passed ? 'passed' : 'failed'}`}>
              {test.passed ? 'Passed' : 'Failed'}
            </span>
          </div>
        ))}
      </div>

      {selectedTest && !selectedTest.passed && (
        <div style={{ marginTop: '1rem', padding: '1rem', border: '1px solid #fee2e2', borderRadius: '0.5rem', background: '#fef2f2' }}>
          <h4 style={{ color: '#991b1b', marginBottom: '0.5rem' }}>{selectedTest.test_name}</h4>
          <div style={{ fontSize: '0.875rem' }}>
            {selectedTest.assertions.filter((a) => !a.passed).map((a, i) => (
              <p key={i}>
                <strong>{a.message}</strong>
                {a.expected && <> — expected: {a.expected}</>}
                {a.actual && <>, actual: {a.actual}</>}
              </p>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default Tests;
