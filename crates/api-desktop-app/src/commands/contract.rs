//! Contract and endpoint commands

use serde::{Deserialize, Serialize};

use crate::services::ExplorerService;
use crate::services::explorer_service::{EndpointFilter, SchemaDetail, SchemaSummary};
use crate::{ApiMeaningGraph, EndpointDetail, EndpointSummary};

use super::{AppState, CommandResult, from_service, state_handle};

/// Get endpoint request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEndpointRequest {
    pub id: String,
}

/// List endpoints request
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEndpointsRequest {
    #[serde(default)]
    pub filter: Option<EndpointFilter>,
}

/// Get schema request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSchemaRequest {
    pub name: String,
}

/// Contract response
#[derive(Debug, Serialize)]
pub struct ContractResponse {
    pub has_contract: bool,
    pub endpoint_count: usize,
    pub schema_count: usize,
}

/// List all endpoints
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn endpoint_list(
    state: AppState<'_>,
    request: Option<ListEndpointsRequest>,
) -> CommandResult<Vec<EndpointSummary>> {
    let state = state_handle(&state);
    let filter = request.and_then(|r| r.filter);
    from_service(ExplorerService::list_endpoints(&state, filter).await)
}

/// Get endpoint details
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn endpoint_get(
    state: AppState<'_>,
    request: GetEndpointRequest,
) -> CommandResult<EndpointDetail> {
    let state = state_handle(&state);
    from_service(ExplorerService::get_endpoint(&state, &request.id).await)
}

/// List all schemas
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn schema_list(state: AppState<'_>) -> CommandResult<Vec<SchemaSummary>> {
    let state = state_handle(&state);
    from_service(ExplorerService::list_schemas(&state).await)
}

/// Get schema detail
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn schema_get(
    state: AppState<'_>,
    request: GetSchemaRequest,
) -> CommandResult<SchemaDetail> {
    let state = state_handle(&state);
    from_service(ExplorerService::get_schema(&state, &request.name).await)
}

/// Get the current contract summary
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn contract_get(state: AppState<'_>) -> CommandResult<ContractResponse> {
    let state = state_handle(&state);
    let endpoints = from_service(ExplorerService::list_endpoints(&state, None).await)?;
    let schemas = from_service(ExplorerService::list_schemas(&state).await)?;

    Ok(ContractResponse {
        has_contract: !endpoints.is_empty() || !schemas.is_empty(),
        endpoint_count: endpoints.len(),
        schema_count: schemas.len(),
    })
}

/// Trigger a contract re-scan
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn contract_rescan(state: AppState<'_>) -> CommandResult<usize> {
    let state = state_handle(&state);
    from_service(ExplorerService::refresh_contract(&state).await)
}

/// Build semantic meaning graph for the active API contract
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn contract_meaning_graph(state: AppState<'_>) -> CommandResult<ApiMeaningGraph> {
    let state = state_handle(&state);
    from_service(ExplorerService::meaning_graph(&state).await)
}
