use serde::{Deserialize, Serialize};

use api_customer_journey::{CustomerGoal, CustomerJourney, DeferredAction, JourneyOutcome};

use crate::services::CustomerJourneyService;
use crate::state::DesktopStateManager;

use super::{AppState, CommandResult};

#[derive(Debug, Deserialize)]
pub struct SelectGoalRequest {
    pub goal: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteOutcomeRequest {
    pub outcome: String,
}

#[derive(Debug, Deserialize)]
pub struct DeferActionRequest {
    pub id: String,
    pub title: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JourneyProgressResponse {
    pub completed: usize,
    pub total: usize,
    pub is_complete: bool,
}

fn parse_goal(value: &str) -> Option<CustomerGoal> {
    match value {
        "understand_my_api" => Some(CustomerGoal::UnderstandMyApi),
        "try_my_api" => Some(CustomerGoal::TryMyApi),
        "create_mock_api" => Some(CustomerGoal::CreateMockApi),
        "test_my_api" => Some(CustomerGoal::TestMyApi),
        "explore_everything" => Some(CustomerGoal::ExploreEverything),
        _ => None,
    }
}

fn parse_outcome(value: &str) -> Option<JourneyOutcome> {
    match value {
        "repository_connected" => Some(JourneyOutcome::RepositoryConnected),
        "api_discovered" => Some(JourneyOutcome::ApiDiscovered),
        "first_request" => Some(JourneyOutcome::FirstRequest),
        "environment_ready" => Some(JourneyOutcome::EnvironmentReady),
        "reusable_request" => Some(JourneyOutcome::ReusableRequest),
        "mock_ready" => Some(JourneyOutcome::MockReady),
        "test_complete" => Some(JourneyOutcome::TestComplete),
        _ => None,
    }
}

#[cfg(feature = "tauri")]
fn state_handle(state: &AppState<'_>) -> std::sync::Arc<DesktopStateManager> {
    state.inner().clone()
}

#[cfg(not(feature = "tauri"))]
fn state_handle(state: &AppState<'_>) -> std::sync::Arc<DesktopStateManager> {
    state.clone()
}

pub async fn journey_get(state: AppState<'_>) -> CommandResult<CustomerJourney> {
    let state = state_handle(&state);
    match CustomerJourneyService::get(&state).await {
        Ok(journey) => CommandResult::ok(journey),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

pub async fn journey_select_goal(
    state: AppState<'_>,
    request: SelectGoalRequest,
) -> CommandResult<CustomerJourney> {
    let Some(goal) = parse_goal(&request.goal) else {
        return CommandResult::validation_error("Unknown customer goal");
    };

    let state = state_handle(&state);
    match CustomerJourneyService::select_goal(&state, goal).await {
        Ok(journey) => CommandResult::ok(journey),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

pub async fn journey_complete_outcome(
    state: AppState<'_>,
    request: CompleteOutcomeRequest,
) -> CommandResult<CustomerJourney> {
    let Some(outcome) = parse_outcome(&request.outcome) else {
        return CommandResult::validation_error("Unknown journey outcome");
    };

    let state = state_handle(&state);
    match CustomerJourneyService::complete_outcome(&state, outcome).await {
        Ok(journey) => CommandResult::ok(journey),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

pub async fn journey_defer_action(
    state: AppState<'_>,
    request: DeferActionRequest,
) -> CommandResult<CustomerJourney> {
    let action = DeferredAction {
        id: request.id,
        title: request.title,
        reason: request.reason,
    };

    let state = state_handle(&state);
    match CustomerJourneyService::defer_action(&state, action).await {
        Ok(journey) => CommandResult::ok(journey),
        Err(e) => CommandResult::error(e.to_string()),
    }
}

pub async fn journey_progress(state: AppState<'_>) -> CommandResult<JourneyProgressResponse> {
    let state = state_handle(&state);
    match CustomerJourneyService::get_state(&state).await {
        Ok(state) => {
            let progress = state.progress();
            CommandResult::ok(JourneyProgressResponse {
                completed: progress.completed,
                total: progress.total,
                is_complete: state.is_complete(),
            })
        }
        Err(e) => CommandResult::error(e.to_string()),
    }
}
