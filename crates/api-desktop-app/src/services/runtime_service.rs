//! Runtime service for mock server control.
//!
//! This service handles:
//! - Mock runtime lifecycle (start/stop/restart)
//! - Runtime state management
//! - Runtime event streaming
//! - Runtime metrics

use std::sync::Arc;

use api_runtime_events::RuntimeEvent;
use serde::{Deserialize, Serialize};

use crate::RuntimeStatus;
use crate::state::DesktopStateManager;

use super::{ServiceError, ServiceResult};

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub port: u16,
    pub profile_id: Option<String>,
    pub auto_start: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            port: 4010,
            profile_id: None,
            auto_start: false,
        }
    }
}

/// Runtime status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatusInfo {
    pub status: RuntimeStatus,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub uptime_seconds: Option<u64>,
    pub metrics: RuntimeMetricsInfo,
}

/// Runtime metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeMetricsInfo {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub validation_failures: u64,
    pub scenario_matches: u64,
    pub average_duration_ms: f64,
}

/// Runtime event (safe for frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEventInfo {
    pub event_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub details: Option<String>,
}

/// Runtime state export/import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStateSnapshot {
    pub scenarios: Vec<serde_json::Value>,
    pub resources: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Runtime service implementation
pub struct RuntimeService;

impl RuntimeService {
    /// Get runtime status
    pub async fn get_status(state: &Arc<DesktopStateManager>) -> ServiceResult<RuntimeStatusInfo> {
        let runtime = state.runtime.read().await;

        let metrics = RuntimeMetricsInfo {
            total_requests: runtime.requests,
            successful_requests: runtime.requests.saturating_sub(runtime.validation_failures),
            failed_requests: runtime.validation_failures,
            validation_failures: runtime.validation_failures,
            scenario_matches: 0,
            average_duration_ms: 0.0,
        };

        let port = runtime
            .address
            .as_ref()
            .and_then(|a| a.rsplit(':').next())
            .and_then(|p| p.parse().ok());

        Ok(RuntimeStatusInfo {
            status: runtime.status,
            address: runtime.address.clone(),
            port,
            uptime_seconds: None, // Would need start time tracking
            metrics,
        })
    }

    /// Start the mock runtime
    pub async fn start(
        state: &Arc<DesktopStateManager>,
        config: RuntimeConfig,
    ) -> ServiceResult<RuntimeStatusInfo> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // Set to starting state
        state.set_runtime_state(RuntimeStatus::Starting, None).await;

        // In production, this would:
        // 1. Load the contract
        // 2. Start the mock runtime server
        // 3. Wait for port binding confirmation

        let address = format!("http://localhost:{}", config.port);

        // Simulate successful start
        state
            .set_runtime_state(RuntimeStatus::Running, Some(address))
            .await;
        state.update_runtime_metrics(0, 0).await;

        Self::get_status(state).await
    }

    /// Stop the mock runtime
    pub async fn stop(state: &Arc<DesktopStateManager>) -> ServiceResult<RuntimeStatusInfo> {
        let current = state.runtime.read().await;

        if current.status == RuntimeStatus::Stopped {
            return Self::get_status(state).await;
        }
        drop(current);

        // Set to stopping state
        state.set_runtime_state(RuntimeStatus::Stopping, None).await;

        // In production, this would gracefully stop the runtime

        state.set_runtime_state(RuntimeStatus::Stopped, None).await;

        Self::get_status(state).await
    }

    /// Restart the mock runtime
    pub async fn restart(
        state: &Arc<DesktopStateManager>,
        config: RuntimeConfig,
    ) -> ServiceResult<RuntimeStatusInfo> {
        Self::stop(state).await?;
        Self::start(state, config).await
    }

    /// Reset runtime state (clear mock data)
    pub async fn reset_state(state: &Arc<DesktopStateManager>) -> ServiceResult<RuntimeStatusInfo> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // Reset metrics
        state.update_runtime_metrics(0, 0).await;

        // In production, this would:
        // 1. Clear stateful mock data
        // 2. Reset scenario match counts
        // 3. Clear request history

        Self::get_status(state).await
    }

    /// Export runtime state
    pub async fn export_state(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<RuntimeStateSnapshot> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would export actual state
        Ok(RuntimeStateSnapshot {
            scenarios: Vec::new(),
            resources: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        })
    }

    /// Import runtime state
    pub async fn import_state(
        state: &Arc<DesktopStateManager>,
        _snapshot: RuntimeStateSnapshot,
    ) -> ServiceResult<()> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would import the state
        Ok(())
    }

    /// Get recent runtime events
    pub async fn get_events(
        state: &Arc<DesktopStateManager>,
        _limit: usize,
    ) -> ServiceResult<Vec<RuntimeEventInfo>> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // In production, this would return actual events
        Ok(Vec::new())
    }

    /// Get runtime metrics
    pub async fn get_metrics(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<RuntimeMetricsInfo> {
        let runtime = state.runtime.read().await;

        Ok(RuntimeMetricsInfo {
            total_requests: runtime.requests,
            successful_requests: runtime.requests.saturating_sub(runtime.validation_failures),
            failed_requests: runtime.validation_failures,
            validation_failures: runtime.validation_failures,
            scenario_matches: 0,
            average_duration_ms: 42.0, // Placeholder
        })
    }

    /// Check if runtime port is available
    pub async fn check_port_available(_port: u16) -> ServiceResult<bool> {
        // In production, this would check actual port availability
        Ok(true)
    }

    /// Convert core runtime event to safe info
    pub fn event_to_info(event: &RuntimeEvent) -> RuntimeEventInfo {
        match event {
            RuntimeEvent::RequestReceived(e) => RuntimeEventInfo {
                event_id: e.event_id.clone(),
                timestamp: e.timestamp,
                event_type: "request_received".to_string(),
                method: Some(e.method.as_str().to_uppercase()),
                path: Some(e.path.clone()),
                status: None,
                duration_ms: None,
                details: None,
            },
            RuntimeEvent::ResponseSent(e) => RuntimeEventInfo {
                event_id: e.event_id.clone(),
                timestamp: e.timestamp,
                event_type: "response_sent".to_string(),
                method: None,
                path: None,
                status: Some(e.status),
                duration_ms: Some(e.duration_ms),
                details: None,
            },
            RuntimeEvent::ValidationFailed(e) => RuntimeEventInfo {
                event_id: e.event_id.clone(),
                timestamp: e.timestamp,
                event_type: "validation_failed".to_string(),
                method: None,
                path: None,
                status: None,
                duration_ms: None,
                details: Some(format!("{} violations", e.violations.len())),
            },
            RuntimeEvent::ScenarioMatched(e) => RuntimeEventInfo {
                event_id: e.event_id.clone(),
                timestamp: e.timestamp,
                event_type: "scenario_matched".to_string(),
                method: None,
                path: None,
                status: None,
                duration_ms: None,
                details: Some(e.scenario_name.clone()),
            },
            _ => RuntimeEventInfo {
                event_id: event.event_id().to_string(),
                timestamp: event.timestamp(),
                event_type: "other".to_string(),
                method: None,
                path: None,
                status: None,
                duration_ms: None,
                details: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_runtime_lifecycle() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        // Start
        let status = RuntimeService::start(&state, RuntimeConfig::default())
            .await
            .unwrap();
        assert_eq!(status.status, RuntimeStatus::Running);
        assert!(status.address.is_some());

        // Stop
        let status = RuntimeService::stop(&state).await.unwrap();
        assert_eq!(status.status, RuntimeStatus::Stopped);
    }

    #[tokio::test]
    async fn test_runtime_requires_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let result = RuntimeService::start(&state, RuntimeConfig::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reset_state() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        // Add some metrics
        state.update_runtime_metrics(100, 5).await;

        // Reset
        RuntimeService::reset_state(&state).await.unwrap();

        let runtime = state.runtime.read().await;
        assert_eq!(runtime.requests, 0);
        assert_eq!(runtime.validation_failures, 0);
    }
}
