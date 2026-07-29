use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};
use uuid::Uuid;

pub type JourneyId = String;
pub type ProjectId = String;

const REQUIRED_OUTCOMES: usize = 7;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JourneyRecommendation {
    pub id: String,
    pub title: String,
    pub description: String,
    pub stage: JourneyStage,
    pub primary_action: String,
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
        let mut state = Self {
            version: 1,
            project_id: project_id.into(),
            selected_goal: None,
            completed_outcomes: Vec::new(),
            current_recommendation: None,
            deferred_actions: Vec::new(),
            completed_at: None,
        };
        state.refresh_recommendation();
        state
    }

    pub fn progress(&self) -> JourneyProgress {
        JourneyProgress {
            completed: self.completed_outcomes.len(),
            total: REQUIRED_OUTCOMES,
        }
    }

    pub fn stage(&self) -> JourneyStage {
        if self.is_complete() {
            JourneyStage::Completion
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::RepositoryConnected)
        {
            JourneyStage::RepositoryConnection
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::ApiDiscovered)
        {
            JourneyStage::Discovery
        } else if self.selected_goal.is_none() {
            JourneyStage::GoalSelection
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::FirstRequest)
        {
            JourneyStage::FirstRequest
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::EnvironmentReady)
        {
            JourneyStage::EnvironmentSetup
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::ReusableRequest)
        {
            JourneyStage::ApiReview
        } else if !self.completed_outcomes.contains(&JourneyOutcome::MockReady) {
            JourneyStage::MockSetup
        } else if !self
            .completed_outcomes
            .contains(&JourneyOutcome::TestComplete)
        {
            JourneyStage::TestSetup
        } else {
            JourneyStage::Completion
        }
    }

    pub fn as_journey(&self) -> CustomerJourney {
        CustomerJourney {
            id: format!("journey_{}", Uuid::new_v4().simple()),
            project_id: self.project_id.clone(),
            goal: self
                .selected_goal
                .clone()
                .unwrap_or(CustomerGoal::UnderstandMyApi),
            stage: self.stage(),
            progress: self.progress(),
            recommendations: self
                .current_recommendation
                .clone()
                .into_iter()
                .collect::<Vec<_>>(),
            deferred_actions: self.deferred_actions.clone(),
        }
    }

    pub fn set_goal(&mut self, goal: CustomerGoal) {
        self.selected_goal = Some(goal);
        self.refresh_recommendation();
    }

    pub fn defer_action(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        reason: Option<String>,
    ) {
        self.deferred_actions.push(DeferredAction {
            id: id.into(),
            title: title.into(),
            reason,
        });
    }

    pub fn complete_outcome(&mut self, outcome: JourneyOutcome) {
        let mut uniq: BTreeSet<JourneyOutcome> = self.completed_outcomes.drain(..).collect();
        uniq.insert(outcome);
        self.completed_outcomes = uniq.into_iter().collect();

        if self.is_complete() {
            self.completed_at = Some(Utc::now());
        }

        self.refresh_recommendation();
    }

    pub fn is_complete(&self) -> bool {
        self.completed_outcomes.len() >= REQUIRED_OUTCOMES
    }

    fn refresh_recommendation(&mut self) {
        self.current_recommendation = self.next_recommendation();
    }

    fn next_recommendation(&self) -> Option<JourneyRecommendation> {
        if self.is_complete() {
            return None;
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::RepositoryConnected)
        {
            return Some(JourneyRecommendation {
                id: "connect-repository".to_string(),
                title: "Connect your repository".to_string(),
                description: "Choose the repository that contains your API source code."
                    .to_string(),
                stage: JourneyStage::RepositoryConnection,
                primary_action: "Connect Repository".to_string(),
            });
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::ApiDiscovered)
        {
            return Some(JourneyRecommendation {
                id: "discover-api".to_string(),
                title: "Discover your API".to_string(),
                description: "Review detected endpoints, models, and authentication patterns."
                    .to_string(),
                stage: JourneyStage::Discovery,
                primary_action: "Review Your API".to_string(),
            });
        }

        if self.selected_goal.is_none() {
            return Some(JourneyRecommendation {
                id: "choose-goal".to_string(),
                title: "Choose your first goal".to_string(),
                description: "Select whether you want to try an endpoint, create a mock API, or run tests first.".to_string(),
                stage: JourneyStage::GoalSelection,
                primary_action: "Choose Goal".to_string(),
            });
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::FirstRequest)
        {
            return Some(JourneyRecommendation {
                id: "first-request".to_string(),
                title: "Send your first request".to_string(),
                description: "Try a safe endpoint to reach your first successful API response."
                    .to_string(),
                stage: JourneyStage::FirstRequest,
                primary_action: "Send Request".to_string(),
            });
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::EnvironmentReady)
        {
            return Some(JourneyRecommendation {
                id: "setup-environment".to_string(),
                title: "Set up an environment".to_string(),
                description:
                    "Use a mock, development, or staging environment for repeatable requests."
                        .to_string(),
                stage: JourneyStage::EnvironmentSetup,
                primary_action: "Configure Environment".to_string(),
            });
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::ReusableRequest)
        {
            return Some(JourneyRecommendation {
                id: "save-request".to_string(),
                title: "Save a reusable request".to_string(),
                description: "Save your successful request to run it again or use it in tests."
                    .to_string(),
                stage: JourneyStage::ApiReview,
                primary_action: "Save Request".to_string(),
            });
        }

        if !self.completed_outcomes.contains(&JourneyOutcome::MockReady) {
            return Some(JourneyRecommendation {
                id: "start-mock".to_string(),
                title: "Start your mock API".to_string(),
                description: "Run your API contract locally for safe, deterministic testing."
                    .to_string(),
                stage: JourneyStage::MockSetup,
                primary_action: "Start Mock API".to_string(),
            });
        }

        if !self
            .completed_outcomes
            .contains(&JourneyOutcome::TestComplete)
        {
            return Some(JourneyRecommendation {
                id: "run-test".to_string(),
                title: "Run your first test".to_string(),
                description: "Validate endpoint behavior with a repeatable API test.".to_string(),
                stage: JourneyStage::TestSetup,
                primary_action: "Run Test".to_string(),
            });
        }

        None
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

pub fn save_customer_journey_state(
    root: &Path,
    state: &CustomerJourneyState,
) -> anyhow::Result<()> {
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
        state.set_goal(CustomerGoal::TryMyApi);
        state.complete_outcome(JourneyOutcome::RepositoryConnected);

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

    #[test]
    fn recommendation_progresses_by_outcome() {
        let mut state = CustomerJourneyState::new("proj");
        assert_eq!(
            state.current_recommendation.as_ref().map(|r| r.id.as_str()),
            Some("connect-repository")
        );

        state.complete_outcome(JourneyOutcome::RepositoryConnected);
        assert_eq!(
            state.current_recommendation.as_ref().map(|r| r.id.as_str()),
            Some("discover-api")
        );

        state.complete_outcome(JourneyOutcome::ApiDiscovered);
        assert_eq!(
            state.current_recommendation.as_ref().map(|r| r.id.as_str()),
            Some("choose-goal")
        );

        state.set_goal(CustomerGoal::TryMyApi);
        assert_eq!(
            state.current_recommendation.as_ref().map(|r| r.id.as_str()),
            Some("first-request")
        );
    }

    #[test]
    fn completion_is_outcome_based() {
        let mut state = CustomerJourneyState::new("proj");
        state.complete_outcome(JourneyOutcome::RepositoryConnected);
        state.complete_outcome(JourneyOutcome::ApiDiscovered);
        state.complete_outcome(JourneyOutcome::FirstRequest);
        state.complete_outcome(JourneyOutcome::EnvironmentReady);
        state.complete_outcome(JourneyOutcome::ReusableRequest);
        state.complete_outcome(JourneyOutcome::MockReady);
        state.complete_outcome(JourneyOutcome::TestComplete);

        assert!(state.is_complete());
        assert!(state.completed_at.is_some());
        assert!(state.current_recommendation.is_none());
        assert_eq!(state.stage(), JourneyStage::Completion);
    }
}
