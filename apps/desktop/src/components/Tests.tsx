import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project } from '../App';

interface TestsProps {
  project: Project;
}

interface TestResult {
  id: string;
  name: string;
  status: 'passed' | 'failed' | 'pending';
  expected?: string;
  actual?: string;
  message?: string;
}

function Tests({ project }: TestsProps) {
  const [tests, setTests] = useState<TestResult[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [selectedTest, setSelectedTest] = useState<TestResult | null>(null);

  useEffect(() => {
    loadTests();
  }, [project.path]);

  const loadTests = async () => {
    try {
      const result = await invoke<{ ok: TestResult[] }>('test_list', {
        projectPath: project.path,
      });
      if (result.ok) {
        setTests(result.ok);
      }
    } catch (error) {
      console.error('Failed to load tests:', error);
      // Mock data for development
      setTests([
        { id: '1', name: 'Create Work Order', status: 'passed' },
        { id: '2', name: 'Get Work Order', status: 'passed' },
        { id: '3', name: 'Invalid Request', status: 'failed', expected: '400', actual: '201', message: 'Status mismatch' },
        { id: '4', name: 'Delete Work Order', status: 'pending' },
      ]);
    }
  };

  const handleRunAll = async () => {
    setIsRunning(true);
    try {
      const result = await invoke<{ ok: TestResult[] }>('test_run', {
        projectPath: project.path,
        testIds: null,
      });
      if (result.ok) {
        setTests(result.ok);
      }
    } catch {
      // Keep current test results
    } finally {
      setIsRunning(false);
    }
  };

  const passedCount = tests.filter((t) => t.status === 'passed').length;
  const failedCount = tests.filter((t) => t.status === 'failed').length;

  return (
    <div>
      <h2>API Tests</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        {passedCount} passed • {failedCount} failed • {tests.length} total
      </p>

      <div style={{ marginBottom: '1rem', display: 'flex', gap: '0.5rem' }}>
        <button className="control-button primary" onClick={handleRunAll} disabled={isRunning}>
          {isRunning ? 'Running...' : 'Run All'}
        </button>
        <button className="control-button">
          Export JUnit
        </button>
      </div>

      <div className="test-list">
        {tests.map((test) => (
          <div
            key={test.id}
            className="test-item"
            onClick={() => setSelectedTest(test)}
            style={{ cursor: 'pointer' }}
          >
            <span className="test-name">{test.name}</span>
            <span className={`test-status ${test.status}`}>
              {test.status === 'passed' ? 'Passed' : test.status === 'failed' ? 'Failed' : 'Pending'}
            </span>
          </div>
        ))}
      </div>

      {selectedTest && selectedTest.status === 'failed' && (
        <div style={{ marginTop: '1rem', padding: '1rem', border: '1px solid #fee2e2', borderRadius: '0.5rem', background: '#fef2f2' }}>
          <h4 style={{ color: '#991b1b', marginBottom: '0.5rem' }}>{selectedTest.name}</h4>
          <div style={{ fontSize: '0.875rem' }}>
            <p><strong>Expected:</strong> {selectedTest.expected}</p>
            <p><strong>Actual:</strong> {selectedTest.actual}</p>
            <p><strong>Message:</strong> {selectedTest.message}</p>
          </div>
        </div>
      )}
    </div>
  );
}

export default Tests;
