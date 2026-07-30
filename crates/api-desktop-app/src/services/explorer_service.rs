//! Explorer service for API endpoint discovery and viewing.
//!
//! This service handles:
//! - Endpoint listing from canonical contract
//! - Endpoint detail retrieval
//! - Schema browsing
//! - Evidence linking

use std::sync::Arc;

use api_core::ApiSchema;
use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;
use crate::{EndpointDetail, EndpointSummary, EvidenceInfo, ParameterInfo, ResponseInfo};

use super::{ServiceError, ServiceResult};

/// Endpoint filter options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EndpointFilter {
    pub method: Option<String>,
    pub path_contains: Option<String>,
}

/// Schema summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSummary {
    pub name: String,
    pub schema_type: String,
    pub properties: Vec<String>,
    pub used_by: Vec<String>,
}

/// Schema detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDetail {
    pub name: String,
    pub schema_type: String,
    pub description: Option<String>,
    pub properties: Vec<SchemaProperty>,
    pub required: Vec<String>,
    pub example: Option<serde_json::Value>,
}

/// Schema property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaProperty {
    pub name: String,
    pub property_type: String,
    pub description: Option<String>,
    pub required: bool,
    pub format: Option<String>,
}

/// Explorer service implementation
pub struct ExplorerService;

impl ExplorerService {
    /// List all endpoints with optional filtering
    pub async fn list_endpoints(
        state: &Arc<DesktopStateManager>,
        filter: Option<EndpointFilter>,
    ) -> ServiceResult<Vec<EndpointSummary>> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        // Try to load effective contract
        let contract = api_storage::load_effective_contract(root);

        if let Ok(contract) = contract {
            let filter = filter.unwrap_or_default();

            let endpoints: Vec<EndpointSummary> = contract
                .endpoints
                .iter()
                .filter(|ep| {
                    // Apply method filter
                    if let Some(method) = &filter.method
                        && ep.method.as_str().to_uppercase() != method.to_uppercase()
                    {
                        return false;
                    }
                    // Apply path filter
                    if let Some(path_contains) = &filter.path_contains
                        && !ep
                            .path
                            .to_lowercase()
                            .contains(&path_contains.to_lowercase())
                    {
                        return false;
                    }
                    true
                })
                .map(|ep| EndpointSummary {
                    id: ep.id.clone(),
                    method: ep.method.as_str().to_uppercase(),
                    path: ep.path.clone(),
                    summary: ep.summary.clone(),
                    confidence: ep.confidence.score,
                    tag: None,
                })
                .collect();

            Ok(endpoints)
        } else {
            // Return empty list if no contract
            Ok(Vec::new())
        }
    }

    /// Get endpoint detail
    pub async fn get_endpoint(
        state: &Arc<DesktopStateManager>,
        endpoint_id: &str,
    ) -> ServiceResult<EndpointDetail> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let contract = api_storage::load_effective_contract(root)
            .map_err(|_| ServiceError::not_found("Contract"))?;

        let endpoint = contract
            .endpoints
            .iter()
            .find(|ep| ep.id == endpoint_id || ep.operation_id.as_deref() == Some(endpoint_id))
            .ok_or_else(|| ServiceError::not_found("Endpoint"))?;

        let parameters: Vec<ParameterInfo> = endpoint
            .parameters
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.clone(),
                location: format!("{:?}", p.location).to_lowercase(),
                required: p.required,
                schema_type: p.schema.id.clone(),
                schema_ref: Some(format!("#/components/schemas/{}", p.schema.id)),
            })
            .collect();

        let responses: Vec<ResponseInfo> = endpoint
            .responses
            .iter()
            .map(|r| ResponseInfo {
                status: r.status,
                content_type: r.content_type.clone(),
                schema_ref: r
                    .schema
                    .as_ref()
                    .map(|s| format!("#/components/schemas/{}", s.id)),
                example: r.example.clone(),
            })
            .collect();

        let evidence: Vec<EvidenceInfo> = endpoint
            .evidence
            .iter()
            .map(|e| EvidenceInfo {
                file: e.file.clone(),
                line_start: e.line_start,
                line_end: e.line_end,
            })
            .collect();

        Ok(EndpointDetail {
            id: endpoint.id.clone(),
            method: endpoint.method.as_str().to_uppercase(),
            path: endpoint.path.clone(),
            summary: endpoint.summary.clone(),
            description: None, // ApiEndpoint doesn't have description
            parameters,
            request_body: endpoint
                .request_bodies
                .first()
                .map(|rb| crate::RequestBodyInfo {
                    content_type: rb.content_type.clone(),
                    required: rb.required,
                    schema_ref: Some(format!("#/components/schemas/{}", rb.schema.id)),
                    example: rb.example.clone(),
                }),
            responses,
            security: endpoint.security.schemes.clone(),
            confidence: endpoint.confidence.score,
            evidence,
        })
    }

    /// List all schemas
    pub async fn list_schemas(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<SchemaSummary>> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let contract = api_storage::load_effective_contract(root);

        if let Ok(contract) = contract {
            let schemas: Vec<SchemaSummary> = contract
                .schemas
                .schemas
                .iter()
                .map(|(name, schema)| {
                    let (schema_type, properties) = Self::extract_schema_info(schema);

                    SchemaSummary {
                        name: name.clone(),
                        schema_type,
                        properties,
                        used_by: vec![], // Would need to calculate from endpoints
                    }
                })
                .collect();

            Ok(schemas)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get schema detail
    pub async fn get_schema(
        state: &Arc<DesktopStateManager>,
        schema_name: &str,
    ) -> ServiceResult<SchemaDetail> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let contract = api_storage::load_effective_contract(root)
            .map_err(|_| ServiceError::not_found("Contract"))?;

        let schema = contract
            .schemas
            .schemas
            .get(schema_name)
            .ok_or_else(|| ServiceError::not_found("Schema"))?;

        let (schema_type, properties, required) = Self::extract_schema_detail(schema);

        Ok(SchemaDetail {
            name: schema_name.to_string(),
            schema_type,
            description: None,
            properties,
            required,
            example: None,
        })
    }

    /// Trigger a real contract re-scan (discovery + compile), replacing
    /// whatever generated/effective contracts already exist.
    pub async fn refresh_contract(state: &Arc<DesktopStateManager>) -> ServiceResult<usize> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let contract = super::contract_service::scan_and_persist(&root)
            .await
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(contract.endpoints.len())
    }

    // Helper to extract schema type and property names
    fn extract_schema_info(schema: &ApiSchema) -> (String, Vec<String>) {
        match schema {
            ApiSchema::Object(obj) => {
                let props: Vec<String> = obj.properties.keys().cloned().collect();
                ("object".to_string(), props)
            }
            ApiSchema::Array(_) => ("array".to_string(), vec![]),
            ApiSchema::String(_) => ("string".to_string(), vec![]),
            ApiSchema::Integer(_) => ("integer".to_string(), vec![]),
            ApiSchema::Number(_) => ("number".to_string(), vec![]),
            ApiSchema::Boolean => ("boolean".to_string(), vec![]),
            ApiSchema::Enum(e) => ("enum".to_string(), e.values.clone()),
            _ => ("unknown".to_string(), vec![]),
        }
    }

    // Helper to extract full schema detail
    fn extract_schema_detail(schema: &ApiSchema) -> (String, Vec<SchemaProperty>, Vec<String>) {
        match schema {
            ApiSchema::Object(obj) => {
                let props: Vec<SchemaProperty> = obj
                    .properties
                    .keys()
                    .map(|name| SchemaProperty {
                        name: name.clone(),
                        property_type: "string".to_string(), // Would need to resolve ref
                        description: None,
                        required: obj.required.contains(name),
                        format: None,
                    })
                    .collect();
                ("object".to_string(), props, obj.required.clone())
            }
            _ => {
                let (schema_type, _) = Self::extract_schema_info(schema);
                (schema_type, vec![], vec![])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_list_endpoints_no_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let result = ExplorerService::list_endpoints(&state, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_endpoints_no_contract() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let endpoints = ExplorerService::list_endpoints(&state, None).await.unwrap();
        assert!(endpoints.is_empty());
    }

    #[tokio::test]
    async fn test_list_schemas_no_contract() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let schemas = ExplorerService::list_schemas(&state).await.unwrap();
        assert!(schemas.is_empty());
    }
}
