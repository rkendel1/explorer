use std::sync::Arc;

use api_customer_journey::{
    CustomerGoal, CustomerJourney, CustomerJourneyState, DeferredAction, JourneyOutcome,
};

use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

pub struct CustomerJourneyService;

impl CustomerJourneyService {
    pub async fn get(state: &Arc<DesktopStateManager>) -> ServiceResult<CustomerJourney> {
        let journey = state
            .get_or_initialize_customer_journey()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;
        Ok(journey.as_journey())
    }

    pub async fn get_state(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<CustomerJourneyState> {
        state
            .get_or_initialize_customer_journey()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))
    }

    pub async fn select_goal(
        state: &Arc<DesktopStateManager>,
        goal: CustomerGoal,
    ) -> ServiceResult<CustomerJourney> {
        let mut journey = state
            .get_or_initialize_customer_journey()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        journey.set_goal(goal);
        state
            .save_customer_journey(journey.clone())
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(journey.as_journey())
    }

    pub async fn complete_outcome(
        state: &Arc<DesktopStateManager>,
        outcome: JourneyOutcome,
    ) -> ServiceResult<CustomerJourney> {
        let mut journey = state
            .get_or_initialize_customer_journey()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        journey.complete_outcome(outcome);
        state
            .save_customer_journey(journey.clone())
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(journey.as_journey())
    }

    pub async fn defer_action(
        state: &Arc<DesktopStateManager>,
        action: DeferredAction,
    ) -> ServiceResult<CustomerJourney> {
        let mut journey = state
            .get_or_initialize_customer_journey()
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        journey.defer_action(action.id, action.title, action.reason);
        state
            .save_customer_journey(journey.clone())
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(journey.as_journey())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DesktopStateManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn service_updates_goal_and_outcomes() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let _ = state
            .open_project(project_dir.path().to_path_buf())
            .await
            .unwrap();

        let journey = CustomerJourneyService::select_goal(&state, CustomerGoal::TryMyApi)
            .await
            .unwrap();
        assert!(matches!(journey.goal, CustomerGoal::TryMyApi));

        let updated =
            CustomerJourneyService::complete_outcome(&state, JourneyOutcome::FirstRequest)
                .await
                .unwrap();
        assert_eq!(updated.progress.completed, 1);
    }
}
