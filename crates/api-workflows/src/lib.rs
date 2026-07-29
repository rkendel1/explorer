//! Guided workflow system for API development.
//!
//! This crate provides:
//! - Workflow definitions with steps
//! - Workflow persistence
//! - Event-driven step completion
//! - Workflow recovery on restart

pub mod events;

use chrono::{DateTime, Utc};
pub use events::{
    CompletionPredicate, CompletionResult, WorkflowCompletionEngine, WorkflowCompletionRule,
    WorkflowEvent, WorkflowEventKind, default_completion_rules,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub steps: Vec<WorkflowStep>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action: WorkflowAction,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowAction {
    ConnectRepository,
    AnalyzeApi,
    ReviewEndpoints,
    RunFirstRequest,
    CreateMockScenario,
    RunTestSuite,
    ConfigureEnvironment,
    Custom { action: String },
}

fn workflows_dir(root: &Path) -> std::path::PathBuf {
    root.join(".repo-api/workflows")
}

fn workflow_file(root: &Path, workflow_id: &str) -> std::path::PathBuf {
    workflows_dir(root).join(format!("{workflow_id}.json"))
}

pub fn create_workflow(
    root: &Path,
    name: impl Into<String>,
    steps: Vec<WorkflowStep>,
) -> anyhow::Result<Workflow> {
    let workflow = Workflow {
        id: format!("wf_{}", Uuid::new_v4().simple()),
        name: name.into(),
        steps,
        created_at: Utc::now(),
    };
    save_workflow(root, &workflow)?;
    Ok(workflow)
}

pub fn save_workflow(root: &Path, workflow: &Workflow) -> anyhow::Result<()> {
    fs::create_dir_all(workflows_dir(root))?;
    fs::write(
        workflow_file(root, &workflow.id),
        serde_json::to_vec_pretty(workflow)?,
    )?;
    Ok(())
}

pub fn list_workflows(root: &Path) -> anyhow::Result<Vec<Workflow>> {
    let dir = workflows_dir(root);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut workflows: Vec<Workflow> = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            workflows.push(serde_json::from_slice(&fs::read(entry.path())?)?);
        }
    }

    workflows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(workflows)
}

pub fn complete_step(root: &Path, workflow_id: &str, step_id: &str) -> anyhow::Result<Workflow> {
    let file = workflow_file(root, workflow_id);
    let mut workflow: Workflow = serde_json::from_slice(&fs::read(&file)?)?;
    let step = workflow
        .steps
        .iter_mut()
        .find(|step| step.id == step_id)
        .ok_or_else(|| anyhow::anyhow!("step '{step_id}' not found"))?;
    step.completed = true;
    fs::write(file, serde_json::to_vec_pretty(&workflow)?)?;
    Ok(workflow)
}

pub fn starter_workflow_steps() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep {
            id: "connect-repository".into(),
            title: "Connect Repository".into(),
            description: "Connect this repository as the API project source.".into(),
            action: WorkflowAction::ConnectRepository,
            completed: true,
        },
        WorkflowStep {
            id: "analyze-api".into(),
            title: "Analyze API".into(),
            description: "Scan routes, schemas, and evidence to build the contract.".into(),
            action: WorkflowAction::AnalyzeApi,
            completed: false,
        },
        WorkflowStep {
            id: "run-first-request".into(),
            title: "Run First Request".into(),
            description: "Execute a request against mock or configured environment.".into(),
            action: WorkflowAction::RunFirstRequest,
            completed: false,
        },
        WorkflowStep {
            id: "configure-authentication".into(),
            title: "Configure Authentication".into(),
            description: "Set up API key or bearer token authentication.".into(),
            action: WorkflowAction::ConfigureEnvironment,
            completed: false,
        },
        WorkflowStep {
            id: "create-mock-scenario".into(),
            title: "Create Mock Scenario".into(),
            description: "Create a custom mock response scenario.".into(),
            action: WorkflowAction::CreateMockScenario,
            completed: false,
        },
        WorkflowStep {
            id: "run-test-suite".into(),
            title: "Run Test Suite".into(),
            description: "Execute the API test suite and review results.".into(),
            action: WorkflowAction::RunTestSuite,
            completed: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_and_complete_workflow_step() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = create_workflow(dir.path(), "Getting Started", starter_workflow_steps())
            .expect("create workflow");

        let listed = list_workflows(dir.path()).expect("list workflows");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Getting Started");

        let updated =
            complete_step(dir.path(), &workflow.id, "analyze-api").expect("complete step");
        assert!(
            updated
                .steps
                .iter()
                .any(|s| s.id == "analyze-api" && s.completed)
        );
    }

    #[test]
    fn starter_workflow_has_all_steps() {
        let steps = starter_workflow_steps();
        assert_eq!(steps.len(), 6);
        assert!(steps.iter().any(|s| s.id == "connect-repository"));
        assert!(steps.iter().any(|s| s.id == "analyze-api"));
        assert!(steps.iter().any(|s| s.id == "run-first-request"));
        assert!(steps.iter().any(|s| s.id == "configure-authentication"));
        assert!(steps.iter().any(|s| s.id == "create-mock-scenario"));
        assert!(steps.iter().any(|s| s.id == "run-test-suite"));
    }
}
