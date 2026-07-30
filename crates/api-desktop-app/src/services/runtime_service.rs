//! Runtime service for mock server control.
//!
//! This service handles:
//! - Mock runtime lifecycle (start/stop/restart) - actually spawns/aborts a
//!   real `api_mock_runtime` server rather than flipping a status flag
//! - Runtime state management
//! - Runtime event streaming (buffered, not yet pushed live to the frontend)
//! - Runtime metrics

use std::net::SocketAddr;
use std::sync::Arc;

use api_runtime_events::{EventEmitter, RuntimeEvent};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::RuntimeStatus;
use crate::state::{DesktopStateManager, RunningMockServer};

use super::{CustomerJourneyService, ServiceError, ServiceResult};

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

/// Runtime state export/import.
///
/// `resources` is the live `api-mock-runtime` state payload returned by
/// `GET /__api/state/export` (including both `resources` and counters).
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
        let metrics = Self::compute_metrics(state).await;

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

    /// Start the mock runtime. Actually binds `config.port` and spawns a
    /// real `api_mock_runtime` server backed by the project's contract.
    pub async fn start(
        state: &Arc<DesktopStateManager>,
        config: RuntimeConfig,
    ) -> ServiceResult<RuntimeStatusInfo> {
        {
            let project = state.project.read().await;
            if project.is_none() {
                return Err(ServiceError::no_project());
            }
        }
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        // Stop any previously running server first so restarts don't leak tasks/ports.
        Self::stop_internal(state).await;

        state.set_runtime_state(RuntimeStatus::Starting, None).await;

        let bind_addr: SocketAddr = format!("127.0.0.1:{}", config.port)
            .parse()
            .map_err(|e| ServiceError::runtime_failed(&format!("invalid port: {e}")))?;

        // Bind synchronously here (not inside the spawned task) so a port
        // conflict surfaces as a real error immediately, and so there's no
        // window where "Running" is reported before the port is actually live.
        let listener = match tokio::net::TcpListener::bind(bind_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                state.set_runtime_state(RuntimeStatus::Error, None).await;
                return Err(ServiceError::runtime_failed(&format!(
                    "port {} unavailable: {e}",
                    config.port
                )));
            }
        };

        let contract = super::contract_service::ensure_contract(&root)
            .await
            .map_err(|e| {
                ServiceError::runtime_failed(&format!("failed to compile contract: {e}"))
            })?;

        let scenarios = Self::load_scenarios(&root);

        let runtime_id = format!("runtime_{}", uuid::Uuid::new_v4().simple());
        let emitter = EventEmitter::new(runtime_id, 1000);
        let mut subscription = emitter.subscribe();

        // Fixed seed for deterministic mock data, matching the CLI's default.
        let seed = 42u64;
        let server_task = {
            let emitter = emitter.clone();
            tokio::spawn(async move {
                let _ = api_mock_runtime::start_mock_server_with_listener(
                    listener, contract, seed, scenarios, true, emitter,
                )
                .await;
            })
        };

        let pump_state = state.clone();
        let event_pump_task = tokio::spawn(async move {
            while let Some(event) = subscription.recv().await {
                pump_state.push_runtime_event(event.clone()).await;
                let mut runtime = pump_state.runtime.write().await;
                match &event {
                    RuntimeEvent::RequestReceived(_) => runtime.requests += 1,
                    RuntimeEvent::ValidationFailed(_) => runtime.validation_failures += 1,
                    _ => {}
                }
            }
        });

        *state.runtime_server.write().await = Some(RunningMockServer {
            server_task,
            event_pump_task,
        });

        let address = format!("http://{bind_addr}");
        state
            .set_runtime_state(RuntimeStatus::Running, Some(address))
            .await;
        state.update_runtime_metrics(0, 0).await;

        let _ = CustomerJourneyService::complete_outcome(
            state,
            api_customer_journey::JourneyOutcome::MockReady,
        )
        .await;

        Self::get_status(state).await
    }

    /// Stop the mock runtime, aborting the real server task if one is running.
    pub async fn stop(state: &Arc<DesktopStateManager>) -> ServiceResult<RuntimeStatusInfo> {
        Self::stop_internal(state).await;
        Self::get_status(state).await
    }

    async fn stop_internal(state: &Arc<DesktopStateManager>) {
        let current = state.runtime.read().await.status;
        if current == RuntimeStatus::Stopped {
            return;
        }

        state.set_runtime_state(RuntimeStatus::Stopping, None).await;

        if let Some(server) = state.runtime_server.write().await.take() {
            server.abort().await;
        }

        state.set_runtime_state(RuntimeStatus::Stopped, None).await;
    }

    /// Restart the mock runtime
    pub async fn restart(
        state: &Arc<DesktopStateManager>,
        config: RuntimeConfig,
    ) -> ServiceResult<RuntimeStatusInfo> {
        Self::stop(state).await?;
        Self::start(state, config).await
    }

    /// Reset runtime state (clear mock data). Since the running server keeps
    /// its resource state in-process with no external reset hook, this
    /// restarts the server - a fresh `RuntimeState` is exactly "all mock
    /// data cleared" - rather than pretending to clear it in place.
    pub async fn reset_state(state: &Arc<DesktopStateManager>) -> ServiceResult<RuntimeStatusInfo> {
        {
            let project = state.project.read().await;
            if project.is_none() {
                return Err(ServiceError::no_project());
            }
        }

        let was_running = state.runtime.read().await.status == RuntimeStatus::Running;
        let port = state
            .runtime
            .read()
            .await
            .address
            .as_ref()
            .and_then(|a| a.rsplit(':').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(4010);

        if was_running {
            Self::restart(state, RuntimeConfig {
                port,
                profile_id: None,
                auto_start: false,
            })
            .await
        } else {
            state.update_runtime_metrics(0, 0).await;
            state.runtime_events.write().await.clear();
            Self::get_status(state).await
        }
    }

    /// Export runtime state. See `RuntimeStateSnapshot` docs for the
    /// wire format.
    pub async fn export_state(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<RuntimeStateSnapshot> {
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let scenarios = Self::load_scenarios(&root)
            .into_iter()
            .filter_map(|s| serde_json::to_value(s).ok())
            .collect();

        let resources = Self::fetch_live_state_payload(state).await?;

        Ok(RuntimeStateSnapshot {
            scenarios,
            resources,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Import runtime state into the running mock server.
    pub async fn import_state(
        state: &Arc<DesktopStateManager>,
        snapshot: RuntimeStateSnapshot,
    ) -> ServiceResult<()> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let base = Self::running_runtime_base_url(state).await?;
        let url = format!("{base}/__api/state/import");
        let response = reqwest::Client::new()
            .post(&url)
            .json(&snapshot.resources)
            .send()
            .await
            .map_err(|e| ServiceError::runtime_failed(&format!("state import failed: {e}")))?;

        if response.status() != StatusCode::OK {
            return Err(ServiceError::runtime_failed(&format!(
                "state import failed with HTTP {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Get recent runtime events (most recent first)
    pub async fn get_events(
        state: &Arc<DesktopStateManager>,
        limit: usize,
    ) -> ServiceResult<Vec<RuntimeEventInfo>> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let events = state.runtime_events.read().await;
        Ok(events.iter().take(limit).map(Self::event_to_info).collect())
    }

    /// Get runtime metrics
    pub async fn get_metrics(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<RuntimeMetricsInfo> {
        Ok(Self::compute_metrics(state).await)
    }

    /// Check if a port is available for binding
    pub async fn check_port_available(port: u16) -> ServiceResult<bool> {
        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .map_err(|e| ServiceError::validation(format!("invalid port: {e}")))?;
        Ok(tokio::net::TcpListener::bind(addr).await.is_ok())
    }

    async fn compute_metrics(state: &Arc<DesktopStateManager>) -> RuntimeMetricsInfo {
        let runtime = state.runtime.read().await;
        let events = state.runtime_events.read().await;

        let scenario_matches = events
            .iter()
            .filter(|e| matches!(e, RuntimeEvent::ScenarioMatched(_)))
            .count() as u64;

        let durations: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                RuntimeEvent::ResponseSent(e) => Some(e.duration_ms),
                _ => None,
            })
            .collect();
        let average_duration_ms = if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<u64>() as f64 / durations.len() as f64
        };

        RuntimeMetricsInfo {
            total_requests: runtime.requests,
            successful_requests: runtime.requests.saturating_sub(runtime.validation_failures),
            failed_requests: runtime.validation_failures,
            validation_failures: runtime.validation_failures,
            scenario_matches,
            average_duration_ms,
        }
    }

    async fn running_runtime_base_url(state: &Arc<DesktopStateManager>) -> ServiceResult<String> {
        let runtime = state.runtime.read().await;
        if runtime.status != RuntimeStatus::Running {
            return Err(ServiceError::validation(
                "Runtime must be running to export or import state",
            ));
        }

        let address = runtime.address.clone().ok_or_else(|| {
            ServiceError::runtime_failed("runtime has no address while marked running")
        })?;

        Ok(address.trim_end_matches('/').to_string())
    }

    async fn fetch_live_state_payload(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<serde_json::Value> {
        let base = Self::running_runtime_base_url(state).await?;
        let url = format!("{base}/__api/state/export");

        let response = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| ServiceError::runtime_failed(&format!("state export failed: {e}")))?;

        if response.status() != StatusCode::OK {
            return Err(ServiceError::runtime_failed(&format!(
                "state export failed with HTTP {}",
                response.status()
            )));
        }

        response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ServiceError::runtime_failed(&format!("invalid state payload: {e}")))
    }

    /// Load mock scenario files from `.repo-api/scenarios/*.yaml`, if any.
    fn load_scenarios(root: &std::path::Path) -> Vec<api_mock_runtime::MockScenario> {
        let dir = root.join(".repo-api/scenarios");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };

        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("yaml"))
            .filter_map(|e| api_mock_runtime::load_scenarios(&e.path()).ok())
            .flatten()
            .collect()
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

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn test_runtime_lifecycle() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let status = RuntimeService::start(
            &state,
            RuntimeConfig {
                port: free_port(),
                profile_id: None,
                auto_start: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(status.status, RuntimeStatus::Running);
        assert!(status.address.is_some());

        // The address should actually be reachable.
        let addr = status.address.unwrap().replace("http://", "");
        assert!(tokio::net::TcpStream::connect(&addr).await.is_ok());

        let status = RuntimeService::stop(&state).await.unwrap();
        assert_eq!(status.status, RuntimeStatus::Stopped);

        // And the port should be released again.
        assert!(tokio::net::TcpListener::bind(&addr).await.is_ok());
    }

    #[tokio::test]
    async fn test_runtime_requires_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let result = RuntimeService::start(&state, RuntimeConfig::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_fails_on_port_in_use() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let port = free_port();
        let _blocker = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();

        let result = RuntimeService::start(
            &state,
            RuntimeConfig {
                port,
                profile_id: None,
                auto_start: false,
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            state.runtime.read().await.status,
            RuntimeStatus::Error
        );
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

        // Reset (runtime isn't running, so this just clears counters)
        RuntimeService::reset_state(&state).await.unwrap();

        let runtime = state.runtime.read().await;
        assert_eq!(runtime.requests, 0);
        assert_eq!(runtime.validation_failures, 0);
    }
}
