use api_projects::{ApiProject, create_project, load_project};
use api_workflows::{create_workflow, list_workflows, starter_workflow_steps};
use std::path::Path;

pub struct DesktopLaunchSummary {
    pub project: ApiProject,
    pub endpoint_count: usize,
    pub schema_count: usize,
    pub workflow_count: usize,
}

pub fn launch_or_open(
    root: &Path,
    project_name: Option<&str>,
) -> anyhow::Result<DesktopLaunchSummary> {
    api_storage::init_layout(root)?;
    let project = if let Some(existing) = load_project(root)? {
        existing
    } else {
        create_project(root, project_name.unwrap_or("Repository API Project"))?
    };
    let mut journey =
        api_customer_journey::load_or_initialize_customer_journey_state(root, project.id.clone())?;
    journey.complete_outcome(api_customer_journey::JourneyOutcome::RepositoryConnected);

    let workflows = list_workflows(root)?;
    if workflows.is_empty() {
        let _ = create_workflow(root, "Getting Started", starter_workflow_steps())?;
    }

    let vault = api_vault::VaultStore::open(root)?;
    let _ = vault.list_entries()?;

    let (endpoint_count, schema_count) = match api_storage::load_effective_contract(root) {
        Ok(contract) => (contract.endpoints.len(), contract.schemas.schemas.len()),
        Err(_) => (0, 0),
    };
    if endpoint_count > 0 {
        journey.complete_outcome(api_customer_journey::JourneyOutcome::ApiDiscovered);
    }
    api_customer_journey::save_customer_journey_state(root, &journey)?;

    let workflow_count = list_workflows(root)?.len();

    Ok(DesktopLaunchSummary {
        project,
        endpoint_count,
        schema_count,
        workflow_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_creates_project_and_workflow() {
        let dir = tempfile::tempdir().expect("tempdir");
        let summary = launch_or_open(dir.path(), Some("FieldFlow API")).expect("launch");
        assert_eq!(summary.project.name, "FieldFlow API");
        assert!(summary.workflow_count >= 1);
        assert!(dir.path().join(".repo-api/customer-journey.json").exists());
        let journey = api_customer_journey::load_customer_journey_state(dir.path())
            .expect("load journey")
            .expect("journey exists");
        assert!(
            journey
                .completed_outcomes
                .contains(&api_customer_journey::JourneyOutcome::RepositoryConnected)
        );
    }
}
