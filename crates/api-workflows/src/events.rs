//! Workflow event-driven completion system.
//!
//! This module provides automatic workflow step completion based on
//! application events, eliminating the need for manual completion.

use crate::{Workflow, WorkflowStep, save_workflow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Workflow event kinds that can trigger step completion
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventKind {
    /// Project was opened
    ProjectOpened,
    /// Repository was scanned for API discovery
    RepositoryScanned,
    /// An endpoint was selected in the explorer
    EndpointSelected,
    /// A request was successfully executed
    RequestExecuted,
    /// An environment was selected
    EnvironmentSelected,
    /// A vault credential was linked to an environment
    VaultCredentialLinked,
    /// Authentication was configured
    AuthenticationConfigured,
    /// A mock scenario was created
    ScenarioCreated,
    /// Mock runtime was started
    MockRuntimeStarted,
    /// Test suite was executed
    TestSuiteExecuted,
    /// Contract change was reviewed
    ContractChangeReviewed,
}

/// Workflow event with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub kind: WorkflowEventKind,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub context: HashMap<String, String>,
}

impl WorkflowEvent {
    pub fn new(kind: WorkflowEventKind) -> Self {
        Self {
            kind,
            timestamp: Utc::now(),
            context: HashMap::new(),
        }
    }

    pub fn with_context(mut self, key: &str, value: &str) -> Self {
        self.context.insert(key.to_string(), value.to_string());
        self
    }
}

/// Completion predicate for evaluating whether a step should complete
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompletionPredicate {
    /// Always complete when the event fires
    Always,
    /// Complete if context contains a specific key-value pair
    ContextEquals { key: String, value: String },
    /// Complete if context contains a specific key
    ContextExists { key: String },
    /// Complete if a minimum count threshold is met
    CountAtLeast { key: String, min: usize },
}

impl CompletionPredicate {
    pub fn evaluate(&self, event: &WorkflowEvent) -> bool {
        match self {
            Self::Always => true,
            Self::ContextEquals { key, value } => {
                event.context.get(key).map(|v| v == value).unwrap_or(false)
            }
            Self::ContextExists { key } => event.context.contains_key(key),
            Self::CountAtLeast { key, min } => event
                .context
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
                .map(|count| count >= *min)
                .unwrap_or(false),
        }
    }
}

/// Rule binding an event to a workflow step completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowCompletionRule {
    pub step_id: String,
    pub event: WorkflowEventKind,
    pub predicate: CompletionPredicate,
}

/// Default completion rules for the starter workflow
pub fn default_completion_rules() -> Vec<WorkflowCompletionRule> {
    vec![
        WorkflowCompletionRule {
            step_id: "connect-repository".to_string(),
            event: WorkflowEventKind::ProjectOpened,
            predicate: CompletionPredicate::Always,
        },
        WorkflowCompletionRule {
            step_id: "analyze-api".to_string(),
            event: WorkflowEventKind::RepositoryScanned,
            predicate: CompletionPredicate::Always,
        },
        WorkflowCompletionRule {
            step_id: "run-first-request".to_string(),
            event: WorkflowEventKind::RequestExecuted,
            predicate: CompletionPredicate::Always,
        },
        WorkflowCompletionRule {
            step_id: "configure-authentication".to_string(),
            event: WorkflowEventKind::AuthenticationConfigured,
            predicate: CompletionPredicate::Always,
        },
        WorkflowCompletionRule {
            step_id: "create-mock-scenario".to_string(),
            event: WorkflowEventKind::ScenarioCreated,
            predicate: CompletionPredicate::Always,
        },
        WorkflowCompletionRule {
            step_id: "run-test-suite".to_string(),
            event: WorkflowEventKind::TestSuiteExecuted,
            predicate: CompletionPredicate::Always,
        },
    ]
}

/// Workflow completion engine that listens for events and completes steps
pub struct WorkflowCompletionEngine {
    root: std::path::PathBuf,
    rules: Vec<WorkflowCompletionRule>,
    workflows: Arc<RwLock<HashMap<String, Workflow>>>,
    event_sender: broadcast::Sender<WorkflowEvent>,
}

impl WorkflowCompletionEngine {
    /// Create a new completion engine
    pub fn new(root: &Path, rules: Vec<WorkflowCompletionRule>) -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            root: root.to_path_buf(),
            rules,
            workflows: Arc::new(RwLock::new(HashMap::new())),
            event_sender: sender,
        }
    }

    /// Create with default rules
    pub fn with_defaults(root: &Path) -> Self {
        Self::new(root, default_completion_rules())
    }

    /// Register a workflow to track
    pub async fn register_workflow(&self, workflow: Workflow) {
        let mut workflows = self.workflows.write().await;
        workflows.insert(workflow.id.clone(), workflow);
    }

    /// Get a workflow by ID
    pub async fn get_workflow(&self, workflow_id: &str) -> Option<Workflow> {
        let workflows = self.workflows.read().await;
        workflows.get(workflow_id).cloned()
    }

    /// Get all tracked workflows
    pub async fn get_workflows(&self) -> Vec<Workflow> {
        let workflows = self.workflows.read().await;
        workflows.values().cloned().collect()
    }

    /// Subscribe to workflow events
    pub fn subscribe(&self) -> broadcast::Receiver<WorkflowEvent> {
        self.event_sender.subscribe()
    }

    /// Emit an event and process completions
    pub async fn emit(&self, event: WorkflowEvent) -> Vec<CompletionResult> {
        let _ = self.event_sender.send(event.clone());
        self.process_event(&event).await
    }

    /// Process an event and complete matching workflow steps
    async fn process_event(&self, event: &WorkflowEvent) -> Vec<CompletionResult> {
        let mut results = Vec::new();
        let mut workflows = self.workflows.write().await;

        // Find rules that match this event
        let matching_rules: Vec<_> = self
            .rules
            .iter()
            .filter(|rule| rule.event == event.kind && rule.predicate.evaluate(event))
            .collect();

        // Apply to all workflows
        for workflow in workflows.values_mut() {
            for rule in &matching_rules {
                // Find the step
                if let Some(step) = workflow
                    .steps
                    .iter_mut()
                    .find(|s| s.id == rule.step_id && !s.completed)
                {
                    step.completed = true;

                    // Persist the change
                    if let Err(e) = save_workflow(&self.root, workflow) {
                        results.push(CompletionResult {
                            workflow_id: workflow.id.clone(),
                            step_id: rule.step_id.clone(),
                            success: false,
                            error: Some(e.to_string()),
                        });
                    } else {
                        results.push(CompletionResult {
                            workflow_id: workflow.id.clone(),
                            step_id: rule.step_id.clone(),
                            success: true,
                            error: None,
                        });
                    }
                }
            }
        }

        results
    }

    /// Get the current recommended step for a workflow
    pub async fn get_recommended_step(&self, workflow_id: &str) -> Option<WorkflowStep> {
        let workflows = self.workflows.read().await;
        workflows
            .get(workflow_id)
            .and_then(|workflow| workflow.steps.iter().find(|step| !step.completed).cloned())
    }

    /// Get workflow progress as (completed, total)
    pub async fn get_progress(&self, workflow_id: &str) -> Option<(usize, usize)> {
        let workflows = self.workflows.read().await;
        workflows.get(workflow_id).map(|workflow| {
            let completed = workflow.steps.iter().filter(|s| s.completed).count();
            (completed, workflow.steps.len())
        })
    }

    /// Reevaluate incomplete steps based on persisted evidence
    pub async fn recover_state(&self) {
        // This would check application state to determine if steps
        // should be marked as completed based on existing evidence
        // For example, if a contract exists, the analyze-api step is complete

        let mut workflows = self.workflows.write().await;

        for workflow in workflows.values_mut() {
            // Check for connect-repository (always complete if we have a project)
            if let Some(step) = workflow
                .steps
                .iter_mut()
                .find(|s| s.id == "connect-repository" && !s.completed)
            {
                // If we're running, the repository is connected
                step.completed = true;
            }

            // Check for analyze-api (complete if contract exists)
            let contract_path = self.root.join(".repo-api/contract/effective.json");
            if contract_path.exists()
                && let Some(step) = workflow
                    .steps
                    .iter_mut()
                    .find(|s| s.id == "analyze-api" && !s.completed)
            {
                step.completed = true;
            }

            // Save recovered state
            let _ = save_workflow(&self.root, workflow);
        }
    }

    /// Check if a step should be completed for a given event kind
    /// This is a simple predicate check without actually completing the step
    pub fn check_step(&self, step_id: &str, event_kind: &WorkflowEventKind) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.step_id == step_id && rule.event == *event_kind)
    }
}

/// Result of a step completion attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResult {
    pub workflow_id: String,
    pub step_id: String,
    pub success: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_workflow, starter_workflow_steps};

    #[tokio::test]
    async fn event_completes_matching_step() {
        let dir = tempfile::tempdir().unwrap();
        let workflow = create_workflow(dir.path(), "Test", starter_workflow_steps()).unwrap();

        let engine = WorkflowCompletionEngine::with_defaults(dir.path());
        engine.register_workflow(workflow.clone()).await;

        // Verify analyze-api starts incomplete
        let wf = engine.get_workflow(&workflow.id).await.unwrap();
        let step = wf.steps.iter().find(|s| s.id == "analyze-api").unwrap();
        assert!(!step.completed);

        // Emit event
        let event = WorkflowEvent::new(WorkflowEventKind::RepositoryScanned);
        let results = engine.emit(event).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Verify step is now complete
        let wf = engine.get_workflow(&workflow.id).await.unwrap();
        let step = wf.steps.iter().find(|s| s.id == "analyze-api").unwrap();
        assert!(step.completed);
    }

    #[tokio::test]
    async fn predicate_evaluates_context() {
        let predicate = CompletionPredicate::ContextEquals {
            key: "status".to_string(),
            value: "success".to_string(),
        };

        let event_match = WorkflowEvent::new(WorkflowEventKind::RequestExecuted)
            .with_context("status", "success");
        assert!(predicate.evaluate(&event_match));

        let event_no_match = WorkflowEvent::new(WorkflowEventKind::RequestExecuted)
            .with_context("status", "failure");
        assert!(!predicate.evaluate(&event_no_match));
    }

    #[tokio::test]
    async fn get_recommended_step() {
        let dir = tempfile::tempdir().unwrap();
        let workflow = create_workflow(dir.path(), "Test", starter_workflow_steps()).unwrap();

        let engine = WorkflowCompletionEngine::with_defaults(dir.path());
        engine.register_workflow(workflow.clone()).await;

        // First incomplete step should be analyze-api (connect-repository is auto-completed)
        let recommended = engine.get_recommended_step(&workflow.id).await.unwrap();
        assert_eq!(recommended.id, "analyze-api");
    }

    #[tokio::test]
    async fn get_progress() {
        let dir = tempfile::tempdir().unwrap();
        let workflow = create_workflow(dir.path(), "Test", starter_workflow_steps()).unwrap();

        let engine = WorkflowCompletionEngine::with_defaults(dir.path());
        engine.register_workflow(workflow.clone()).await;

        let (completed, total) = engine.get_progress(&workflow.id).await.unwrap();
        assert_eq!(completed, 1); // connect-repository is pre-completed
        assert_eq!(total, 6); // 6 steps total in starter workflow
    }
}
