//! Workflow service for guided workflow management.
//!
//! This service handles:
//! - Workflow listing and retrieval
//! - Event-driven step completion
//! - Workflow recovery on restart
//! - Progress tracking

use std::sync::Arc;

use api_workflows::events::{WorkflowEvent, WorkflowEventKind};
use api_workflows::{WorkflowStep, create_workflow, starter_workflow_steps};
use serde::{Deserialize, Serialize};

use crate::WorkflowStepSummary;
use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

/// Workflow summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub current_step: Option<WorkflowStepSummary>,
    pub is_complete: bool,
}

/// Workflow detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDetail {
    pub id: String,
    pub name: String,
    pub steps: Vec<WorkflowStepInfo>,
    pub progress_percentage: f32,
}

/// Workflow step information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub completed: bool,
    pub action: String,
}

/// Workflow event result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEventResult {
    pub event_processed: bool,
    pub completed_steps: Vec<String>,
    pub workflow_id: Option<String>,
}

/// Workflow service implementation
pub struct WorkflowService;

impl WorkflowService {
    /// List all workflows
    pub async fn list(state: &Arc<DesktopStateManager>) -> ServiceResult<Vec<WorkflowSummary>> {
        let workflows = state.workflows.read().await;

        let summaries: Vec<WorkflowSummary> = workflows
            .iter()
            .map(|wf| {
                let completed = wf.steps.iter().filter(|s| s.completed).count();
                let current = wf.steps.iter().find(|s| !s.completed);

                WorkflowSummary {
                    id: wf.id.clone(),
                    name: wf.name.clone(),
                    completed_steps: completed,
                    total_steps: wf.steps.len(),
                    current_step: current.map(|s| WorkflowStepSummary {
                        id: s.id.clone(),
                        title: s.title.clone(),
                        completed: s.completed,
                    }),
                    is_complete: completed == wf.steps.len(),
                }
            })
            .collect();

        Ok(summaries)
    }

    /// Get workflow detail
    pub async fn get(
        state: &Arc<DesktopStateManager>,
        workflow_id: &str,
    ) -> ServiceResult<WorkflowDetail> {
        let workflows = state.workflows.read().await;

        let workflow = workflows
            .iter()
            .find(|wf| wf.id == workflow_id)
            .ok_or_else(|| ServiceError::not_found("Workflow"))?;

        let completed = workflow.steps.iter().filter(|s| s.completed).count();
        let progress = if workflow.steps.is_empty() {
            100.0
        } else {
            (completed as f32 / workflow.steps.len() as f32) * 100.0
        };

        Ok(WorkflowDetail {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
            steps: workflow
                .steps
                .iter()
                .map(|s| WorkflowStepInfo {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    description: s.description.clone(),
                    completed: s.completed,
                    action: Self::action_to_string(&s.action),
                })
                .collect(),
            progress_percentage: progress,
        })
    }

    /// Emit a workflow event (triggers automatic step completion)
    pub async fn emit_event(
        state: &Arc<DesktopStateManager>,
        event_kind: WorkflowEventKind,
    ) -> ServiceResult<WorkflowEventResult> {
        let engine_guard = state.workflow_engine.read().await;

        if let Some(engine) = engine_guard.as_ref() {
            let event = WorkflowEvent::new(event_kind);
            let results = engine.emit(event).await;

            let completed_steps: Vec<String> = results
                .iter()
                .filter(|r| r.success)
                .map(|r| r.step_id.clone())
                .collect();

            let workflow_id = results.first().map(|r| r.workflow_id.clone());

            // Update local workflow state
            drop(engine_guard);
            let mut workflows = state.workflows.write().await;
            for completed_step in &completed_steps {
                for workflow in workflows.iter_mut() {
                    if let Some(step) = workflow.steps.iter_mut().find(|s| s.id == *completed_step)
                    {
                        step.completed = true;
                    }
                }
            }

            Ok(WorkflowEventResult {
                event_processed: !completed_steps.is_empty(),
                completed_steps,
                workflow_id,
            })
        } else {
            Ok(WorkflowEventResult {
                event_processed: false,
                completed_steps: Vec::new(),
                workflow_id: None,
            })
        }
    }

    /// Get the current recommended step
    pub async fn get_recommended_step(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Option<WorkflowStepSummary>> {
        let workflows = state.workflows.read().await;

        // Find first incomplete step across all workflows
        for wf in workflows.iter() {
            if let Some(step) = wf.steps.iter().find(|s| !s.completed) {
                return Ok(Some(WorkflowStepSummary {
                    id: step.id.clone(),
                    title: step.title.clone(),
                    completed: step.completed,
                }));
            }
        }

        Ok(None)
    }

    /// Recover workflow state (called on restart)
    pub async fn recover_state(state: &Arc<DesktopStateManager>) -> ServiceResult<()> {
        let engine_guard = state.workflow_engine.read().await;

        if let Some(engine) = engine_guard.as_ref() {
            engine.recover_state().await;
        }

        Ok(())
    }

    /// Create a new workflow
    pub async fn create(
        state: &Arc<DesktopStateManager>,
        name: &str,
        steps: Vec<WorkflowStep>,
    ) -> ServiceResult<WorkflowSummary> {
        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let workflow = create_workflow(root, name, steps)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        // Add to local state
        {
            let mut workflows = state.workflows.write().await;
            workflows.push(workflow.clone());
        }

        // Register with engine
        {
            let engine_guard = state.workflow_engine.read().await;
            if let Some(engine) = engine_guard.as_ref() {
                engine.register_workflow(workflow.clone()).await;
            }
        }

        let completed = workflow.steps.iter().filter(|s| s.completed).count();

        Ok(WorkflowSummary {
            id: workflow.id,
            name: workflow.name,
            completed_steps: completed,
            total_steps: workflow.steps.len(),
            current_step: workflow.steps.iter().find(|s| !s.completed).map(|s| {
                WorkflowStepSummary {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    completed: s.completed,
                }
            }),
            is_complete: completed == workflow.steps.len(),
        })
    }

    /// Create the default starter workflow
    pub async fn create_starter_workflow(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<WorkflowSummary> {
        Self::create(state, "Getting Started", starter_workflow_steps()).await
    }

    // Private helpers

    fn action_to_string(action: &api_workflows::WorkflowAction) -> String {
        match action {
            api_workflows::WorkflowAction::ConnectRepository => "connect_repository".to_string(),
            api_workflows::WorkflowAction::AnalyzeApi => "analyze_api".to_string(),
            api_workflows::WorkflowAction::ReviewEndpoints => "review_endpoints".to_string(),
            api_workflows::WorkflowAction::RunFirstRequest => "run_first_request".to_string(),
            api_workflows::WorkflowAction::CreateMockScenario => "create_mock_scenario".to_string(),
            api_workflows::WorkflowAction::RunTestSuite => "run_test_suite".to_string(),
            api_workflows::WorkflowAction::ConfigureEnvironment => {
                "configure_environment".to_string()
            }
            api_workflows::WorkflowAction::Custom { action } => action.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_workflows::Workflow;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_workflow_list() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let summaries = WorkflowService::list(&state).await.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn test_recommended_step() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());

        // Create a workflow with some incomplete steps
        let mut workflows = state.workflows.write().await;
        workflows.push(Workflow {
            id: "wf-1".to_string(),
            name: "Test".to_string(),
            steps: vec![
                WorkflowStep {
                    id: "step-1".to_string(),
                    title: "First Step".to_string(),
                    description: "Do this first".to_string(),
                    action: api_workflows::WorkflowAction::ConnectRepository,
                    completed: true,
                },
                WorkflowStep {
                    id: "step-2".to_string(),
                    title: "Second Step".to_string(),
                    description: "Do this second".to_string(),
                    action: api_workflows::WorkflowAction::AnalyzeApi,
                    completed: false,
                },
            ],
            created_at: chrono::Utc::now(),
        });
        drop(workflows);

        let recommended = WorkflowService::get_recommended_step(&state).await.unwrap();
        assert!(recommended.is_some());
        assert_eq!(recommended.unwrap().id, "step-2");
    }

    #[tokio::test]
    async fn test_workflow_detail() {
        let app_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        // Add workflow
        let mut workflows = state.workflows.write().await;
        workflows.push(Workflow {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            steps: starter_workflow_steps(),
            created_at: chrono::Utc::now(),
        });
        drop(workflows);

        let detail = WorkflowService::get(&state, "wf-1").await.unwrap();
        assert_eq!(detail.name, "Test Workflow");
        assert_eq!(detail.steps.len(), 6);
    }
}
