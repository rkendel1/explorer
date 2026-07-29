import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Project, WorkflowStep } from '../App';

interface WorkflowsProps {
  project: Project;
}

function Workflows({ project }: WorkflowsProps) {
  const [steps, setSteps] = useState<WorkflowStep[]>([]);

  useEffect(() => {
    loadWorkflows();
  }, [project.path]);

  const loadWorkflows = async () => {
    try {
      const result = await invoke<{ ok: { steps: WorkflowStep[] }[] }>('workflow_list', {
        projectPath: project.path,
      });
      if (result.ok && result.ok.length > 0) {
        setSteps(result.ok[0].steps);
      }
    } catch (error) {
      console.error('Failed to load workflows:', error);
      // Mock data for development
      setSteps([
        {
          id: 'connect-repository',
          name: 'Connect Repository',
          description: 'Open a repository to begin API discovery',
          completed: true,
          current: false,
        },
        {
          id: 'analyze-api',
          name: 'Analyze API',
          description: 'Run analysis to discover API endpoints',
          completed: true,
          current: false,
        },
        {
          id: 'first-request',
          name: 'Run Your First Request',
          description: 'Execute a request against an endpoint',
          completed: false,
          current: true,
        },
        {
          id: 'configure-auth',
          name: 'Configure Authentication',
          description: 'Set up authentication credentials',
          completed: false,
          current: false,
        },
        {
          id: 'create-scenario',
          name: 'Create Mock Scenario',
          description: 'Define a mock response scenario',
          completed: false,
          current: false,
        },
        {
          id: 'run-tests',
          name: 'Run Test Suite',
          description: 'Execute the API test suite',
          completed: false,
          current: false,
        },
      ]);
    }
  };

  const getStepIndicator = (step: WorkflowStep) => {
    if (step.completed) {
      return <span className="step-indicator completed">✓</span>;
    }
    if (step.current) {
      return <span className="step-indicator current">→</span>;
    }
    return <span className="step-indicator pending">○</span>;
  };

  return (
    <div>
      <h2>Getting Started</h2>
      <p style={{ color: '#6c757d', marginBottom: '1.5rem' }}>
        Follow these steps to get the most out of Repo API
      </p>

      <div className="workflow-list">
        {steps.map((step) => (
          <div key={step.id} className="workflow-step">
            {getStepIndicator(step)}
            <div className="step-content">
              <div className="step-title">{step.name}</div>
              <div className="step-description">{step.description}</div>
            </div>
            {step.current && (
              <button className="action-button" style={{ fontSize: '0.875rem', padding: '0.5rem 1rem' }}>
                Start
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default Workflows;
