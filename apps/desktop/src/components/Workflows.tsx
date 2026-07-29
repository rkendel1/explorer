import { useState } from 'react';
import type { Project } from '../App';

type NavigationItem =
  | 'projects'
  | 'explorer'
  | 'requests'
  | 'workflows'
  | 'tests'
  | 'runtime'
  | 'vault'
  | 'changes'
  | 'settings';

interface WorkflowsProps {
  project: Project;
  onNavigate: (item: NavigationItem) => void;
}

type CustomerGoal =
  | 'try_endpoint'
  | 'create_mock_api'
  | 'test_my_api'
  | 'explore_my_api';

type Outcome = {
  id: string;
  label: string;
  completed: boolean;
};

function Workflows({ project, onNavigate }: WorkflowsProps) {
  const [goal, setGoal] = useState<CustomerGoal | null>(null);
  const [outcomes, setOutcomes] = useState<Outcome[]>([
    { id: 'repository_connected', label: 'Connected your repository', completed: true },
    {
      id: 'api_discovered',
      label: 'Mapped your API',
      completed: (project.endpointCount ?? 0) > 0,
    },
    { id: 'first_request', label: 'Sent a successful request', completed: false },
    { id: 'environment_ready', label: 'Configured an environment', completed: false },
    { id: 'reusable_request', label: 'Created a reusable request', completed: false },
    { id: 'mock_ready', label: 'Started a mock API', completed: false },
    { id: 'test_complete', label: 'Ran your first test', completed: false },
  ]);

  const completed = outcomes.filter((o) => o.completed).length;
  const total = outcomes.length;
  const complete = completed === total;

  const nextOutcome = outcomes.find((o) => !o.completed);

  const recommendation = (() => {
    if (!nextOutcome) {
      return {
        title: 'Your API Workspace Is Ready',
        description: 'You are ready to explore the full workspace.',
        actionLabel: 'Open My API Workspace',
        action: () => onNavigate('explorer' as NavigationItem),
      };
    }

    switch (nextOutcome.id) {
      case 'api_discovered':
        return {
          title: 'Review what we discovered',
          description: 'We found your API. Review endpoints and models before continuing.',
          actionLabel: 'Review Your API',
          action: () => {
            markOutcome('api_discovered');
            onNavigate('explorer');
          },
        };
      case 'first_request':
        return {
          title: 'Try your first request',
          description: 'Choose a safe endpoint and send your first API request.',
          actionLabel: 'Try An Endpoint',
          action: () => {
            setGoal(goal ?? 'try_endpoint');
            onNavigate('requests');
            markOutcome('first_request');
          },
        };
      case 'environment_ready':
        return {
          title: 'Choose how to run your API',
          description: 'Use mock, development, or staging as your first runtime target.',
          actionLabel: 'Configure Environment',
          action: () => {
            onNavigate('requests');
            markOutcome('environment_ready');
          },
        };
      case 'reusable_request':
        return {
          title: 'Make it repeatable',
          description: 'Save your working request so it can be re-used in tests.',
          actionLabel: 'Save Request',
          action: () => markOutcome('reusable_request'),
        };
      case 'mock_ready':
        return {
          title: 'Run through a mock environment',
          description: 'Start a local mock API generated from your API contract.',
          actionLabel: 'Start Mock API',
          action: () => {
            onNavigate('runtime');
            markOutcome('mock_ready');
          },
        };
      case 'test_complete':
        return {
          title: 'Validate it with tests',
          description: 'Run a first test to make behavior safe and repeatable.',
          actionLabel: 'Run First Test',
          action: () => {
            onNavigate('tests');
            markOutcome('test_complete');
          },
        };
      default:
        return {
          title: 'Continue setup',
          description: 'Proceed through guided setup to reach first value.',
          actionLabel: 'Continue',
          action: () => {},
        };
    }
  })();

  const markOutcome = (id: string) => {
    setOutcomes((prev) => prev.map((o) => (o.id === id ? { ...o, completed: true } : o)));
  };

  return (
    <div>
      <h2>Getting Started</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        Connect repository → discover API → first successful request → mock → tests
      </p>

      <div className="runtime-card" style={{ marginBottom: '1rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>{recommendation.title}</h3>
        <p style={{ color: '#6c757d', marginBottom: '0.75rem' }}>{recommendation.description}</p>
        <button className="action-button" onClick={recommendation.action}>
          {recommendation.actionLabel}
        </button>
        <button className="control-button" style={{ marginLeft: '0.5rem' }}>
          Do This Later
        </button>
      </div>

      <div className="runtime-card" style={{ marginBottom: '1rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>What Would You Like To Do?</h3>
        <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
          <button className="control-button" onClick={() => setGoal('try_endpoint')}>Try An Endpoint</button>
          <button className="control-button" onClick={() => setGoal('create_mock_api')}>Create A Mock API</button>
          <button className="control-button" onClick={() => setGoal('test_my_api')}>Test My API</button>
          <button className="control-button" onClick={() => setGoal('explore_my_api')}>Explore My API</button>
        </div>
        {goal && (
          <p style={{ marginTop: '0.75rem', color: '#6c757d' }}>
            Selected goal: {goal.replace(/_/g, ' ')}
          </p>
        )}
      </div>

      <div className="workflow-list">
        {outcomes.map((step) => (
          <div key={step.id} className="workflow-step">
            <span className={`step-indicator ${step.completed ? 'completed' : 'pending'}`}>
              {step.completed ? '✓' : '○'}
            </span>
            <div className="step-content">
              <div className="step-title">{step.label}</div>
            </div>
          </div>
        ))}
      </div>

      <p style={{ marginTop: '1rem', color: '#6c757d' }}>
        Setup progress: {completed} of {total} complete
      </p>
      {complete && <p style={{ marginTop: '0.5rem', color: '#16a34a' }}>Your API workspace is ready.</p>}
    </div>
  );
}

export default Workflows;
