//! Contract and endpoint commands

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;
use crate::{EndpointDetail, EndpointSummary, EvidenceInfo, ParameterInfo, ResponseInfo};

use super::CommandResult;

/// Get endpoint request
#[derive(Debug, Deserialize)]
pub struct GetEndpointRequest {
    pub id: String,
}

/// Schema summary
#[derive(Debug, Clone, Serialize)]
pub struct SchemaSummary {
    pub name: String,
    pub schema_type: String,
    pub properties: Vec<String>,
}

/// Get contract request
#[derive(Debug, Deserialize)]
pub struct GetContractRequest {
    pub format: Option<String>,
}

/// Contract response
#[derive(Debug, Serialize)]
pub struct ContractResponse {
    pub version: String,
    pub environment_count: usize,
    pub has_contract: bool,
}

/// List all endpoints
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn endpoint_list(state: Arc<DesktopStateManager>) -> CommandResult<Vec<EndpointSummary>> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        // Generate sample endpoints based on environments
        let endpoints: Vec<EndpointSummary> = project
            .environments
            .iter()
            .enumerate()
            .take(5)
            .map(|(i, env)| EndpointSummary {
                id: format!("endpoint-{}", i),
                method: if i % 2 == 0 {
                    "GET".to_string()
                } else {
                    "POST".to_string()
                },
                path: format!("/{}", env.name.to_lowercase().replace(' ', "-")),
                summary: Some(format!("Endpoint for {}", env.name)),
                confidence: 0.95,
                tag: Some("default".to_string()),
            })
            .collect();

        CommandResult::ok(endpoints)
    } else {
        CommandResult::error("No project open")
    }
}

/// Get endpoint details
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn endpoint_get(
    state: Arc<DesktopStateManager>,
    request: GetEndpointRequest,
) -> CommandResult<EndpointDetail> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    let detail = EndpointDetail {
        id: request.id.clone(),
        method: "GET".to_string(),
        path: "/example".to_string(),
        summary: Some("Example endpoint".to_string()),
        description: Some("An example endpoint for demonstration".to_string()),
        parameters: vec![ParameterInfo {
            name: "id".to_string(),
            location: "path".to_string(),
            required: true,
            schema_type: "string".to_string(),
        }],
        request_body: None,
        responses: vec![ResponseInfo {
            status: 200,
            content_type: Some("application/json".to_string()),
            schema_ref: Some("#/components/schemas/Example".to_string()),
        }],
        security: vec!["bearerAuth".to_string()],
        confidence: 0.95,
        evidence: vec![EvidenceInfo {
            file: "src/routes/example.ts".to_string(),
            line_start: Some(10),
            line_end: Some(25),
        }],
    };

    CommandResult::ok(detail)
}

/// List all schemas
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn schema_list(state: Arc<DesktopStateManager>) -> CommandResult<Vec<SchemaSummary>> {
    let project = state.project.read().await;

    if project.is_none() {
        return CommandResult::error("No project open");
    }

    let schemas = vec![SchemaSummary {
        name: "Example".to_string(),
        schema_type: "object".to_string(),
        properties: vec!["id".to_string(), "name".to_string()],
    }];

    CommandResult::ok(schemas)
}

/// Get the current contract
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn contract_get(
    state: Arc<DesktopStateManager>,
    _request: GetContractRequest,
) -> CommandResult<ContractResponse> {
    let project = state.project.read().await;

    if let Some(project) = project.as_ref() {
        CommandResult::ok(ContractResponse {
            version: "1.0.0".to_string(),
            environment_count: project.environments.len(),
            has_contract: !project.contract.path.is_empty(),
        })
    } else {
        CommandResult::error("No project open")
    }
}
