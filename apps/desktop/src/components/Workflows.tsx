import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/errors';
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

type JourneyOutcome =
  | 'repository_connected'
  | 'api_discovered'
  | 'first_request'
  | 'environment_ready'
  | 'reusable_request'
  | 'mock_ready'
  | 'test_complete';

type JourneyState = {
  selected_goal: CustomerGoal | null;
  completed_outcomes: JourneyOutcome[];
  current_recommendation: {
    id: string;
    title: string;
    description: string;
    primary_action: string;
  } | null;
};

type EnvironmentConfig = {
  id: string;
  name: string;
  is_active: boolean;
};

type Outcome = {
  id: JourneyOutcome;
  label: string;
};

const ORDERED_OUTCOMES: Outcome[] = [
  { id: 'repository_connected', label: 'Connected your repository' },
  { id: 'api_discovered', label: 'Mapped your API' },
  { id: 'first_request', label: 'Sent a successful request' },
  { id: 'environment_ready', label: 'Configured an environment' },
  { id: 'reusable_request', label: 'Created a reusable request' },
  { id: 'mock_ready', label: 'Started a mock API' },
  { id: 'test_complete', label: 'Ran your first test' },
];

function Workflows({ project, onNavigate }: WorkflowsProps) {
  const [journeyState, setJourneyState] = useState<JourneyState | null>(null);
  const [goal, setGoal] = useState<CustomerGoal | null>(null);
  const [environments, setEnvironments] = useState<EnvironmentConfig[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);

  const loadJourneyState = async () => {
    setError(null);
    try {
      const state = await invoke<JourneyState>('journey_state');
      setJourneyState(state);
      setGoal(state.selected_goal);

      const envs = await invoke<EnvironmentConfig[]>('environment_list');
      setEnvironments(envs);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  useEffect(() => {
    loadJourneyState();
  }, [project.path]);

  const completeOutcome = async (outcome: JourneyOutcome) => {
    setError(null);
    setIsBusy(true);
    try {
      await invoke('journey_complete_outcome', {
        request: { outcome },
      });
      await loadJourneyState();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const selectGoal = async (nextGoal: CustomerGoal) => {
    setGoal(nextGoal);
    setError(null);
    setIsBusy(true);
    try {
      await invoke('journey_select_goal', {
        request: { goal: nextGoal },
      });
      await loadJourneyState();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const outcomes = useMemo(
    () =>
      ORDERED_OUTCOMES.map((step) => ({
        ...step,
        completed: journeyState?.completed_outcomes.includes(step.id) ?? false,
      })),
    [journeyState],
  );

  const completed = outcomes.filter((o) => o.completed).length;
  const total = outcomes.length;
  const complete = completed === total;

  const nextOutcome = outcomes.find((o) => !o.completed);

  const recommendation = (() => {
    if (journeyState?.current_recommendation) {
      const id = journeyState.current_recommendation.id;
      if (id === 'discover-api') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: async () => {
            if (!journeyState.completed_outcomes.includes('api_discovered')) {
              await completeOutcome('api_discovered');
            }
            onNavigate('explorer');
          },
        };
      }
      if (id === 'first-request') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: () => onNavigate('requests'),
        };
      }
      if (id === 'setup-environment') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: () => onNavigate('requests'),
        };
      }
      if (id === 'save-request') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: () => onNavigate('requests'),
        };
      }
      if (id === 'start-mock') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: () => onNavigate('runtime'),
        };
      }
      if (id === 'run-test') {
        return {
          title: journeyState.current_recommendation.title,
          description: journeyState.current_recommendation.description,
          actionLabel: journeyState.current_recommendation.primary_action,
          action: async () => {
            await invoke('test_prepare_onboarding');
            await invoke('test_run', {
              request: { suiteId: null, testIds: null },
            });
            onNavigate('tests');
          },
        };
      }
    }

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
          action: async () => {
            await completeOutcome('api_discovered');
            onNavigate('explorer');
          },
        };
      case 'first_request':
        return {
          title: 'Try your first request',
          description: 'Choose a safe endpoint and send your first API request.',
          actionLabel: 'Try An Endpoint',
          action: () => {
            if (!goal) {
              void selectGoal('try_endpoint');
            }
            onNavigate('requests');
          },
        };
      case 'environment_ready':
        return {
          title: 'Choose how to run your API',
          description: 'Use mock, development, or staging as your first runtime target.',
          actionLabel: 'Configure Environment',
          action: () => onNavigate('requests'),
        };
      case 'reusable_request':
        return {
          title: 'Make it repeatable',
          description: 'Save your working request so it can be re-used in tests.',
          actionLabel: 'Save Request',
          action: () => onNavigate('requests'),
        };
      case 'mock_ready':
        return {
          title: 'Run through a mock environment',
          description: 'Start a local mock API generated from your API contract.',
          actionLabel: 'Start Mock API',
          action: () => onNavigate('runtime'),
        };
      case 'test_complete':
        return {
          title: 'Validate it with tests',
          description: 'Run a first test to make behavior safe and repeatable.',
          actionLabel: 'Run First Test',
          action: async () => {
            await invoke('test_prepare_onboarding');
            await invoke('test_run', {
              request: { suiteId: null, testIds: null },
            });
            onNavigate('tests');
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

  const deferCurrentAction = async () => {
    if (!journeyState?.current_recommendation) {
      return;
    }

    setError(null);
    setIsBusy(true);
    try {
      await invoke('journey_defer_action', {
        request: {
          id: journeyState.current_recommendation.id,
          title: journeyState.current_recommendation.title,
          reason: 'postponed from workflows panel',
        },
      });
      await loadJourneyState();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setIsBusy(false);
    }
  };

  const ensureEnvironment = async (name: string, makeActive = true) => {
    setError(null);
    setIsBusy(true);
    try {
      const existing = environments.find((env) => env.id === name);
      if (!existing) {
        await invoke('environment_create', {
          request: { name },
        });
      }

      if (makeActive) {
        await invoke('environment_select', {
          request: { id: name },
        });
      }

      await completeOutcome('environment_ready');
      await loadJourneyState();
      onNavigate('requests');
    } catch (err) {
      setError(errorMessage(err));
      setIsBusy(false);
    }
  };

  const activeEnvironment = environments.find((env) => env.is_active);
  const environmentReady = journeyState?.completed_outcomes.includes('environment_ready');

  return (
    <div>
      <h2>Getting Started</h2>
      <p style={{ color: '#6c757d', marginBottom: '1rem' }}>
        Connect repository → discover API → first successful request → mock → tests
      </p>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>&times;</button>
        </div>
      )}

      <div className="runtime-card" style={{ marginBottom: '1rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>{recommendation.title}</h3>
        <p style={{ color: '#6c757d', marginBottom: '0.75rem' }}>{recommendation.description}</p>
        <button className="action-button" onClick={() => void recommendation.action()} disabled={isBusy}>
          {recommendation.actionLabel}
        </button>
        <button className="control-button" style={{ marginLeft: '0.5rem' }} onClick={() => void deferCurrentAction()} disabled={isBusy}>
          Do This Later
        </button>
      </div>

      <div className="runtime-card" style={{ marginBottom: '1rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>What Would You Like To Do?</h3>
        <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
          <button className="control-button" onClick={() => void selectGoal('try_endpoint')} disabled={isBusy}>Try An Endpoint</button>
          <button className="control-button" onClick={() => void selectGoal('create_mock_api')} disabled={isBusy}>Create A Mock API</button>
          <button className="control-button" onClick={() => void selectGoal('test_my_api')} disabled={isBusy}>Test My API</button>
          <button className="control-button" onClick={() => void selectGoal('explore_my_api')} disabled={isBusy}>Explore My API</button>
        </div>
        {goal && (
          <p style={{ marginTop: '0.75rem', color: '#6c757d' }}>
            Selected goal: {goal.replace(/_/g, ' ')}
          </p>
        )}
      </div>

      <div className="runtime-card" style={{ marginBottom: '1rem' }}>
        <h3 style={{ fontSize: '1rem', marginBottom: '0.5rem' }}>Environment Setup</h3>
        {!environmentReady ? (
          <>
            <p style={{ color: '#6c757d', marginBottom: '0.5rem' }}>
              Pick where requests should run. Start with Mock for safe onboarding.
            </p>
            <div className="env-guide-list">
              <div className="env-guide-item">
                <strong>Mock (Recommended)</strong>
                <span>Runs against local mock runtime. No real data changes.</span>
                <button className="control-button" onClick={() => void ensureEnvironment('mock')} disabled={isBusy}>
                  Use Mock
                </button>
              </div>
              <div className="env-guide-item">
                <strong>Development</strong>
                <span>Use this when you have a dev backend URL ready.</span>
                <button className="control-button" onClick={() => void ensureEnvironment('development')} disabled={isBusy}>
                  Create Dev Starter
                </button>
              </div>
              <div className="env-guide-item">
                <strong>Staging</strong>
                <span>Use this after validating request behavior in mock/dev.</span>
                <button className="control-button" onClick={() => void ensureEnvironment('staging')} disabled={isBusy}>
                  Create Staging Starter
                </button>
              </div>
            </div>
            <p className="env-help-copy" style={{ marginTop: '0.5rem' }}>
              After choosing one, we will mark this step complete and take you to Requests.
            </p>
          </>
        ) : (
          <>
            <p style={{ color: '#16a34a', marginBottom: '0.5rem' }}>
              Environment configured.
            </p>
            <p className="env-help-copy">
              Active: {activeEnvironment?.name ?? 'none'}.
              You can switch environments from your next request setup.
            </p>
          </>
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
