use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub type JourneyId = String;
pub type ProjectId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CustomerGoal {
    UnderstandMyApi,
    TryMyApi,
    CreateMockApi,
    TestMyApi,
    ExploreEverything,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JourneyStage {
    Welcome,
    RepositoryConnection,
    Discovery,
    ApiReview,
    GoalSelection,
    FirstRequest,
    EnvironmentSetup,
    AuthenticationSetup,
    MockSetup,
    TestSetup,
    Completion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JourneyOutcome {
    RepositoryConnected,
    ApiDiscovered,
    FirstRequest,
    EnvironmentReady,
    ReusableRequest,
    MockReady,
    TestComplete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JourneyProgress {
    pub completed_outcomes: Vec<JourneyOutcome>,
    pub total_outcomes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JourneyRecommendation {
    pub id: String,
    pub title: String,
    pub description: String,
    pub stage: JourneyStage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeferredAction {
    pub id: String,
    pub title: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerJourney {
    pub id: JourneyId,
    pub project_id: ProjectId,
    pub goal: CustomerGoal,
    pub stage: JourneyStage,
    pub progress: JourneyProgress,
    pub recommendations: Vec<JourneyRecommendation>,
    pub deferred_actions: Vec<DeferredAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerJourneyState {
    pub version: u32,
    pub project_id: ProjectId,
    pub selected_goal: Option<CustomerGoal>,
    pub completed_outcomes: Vec<JourneyOutcome>,
    pub current_recommendation: Option<JourneyRecommendation>,
    pub deferred_actions: Vec<DeferredAction>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl CustomerJourneyState {
    pub fn new(project_id: impl Into<ProjectId>) -> Self {
        Self {
            version: 1,
            project_id: project_id.into(),
            selected_goal: None,
            completed_outcomes: Vec::new(),
            current_recommendation: None,
            deferred_actions: Vec::new(),
            completed_at: None,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.completed_outcomes.len() >= 7
    }
}

pub fn customer_journey_file(root: &Path) -> std::path::PathBuf {
    root.join(".repo-api/customer-journey.json")
}

pub fn load_customer_journey_state(root: &Path) -> anyhow::Result<Option<CustomerJourneyState>> {
    let file = customer_journey_file(root);
    if !file.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(file)?)?))
}

pub fn save_customer_journey_state(root: &Path, state: &CustomerJourneyState) -> anyhow::Result<()> {
    let file = customer_journey_file(root);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

pub fn load_or_initialize_customer_journey_state(
    root: &Path,
    project_id: impl Into<ProjectId>,
) -> anyhow::Result<CustomerJourneyState> {
    if let Some(existing) = load_customer_journey_state(root)? {
        return Ok(existing);
    }

    let state = CustomerJourneyState::new(project_id);
    save_customer_journey_state(root, &state)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_state_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut state = CustomerJourneyState::new("proj_123");
        state.selected_goal = Some(CustomerGoal::TryMyApi);
        state.completed_outcomes = vec![JourneyOutcome::RepositoryConnected];

        save_customer_journey_state(dir.path(), &state).expect("save state");
        let loaded = load_customer_journey_state(dir.path())
            .expect("load state")
            .expect("state exists");

        assert_eq!(loaded.project_id, "proj_123");
        assert_eq!(loaded.selected_goal, Some(CustomerGoal::TryMyApi));
        assert_eq!(loaded.completed_outcomes.len(), 1);
    }

    #[test]
    fn load_or_initialize_creates_default_state_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = load_or_initialize_customer_journey_state(dir.path(), "proj_abc")
            .expect("initialize state");

        assert_eq!(state.project_id, "proj_abc");
        assert_eq!(state.version, 1);
        assert!(customer_journey_file(dir.path()).exists());
    }
}
