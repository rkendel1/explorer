//! API Workbench - local application surface for the API development platform.
//!
//! This crate owns:
//! - Repository connection
//! - Endpoint explorer
//! - Request editor
//! - Response viewer
//! - Schema explorer
//! - Source evidence viewer
//! - Environments
//! - Saved requests
//! - Test suites
//! - Contract changes
//! - Mock runtime controls
//! - Runtime activity

use api_compiler::to_openapi;
use api_core::{ApiContract, ApiEndpoint, ApiEnvironment, ApiSchema};
use api_runtime_events::{EventEmitter, RuntimeMetrics};
use api_watch::{ContractChangeSet, ContractRevision, WatchEvent, WatchState};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

/// Workbench configuration
#[derive(Debug, Clone)]
pub struct WorkbenchConfig {
    pub workbench_port: u16,
    pub mock_port: u16,
    pub watch_enabled: bool,
    pub auto_open: bool,
}

impl Default for WorkbenchConfig {
    fn default() -> Self {
        Self {
            workbench_port: 4173,
            mock_port: 4010,
            watch_enabled: true,
            auto_open: true,
        }
    }
}

/// Workbench state shared across handlers
#[derive(Clone)]
pub struct WorkbenchState {
    pub root: PathBuf,
    pub contract: Arc<RwLock<Option<ApiContract>>>,
    pub revision: Arc<RwLock<Option<ContractRevision>>>,
    pub pending_changes: Arc<RwLock<Option<ContractChangeSet>>>,
    pub watch_state: Arc<RwLock<WatchState>>,
    pub environments: Arc<RwLock<Vec<ApiEnvironment>>>,
    pub mock_running: Arc<RwLock<bool>>,
    pub mock_port: u16,
    pub runtime_metrics: Arc<RwLock<RuntimeMetrics>>,
    pub event_emitter: Arc<EventEmitter>,
    pub watch_events: broadcast::Sender<WatchEvent>,
}

impl WorkbenchState {
    pub fn new(root: PathBuf, mock_port: u16) -> Self {
        let (watch_events, _) = broadcast::channel(256);
        Self {
            root,
            contract: Arc::new(RwLock::new(None)),
            revision: Arc::new(RwLock::new(None)),
            pending_changes: Arc::new(RwLock::new(None)),
            watch_state: Arc::new(RwLock::new(WatchState::Stopped)),
            environments: Arc::new(RwLock::new(Vec::new())),
            mock_running: Arc::new(RwLock::new(false)),
            mock_port,
            runtime_metrics: Arc::new(RwLock::new(RuntimeMetrics::default())),
            event_emitter: Arc::new(EventEmitter::new(
                format!("wb_{}", Uuid::new_v4().simple()),
                1024,
            )),
            watch_events,
        }
    }

    pub async fn load_initial(&self) -> anyhow::Result<()> {
        // Load environments
        let envs = api_storage::load_environments(&self.root)?;
        {
            let mut guard = self.environments.write().await;
            *guard = envs;
        }

        // Try to load existing contract
        if let Ok(contract) = api_storage::load_effective_contract(&self.root) {
            let mut guard = self.contract.write().await;
            *guard = Some(contract);
        }

        Ok(())
    }
}

/// API response types
#[derive(Debug, Serialize)]
pub struct WorkbenchStatus {
    pub repository: String,
    pub watch_state: WatchState,
    pub mock_running: bool,
    pub mock_url: String,
    pub endpoint_count: usize,
    pub schema_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_changes: Option<PendingChangeSummary>,
}

#[derive(Debug, Serialize)]
pub struct PendingChangeSummary {
    pub change_set_id: String,
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub breaking: bool,
}

#[derive(Debug, Serialize)]
pub struct EndpointSummary {
    pub id: String,
    pub method: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub confidence_score: f32,
    pub confidence_level: String,
    pub has_request_body: bool,
    pub response_count: usize,
}

#[derive(Debug, Serialize)]
pub struct EndpointDetail {
    pub endpoint: ApiEndpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schemas: Option<Vec<Value>>,
}

#[derive(Debug, Serialize)]
pub struct SchemaSummary {
    pub id: String,
    pub schema_type: String,
    pub property_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    pub name: String,
    pub variables: BTreeMap<String, VariableInfo>,
}

#[derive(Debug, Serialize)]
pub struct VariableInfo {
    pub value: String,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct SetVariableRequest {
    pub value: String,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Serialize)]
pub struct MockStatus {
    pub running: bool,
    pub address: String,
    pub requests: u64,
    pub validation_failures: u64,
    pub scenario_matches: u64,
    pub generated_responses: u64,
    pub state_mutations: u64,
    pub average_duration_ms: f64,
}

// Handler functions

async fn get_status(State(state): State<WorkbenchState>) -> Json<WorkbenchStatus> {
    let contract = state.contract.read().await;
    let revision = state.revision.read().await;
    let pending = state.pending_changes.read().await;
    let watch_state = *state.watch_state.read().await;
    let mock_running = *state.mock_running.read().await;

    let pending_summary = pending.as_ref().map(|cs| {
        let added = cs
            .changes
            .iter()
            .filter(|c| {
                c.categories
                    .iter()
                    .any(|cat| matches!(cat, api_watch::ChangeCategory::EndpointAdded))
            })
            .count();
        let modified = cs
            .changes
            .iter()
            .filter(|c| {
                c.categories
                    .iter()
                    .any(|cat| matches!(cat, api_watch::ChangeCategory::EndpointModified))
            })
            .count();
        let removed = cs
            .changes
            .iter()
            .filter(|c| {
                c.categories
                    .iter()
                    .any(|cat| matches!(cat, api_watch::ChangeCategory::EndpointRemoved))
            })
            .count();
        let breaking = cs.has_breaking_changes();

        PendingChangeSummary {
            change_set_id: cs.id.clone(),
            added,
            modified,
            removed,
            breaking,
        }
    });

    Json(WorkbenchStatus {
        repository: state.root.display().to_string(),
        watch_state,
        mock_running,
        mock_url: format!("http://127.0.0.1:{}", state.mock_port),
        endpoint_count: contract.as_ref().map(|c| c.endpoints.len()).unwrap_or(0),
        schema_count: contract
            .as_ref()
            .map(|c| c.schemas.schemas.len())
            .unwrap_or(0),
        current_revision: revision.as_ref().map(|r| r.id.clone()),
        pending_changes: pending_summary,
    })
}

async fn get_contract(State(state): State<WorkbenchState>) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => Json(c.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "no contract"}))).into_response(),
    }
}

async fn get_openapi(State(state): State<WorkbenchState>) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => Json(to_openapi(c)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "no contract"}))).into_response(),
    }
}

async fn list_endpoints(State(state): State<WorkbenchState>) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => {
            let summaries: Vec<EndpointSummary> = c
                .endpoints
                .iter()
                .map(|ep| EndpointSummary {
                    id: ep.id.clone(),
                    method: ep.method.as_str().to_uppercase(),
                    path: ep.path.clone(),
                    operation_id: ep.operation_id.clone(),
                    summary: ep.summary.clone(),
                    confidence_score: ep.confidence.score,
                    confidence_level: format!("{:?}", ep.confidence.level),
                    has_request_body: !ep.request_bodies.is_empty(),
                    response_count: ep.responses.len(),
                })
                .collect();
            Json(summaries).into_response()
        }
        None => Json(Vec::<EndpointSummary>::new()).into_response(),
    }
}

async fn get_endpoint(
    State(state): State<WorkbenchState>,
    Path(endpoint_id): Path<String>,
) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => {
            let endpoint = c.endpoints.iter().find(|e| e.id == endpoint_id);
            match endpoint {
                Some(ep) => Json(EndpointDetail {
                    endpoint: ep.clone(),
                    request_schema: None,
                    response_schemas: None,
                })
                .into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "endpoint not found"})),
                )
                    .into_response(),
            }
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "no contract"}))).into_response(),
    }
}

async fn list_schemas(State(state): State<WorkbenchState>) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => {
            let summaries: Vec<SchemaSummary> = c
                .schemas
                .schemas
                .iter()
                .map(|(id, schema)| SchemaSummary {
                    id: id.clone(),
                    schema_type: schema_type_name(schema),
                    property_count: match schema {
                        ApiSchema::Object(o) => Some(o.properties.len()),
                        _ => None,
                    },
                })
                .collect();
            Json(summaries).into_response()
        }
        None => Json(Vec::<SchemaSummary>::new()).into_response(),
    }
}

async fn get_schema(
    State(state): State<WorkbenchState>,
    Path(schema_id): Path<String>,
) -> Response {
    let contract = state.contract.read().await;
    match contract.as_ref() {
        Some(c) => match c.schemas.schemas.get(&schema_id) {
            Some(schema) => Json(json!({
                "id": schema_id,
                "schema": schema,
            }))
            .into_response(),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "schema not found"})),
            )
                .into_response(),
        },
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "no contract"}))).into_response(),
    }
}

fn schema_type_name(schema: &ApiSchema) -> String {
    match schema {
        ApiSchema::Null => "null".into(),
        ApiSchema::Boolean => "boolean".into(),
        ApiSchema::Integer(_) => "integer".into(),
        ApiSchema::Number(_) => "number".into(),
        ApiSchema::String(_) => "string".into(),
        ApiSchema::Array(_) => "array".into(),
        ApiSchema::Object(_) => "object".into(),
        ApiSchema::Enum(_) => "enum".into(),
        ApiSchema::OneOf(_) => "oneOf".into(),
        ApiSchema::AnyOf(_) => "anyOf".into(),
        ApiSchema::AllOf(_) => "allOf".into(),
        ApiSchema::Reference(_) => "reference".into(),
        ApiSchema::Unknown => "unknown".into(),
    }
}

async fn list_environments(State(state): State<WorkbenchState>) -> Json<Vec<EnvironmentResponse>> {
    let envs = state.environments.read().await;
    Json(
        envs.iter()
            .map(|e| EnvironmentResponse {
                name: e.name.clone(),
                variables: e
                    .variables
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            VariableInfo {
                                value: v.clone(),
                                secret: false,
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
    )
}

async fn create_environment(
    State(state): State<WorkbenchState>,
    Json(req): Json<CreateEnvironmentRequest>,
) -> Response {
    let mut envs = state.environments.write().await;
    if envs.iter().any(|e| e.name == req.name) {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "environment already exists"})),
        )
            .into_response();
    }

    let env = ApiEnvironment {
        name: req.name.clone(),
        variables: req.variables,
    };
    envs.push(env);

    // Save to storage
    if let Err(e) = api_storage::save_environments(&state.root, &envs) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    (StatusCode::CREATED, Json(json!({"name": req.name}))).into_response()
}

async fn set_environment_variable(
    State(state): State<WorkbenchState>,
    Path((env_name, var_name)): Path<(String, String)>,
    Json(req): Json<SetVariableRequest>,
) -> Response {
    let mut envs = state.environments.write().await;
    let env = envs.iter_mut().find(|e| e.name == env_name);

    match env {
        Some(e) => {
            e.variables.insert(var_name.clone(), req.value);
            if let Err(e) = api_storage::save_environments(&state.root, &envs) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
            Json(json!({"variable": var_name})).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "environment not found"})),
        )
            .into_response(),
    }
}

async fn get_mock_status(State(state): State<WorkbenchState>) -> Json<MockStatus> {
    let running = *state.mock_running.read().await;
    let metrics = state.runtime_metrics.read().await;

    Json(MockStatus {
        running,
        address: format!("http://127.0.0.1:{}", state.mock_port),
        requests: metrics.total_requests,
        validation_failures: metrics.validation_failures,
        scenario_matches: metrics.scenario_matches,
        generated_responses: metrics.generated_responses,
        state_mutations: metrics.state_mutations,
        average_duration_ms: metrics.average_duration_ms(),
    })
}

async fn reset_mock_state(State(state): State<WorkbenchState>) -> Json<Value> {
    let mut metrics = state.runtime_metrics.write().await;
    *metrics = RuntimeMetrics::default();
    Json(json!({"status": "reset"}))
}

async fn get_pending_changes(State(state): State<WorkbenchState>) -> Response {
    let pending = state.pending_changes.read().await;
    match pending.as_ref() {
        Some(cs) => Json(cs.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no pending changes"})),
        )
            .into_response(),
    }
}

async fn accept_changes(State(state): State<WorkbenchState>) -> Response {
    let mut pending = state.pending_changes.write().await;
    if pending.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no pending changes"})),
        )
            .into_response();
    }

    // Accept the changes
    let cs = pending.take().unwrap();

    // Load and apply
    match api_storage::load_effective_contract(&state.root) {
        Ok(contract) => {
            let mut guard = state.contract.write().await;
            *guard = Some(contract);
            let mut ws = state.watch_state.write().await;
            *ws = WatchState::Synchronized;
            Json(json!({"status": "accepted", "change_set_id": cs.id})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn reject_changes(State(state): State<WorkbenchState>) -> Response {
    let mut pending = state.pending_changes.write().await;
    if pending.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no pending changes"})),
        )
            .into_response();
    }

    let cs = pending.take().unwrap();
    let mut ws = state.watch_state.write().await;
    *ws = WatchState::Synchronized;
    Json(json!({"status": "rejected", "change_set_id": cs.id})).into_response()
}

async fn events_stream(
    State(state): State<WorkbenchState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.watch_events.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create the workbench router
pub fn create_router(state: WorkbenchState) -> Router {
    Router::new()
        // Status
        .route("/api/status", get(get_status))
        // Contract
        .route("/api/contract", get(get_contract))
        .route("/api/openapi", get(get_openapi))
        // Endpoints
        .route("/api/endpoints", get(list_endpoints))
        .route("/api/endpoints/{id}", get(get_endpoint))
        // Schemas
        .route("/api/schemas", get(list_schemas))
        .route("/api/schemas/{id}", get(get_schema))
        // Environments
        .route("/api/environments", get(list_environments))
        .route("/api/environments", post(create_environment))
        .route(
            "/api/environments/{env}/{var}",
            post(set_environment_variable),
        )
        // Mock runtime
        .route("/api/mock/status", get(get_mock_status))
        .route("/api/mock/reset", post(reset_mock_state))
        // Changes
        .route("/api/changes", get(get_pending_changes))
        .route("/api/changes/accept", post(accept_changes))
        .route("/api/changes/reject", post(reject_changes))
        // Events
        .route("/api/events", get(events_stream))
        .with_state(state)
}

/// Start the workbench server
pub async fn start_workbench(root: PathBuf, config: WorkbenchConfig) -> anyhow::Result<()> {
    let state = WorkbenchState::new(root.clone(), config.mock_port);
    state.load_initial().await?;

    let app = create_router(state.clone());
    let addr = SocketAddr::from(([127, 0, 0, 1], config.workbench_port));

    println!("API Workbench");
    println!("Repository:");
    println!("  {}", root.display());
    println!("Open:");
    println!("  http://127.0.0.1:{}", config.workbench_port);
    println!("Mock:");
    println!("  http://127.0.0.1:{}", config.mock_port);
    println!("Status:");
    println!("  synchronized");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbench_config_default() {
        let config = WorkbenchConfig::default();
        assert_eq!(config.workbench_port, 4173);
        assert_eq!(config.mock_port, 4010);
        assert!(config.watch_enabled);
    }

    #[tokio::test]
    async fn workbench_state_creation() {
        let state = WorkbenchState::new(PathBuf::from("/tmp/test"), 4010);
        assert!(!*state.mock_running.read().await);
        assert!(state.contract.read().await.is_none());
    }
}
