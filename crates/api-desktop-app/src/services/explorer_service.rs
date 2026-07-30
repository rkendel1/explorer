//! Explorer service for API endpoint discovery and viewing.
//!
//! This service handles:
//! - Endpoint listing from canonical contract
//! - Endpoint detail retrieval
//! - Schema browsing
//! - Evidence linking

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use api_core::{ApiEndpoint, ApiSchema};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::state::DesktopStateManager;
use crate::{
    ApiMeaningGraph, BehaviorScore, BehavioralFingerprint, EndpointDetail,
    EndpointSemanticIntent, EndpointSummary, EvidenceInfo, MeaningGraphEdge,
    MeaningGraphEndpointIntent, MeaningGraphNode, ParameterInfo, ResponseInfo,
};

use super::{ServiceError, ServiceResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MeaningGraphCacheFile {
    cache_key: String,
    graph: ApiMeaningGraph,
}

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
            let known_models: Vec<String> = contract.schemas.schemas.keys().cloned().collect();

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
                .map(|ep| {
                    let semantic_intent = Self::infer_endpoint_semantic_intent(ep, &known_models);
                    EndpointSummary {
                        id: ep.id.clone(),
                        method: ep.method.as_str().to_uppercase(),
                        path: ep.path.clone(),
                        summary: ep.summary.clone(),
                        confidence: ep.confidence.score,
                        tag: semantic_intent.capabilities.first().cloned(),
                    }
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
        let known_models: Vec<String> = contract.schemas.schemas.keys().cloned().collect();

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

        let semantic_intent = Some(Self::infer_endpoint_semantic_intent(endpoint, &known_models));

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
            semantic_intent,
        })
    }

    /// Build an API meaning graph that links endpoints to capabilities,
    /// domain concepts, and data models.
    pub async fn meaning_graph(state: &Arc<DesktopStateManager>) -> ServiceResult<ApiMeaningGraph> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let contract = api_storage::load_effective_contract(root)
            .map_err(|_| ServiceError::not_found("Contract"))?;

        let cache_key = Self::meaning_graph_cache_key(&contract);
        if let Some(cached) = Self::load_cached_meaning_graph(root, &cache_key)? {
            return Ok(cached);
        }

        let known_models: Vec<String> = contract.schemas.schemas.keys().cloned().collect();

        let mut nodes: Vec<MeaningGraphNode> = Vec::new();
        let mut edges: Vec<MeaningGraphEdge> = Vec::new();
        let mut endpoint_intents: Vec<MeaningGraphEndpointIntent> = Vec::new();

        let mut seen_nodes = BTreeSet::new();
        let mut seen_edges = BTreeSet::new();
        let mut aggregate_behavior_scores: BTreeMap<String, f32> = BTreeMap::new();

        for endpoint in &contract.endpoints {
            let intent = Self::infer_endpoint_semantic_intent(endpoint, &known_models);

            let endpoint_node_id = format!("endpoint:{}", endpoint.id);
            Self::push_node(
                &mut nodes,
                &mut seen_nodes,
                MeaningGraphNode {
                    id: endpoint_node_id.clone(),
                    label: format!("{} {}", endpoint.method.as_str().to_uppercase(), endpoint.path),
                    node_type: "endpoint".to_string(),
                },
            );

            for capability in &intent.capabilities {
                let capability_node = format!("capability:{}", Self::slug(capability));
                Self::push_node(
                    &mut nodes,
                    &mut seen_nodes,
                    MeaningGraphNode {
                        id: capability_node.clone(),
                        label: capability.clone(),
                        node_type: "capability".to_string(),
                    },
                );
                Self::push_edge(
                    &mut edges,
                    &mut seen_edges,
                    MeaningGraphEdge {
                        source: endpoint_node_id.clone(),
                        target: capability_node,
                        relation: "supports_capability".to_string(),
                        weight: intent.confidence,
                    },
                );
            }

            for concept in &intent.domain_concepts {
                let concept_node = format!("concept:{}", Self::slug(concept));
                Self::push_node(
                    &mut nodes,
                    &mut seen_nodes,
                    MeaningGraphNode {
                        id: concept_node.clone(),
                        label: concept.clone(),
                        node_type: "concept".to_string(),
                    },
                );
                Self::push_edge(
                    &mut edges,
                    &mut seen_edges,
                    MeaningGraphEdge {
                        source: endpoint_node_id.clone(),
                        target: concept_node,
                        relation: "operates_on".to_string(),
                        weight: intent.confidence,
                    },
                );
            }

            for model in &intent.data_models {
                let model_node = format!("model:{}", Self::slug(model));
                Self::push_node(
                    &mut nodes,
                    &mut seen_nodes,
                    MeaningGraphNode {
                        id: model_node.clone(),
                        label: model.clone(),
                        node_type: "data_model".to_string(),
                    },
                );
                Self::push_edge(
                    &mut edges,
                    &mut seen_edges,
                    MeaningGraphEdge {
                        source: endpoint_node_id.clone(),
                        target: model_node,
                        relation: "uses_model".to_string(),
                        weight: intent.confidence,
                    },
                );
            }

            endpoint_intents.push(MeaningGraphEndpointIntent {
                endpoint_id: endpoint.id.clone(),
                method: endpoint.method.as_str().to_uppercase(),
                path: endpoint.path.clone(),
                primary_intent: intent.primary_intent,
                confidence: intent.confidence,
                capabilities: intent.capabilities,
                domain_concepts: intent.domain_concepts,
                data_models: intent.data_models,
                behavioral_fingerprint: intent.behavioral_fingerprint.clone(),
            });

            for score in intent.behavioral_fingerprint.scores {
                *aggregate_behavior_scores.entry(score.behavior).or_insert(0.0) += score.score;
            }
        }

        let behavioral_fingerprint = Self::normalize_behavior_scores(aggregate_behavior_scores, contract.endpoints.len());
        let graph = ApiMeaningGraph {
            cache_key,
            generated_at: Utc::now(),
            nodes,
            edges,
            endpoint_intents,
            behavioral_fingerprint,
        };

        Self::save_cached_meaning_graph(root, &graph)?;

        Ok(graph)
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

    fn push_node(
        nodes: &mut Vec<MeaningGraphNode>,
        seen_nodes: &mut std::collections::BTreeSet<String>,
        node: MeaningGraphNode,
    ) {
        if seen_nodes.insert(node.id.clone()) {
            nodes.push(node);
        }
    }

    fn push_edge(
        edges: &mut Vec<MeaningGraphEdge>,
        seen_edges: &mut std::collections::BTreeSet<String>,
        edge: MeaningGraphEdge,
    ) {
        let key = format!("{}|{}|{}", edge.source, edge.target, edge.relation);
        if seen_edges.insert(key) {
            edges.push(edge);
        }
    }

    fn infer_endpoint_semantic_intent(
        endpoint: &ApiEndpoint,
        known_models: &[String],
    ) -> EndpointSemanticIntent {
        let method = endpoint.method.as_str().to_uppercase();
        let path_tokens = Self::extract_path_tokens(&endpoint.path);
        let summary_tokens = endpoint
            .summary
            .as_ref()
            .map(|s| Self::tokenize_text(s))
            .unwrap_or_default();
        let operation_tokens = endpoint
            .operation_id
            .as_ref()
            .map(|s| Self::tokenize_identifier(s))
            .unwrap_or_default();

        let mut schema_tokens = Vec::new();
        for parameter in &endpoint.parameters {
            schema_tokens.extend(Self::tokenize_identifier(&parameter.schema.id));
            schema_tokens.extend(Self::tokenize_identifier(&parameter.name));
        }
        for body in &endpoint.request_bodies {
            schema_tokens.extend(Self::tokenize_identifier(&body.schema.id));
        }
        for response in &endpoint.responses {
            if let Some(schema) = &response.schema {
                schema_tokens.extend(Self::tokenize_identifier(&schema.id));
            }
        }

        let mut evidence_tokens = Vec::new();
        for evidence in &endpoint.evidence {
            let file_stem = std::path::Path::new(&evidence.file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            evidence_tokens.extend(Self::tokenize_identifier(file_stem));
        }

        let all_tokens: Vec<String> = path_tokens
            .iter()
            .chain(summary_tokens.iter())
            .chain(operation_tokens.iter())
            .chain(schema_tokens.iter())
            .chain(evidence_tokens.iter())
            .cloned()
            .collect();

        let mut concepts = BTreeSet::new();
        for token in &all_tokens {
            if !Self::is_stopword(token) {
                concepts.insert(Self::title_case(token));
            }
        }

        let capability_catalog: [(&str, &[&str], f32); 8] = [
            (
                "Authentication & Authorization",
                &["auth", "token", "login", "logout", "oauth", "session", "identity"],
                1.0,
            ),
            (
                "Identity & Accounts",
                &["user", "users", "customer", "account", "profile", "member"],
                0.85,
            ),
            (
                "Order & Payment Processing",
                &["order", "orders", "cart", "checkout", "invoice", "payment", "billing"],
                0.9,
            ),
            (
                "Catalog & Inventory",
                &["catalog", "product", "products", "item", "items", "inventory", "sku"],
                0.8,
            ),
            (
                "Search & Discovery",
                &["search", "query", "find", "lookup", "discover"],
                0.7,
            ),
            (
                "Platform Health & Observability",
                &["health", "status", "ready", "metrics", "ping", "monitor"],
                0.75,
            ),
            (
                "Workflow Automation",
                &["workflow", "step", "approve", "submit", "transition", "stage"],
                0.8,
            ),
            (
                "Events & Integrations",
                &["event", "events", "webhook", "notification", "stream", "emit"],
                0.85,
            ),
        ];

        let mut capability_scores: Vec<(String, f32)> = capability_catalog
            .iter()
            .filter_map(|(label, keywords, base_weight)| {
                let token_hits = Self::count_keyword_hits(&all_tokens, keywords) as f32;
                if token_hits <= 0.0 {
                    return None;
                }

                // Strengthen intent when operation_id and summary both contribute.
                let operation_hits = Self::count_keyword_hits(&operation_tokens, keywords) as f32;
                let summary_hits = Self::count_keyword_hits(&summary_tokens, keywords) as f32;
                let schema_hits = Self::count_keyword_hits(&schema_tokens, keywords) as f32;
                let weighted = (token_hits * *base_weight)
                    + (operation_hits * 0.8)
                    + (summary_hits * 0.6)
                    + (schema_hits * 0.4);
                Some(((*label).to_string(), weighted))
            })
            .collect();

        capability_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut capabilities: Vec<String> = capability_scores
            .iter()
            .take(3)
            .map(|(label, _)| label.clone())
            .collect();
        if capabilities.is_empty() {
            capabilities.push("General API Operations".to_string());
        }

        let mut data_models = BTreeSet::new();
        for parameter in &endpoint.parameters {
            data_models.insert(parameter.schema.id.clone());
        }
        for body in &endpoint.request_bodies {
            data_models.insert(body.schema.id.clone());
        }
        for response in &endpoint.responses {
            if let Some(schema) = &response.schema {
                data_models.insert(schema.id.clone());
            }
        }
        if data_models.contains("unknown") {
            data_models.remove("unknown");
        }

        // Infer likely data models from path/operation tokens when direct schema refs are weak.
        if data_models.is_empty() {
            for model in known_models {
                let model_tokens = Self::tokenize_identifier(model);
                if model_tokens
                    .iter()
                    .any(|token| all_tokens.iter().any(|input| input == token))
                {
                    data_models.insert(model.clone());
                }
            }
        }

        let primary_noun = concepts
            .iter()
            .find(|c| c.as_str() != "Api" && c.as_str() != "V1")
            .cloned()
            .unwrap_or_else(|| "Resource".to_string());
        let action = Self::action_phrase(&method, &endpoint.path);
        let primary_intent = format!("{} {}", action, primary_noun.to_lowercase());

        let behavioral_fingerprint = Self::compute_behavioral_fingerprint(
            endpoint,
            &all_tokens,
            &capabilities,
            !data_models.is_empty(),
        );

        let mut confidence = endpoint.confidence.score;
        if endpoint.summary.is_some() {
            confidence = (confidence + 0.05).min(1.0);
        }
        if !data_models.is_empty() {
            confidence = (confidence + 0.05).min(1.0);
        }
        if endpoint.operation_id.is_some() {
            confidence = (confidence + 0.05).min(1.0);
        }

        EndpointSemanticIntent {
            primary_intent,
            rationale: format!(
                "Inferred from {} {}{}{}",
                method,
                endpoint.path,
                endpoint
                    .summary
                    .as_ref()
                    .map(|s| format!(", summary '{s}'"))
                    .unwrap_or_default(),
                if data_models.is_empty() {
                    "".to_string()
                } else {
                    format!(", models [{}]", data_models.iter().cloned().collect::<Vec<_>>().join(", "))
                }
            ),
            confidence,
            capabilities,
            domain_concepts: concepts.into_iter().collect(),
            data_models: data_models.into_iter().collect(),
            behavioral_fingerprint,
        }
    }

    fn compute_behavioral_fingerprint(
        endpoint: &ApiEndpoint,
        tokens: &[String],
        capabilities: &[String],
        has_data_model: bool,
    ) -> BehavioralFingerprint {
        let method = endpoint.method.as_str().to_uppercase();
        let has_item_path = endpoint.path.contains('{') || endpoint.path.contains(':') || endpoint.path.contains('<');
        let has_id_param = endpoint.parameters.iter().any(|p| {
            let name = p.name.to_lowercase();
            matches!(p.location, api_core::ParameterLocation::Path)
                || name.ends_with("id")
                || name == "id"
        });

        let mut scores = BTreeMap::new();
        scores.insert("crud_ish".to_string(), 0.0_f32);
        scores.insert("workflow_driven".to_string(), 0.0_f32);
        scores.insert("event_sourced".to_string(), 0.0_f32);
        scores.insert("rpc_style".to_string(), 0.0_f32);

        if matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            *scores.get_mut("crud_ish").expect("crud") += 0.4;
        }
        if has_item_path || has_id_param {
            *scores.get_mut("crud_ish").expect("crud") += 0.25;
        }
        if has_data_model {
            *scores.get_mut("crud_ish").expect("crud") += 0.2;
        }

        if Self::contains_any(tokens, &["workflow", "approve", "submit", "transition", "stage", "step", "retry"]) {
            *scores.get_mut("workflow_driven").expect("workflow") += 0.65;
        }
        if capabilities.iter().any(|c| c == "Workflow Automation") {
            *scores.get_mut("workflow_driven").expect("workflow") += 0.2;
        }

        if Self::contains_any(tokens, &["event", "events", "stream", "replay", "append", "aggregate", "webhook"]) {
            *scores.get_mut("event_sourced").expect("event") += 0.7;
        }
        if endpoint.path.contains("/events") || endpoint.path.contains("/stream") {
            *scores.get_mut("event_sourced").expect("event") += 0.2;
        }

        if Self::contains_any(tokens, &["execute", "invoke", "run", "command", "action", "rpc"]) {
            *scores.get_mut("rpc_style").expect("rpc") += 0.5;
        }
        if method == "POST" && !has_item_path && !has_data_model {
            *scores.get_mut("rpc_style").expect("rpc") += 0.25;
        }

        let mut scored = scores
            .into_iter()
            .map(|(behavior, score)| BehaviorScore {
                behavior,
                score: score.clamp(0.0, 1.0),
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let dominant = scored
            .first()
            .map(|s| s.behavior.clone())
            .unwrap_or_else(|| "crud_ish".to_string());

        BehavioralFingerprint {
            dominant,
            scores: scored,
        }
    }

    fn normalize_behavior_scores(
        aggregate_scores: BTreeMap<String, f32>,
        endpoint_count: usize,
    ) -> BehavioralFingerprint {
        let denom = endpoint_count.max(1) as f32;
        let mut scores: Vec<BehaviorScore> = aggregate_scores
            .into_iter()
            .map(|(behavior, score)| BehaviorScore {
                behavior,
                score: (score / denom).clamp(0.0, 1.0),
            })
            .collect();
        scores.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let dominant = scores
            .first()
            .map(|s| s.behavior.clone())
            .unwrap_or_else(|| "crud_ish".to_string());
        BehavioralFingerprint { dominant, scores }
    }

    fn action_phrase(method: &str, path: &str) -> &'static str {
        let is_item = path.contains('{') || path.contains(':') || path.contains('<');
        match method {
            "GET" => {
                if is_item {
                    "Retrieve"
                } else {
                    "List"
                }
            }
            "POST" => "Create",
            "PUT" => "Replace",
            "PATCH" => "Update",
            "DELETE" => "Delete",
            "OPTIONS" => "Inspect",
            "HEAD" => "Check",
            _ => "Operate on",
        }
    }

    fn extract_path_tokens(path: &str) -> Vec<String> {
        path.split('/')
            .flat_map(|part| part.split(['-', '_']))
            .map(|part| part.trim_matches(|c: char| c == '{' || c == '}' || c == ':' || c == '<' || c == '>'))
            .filter(|part| !part.is_empty())
            .map(|part| part.to_lowercase())
            .collect()
    }

    fn tokenize_text(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(|token| token.to_lowercase())
            .collect()
    }

    fn tokenize_identifier(text: &str) -> Vec<String> {
        let with_spaces = text
            .replace(['_', '-', '.'], " ")
            .chars()
            .enumerate()
            .flat_map(|(idx, c)| {
                if idx > 0 && c.is_ascii_uppercase() {
                    vec![' ', c]
                } else {
                    vec![c]
                }
            })
            .collect::<String>();
        Self::tokenize_text(&with_spaces)
    }

    fn is_stopword(token: &str) -> bool {
        matches!(
            token,
            "api"
                | "v1"
                | "v2"
                | "and"
                | "or"
                | "the"
                | "a"
                | "an"
                | "to"
                | "for"
                | "of"
                | "by"
                | "with"
                | "in"
                | "on"
        )
    }

    fn title_case(token: &str) -> String {
        let mut chars = token.chars();
        if let Some(first) = chars.next() {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        } else {
            token.to_string()
        }
    }

    fn contains_any(tokens: &[String], needles: &[&str]) -> bool {
        tokens
            .iter()
            .any(|token| needles.iter().any(|needle| token == needle))
    }

    fn count_keyword_hits(tokens: &[String], needles: &[&str]) -> usize {
        tokens
            .iter()
            .filter(|token| needles.iter().any(|needle| token == needle))
            .count()
    }

    fn slug(label: &str) -> String {
        label
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }

    fn meaning_graph_cache_path(root: &std::path::Path) -> std::path::PathBuf {
        root.join(".repo-api").join("meaning_graph_cache.json")
    }

    fn meaning_graph_cache_key(contract: &api_core::ApiContract) -> String {
        let endpoint_fingerprint = contract
            .endpoints
            .iter()
            .map(|ep| {
                format!(
                    "{}:{}:{}:{}:{}:{}",
                    ep.id,
                    ep.method.as_str(),
                    ep.path,
                    ep.summary.as_deref().unwrap_or(""),
                    ep.operation_id.as_deref().unwrap_or(""),
                    ep.responses.len()
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        let schema_fingerprint = contract
            .schemas
            .schemas
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join("|");
        format!(
            "v1:{}:{}:{}",
            contract.version,
            endpoint_fingerprint,
            schema_fingerprint
        )
    }

    fn load_cached_meaning_graph(
        root: &std::path::Path,
        expected_cache_key: &str,
    ) -> ServiceResult<Option<ApiMeaningGraph>> {
        let cache_path = Self::meaning_graph_cache_path(root);
        if !cache_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&cache_path).map_err(|e| {
            ServiceError::internal(&format!("Unable to read meaning graph cache: {e}"))
        })?;
        let parsed: MeaningGraphCacheFile = serde_json::from_str(&content).map_err(|e| {
            ServiceError::internal(&format!("Unable to parse meaning graph cache: {e}"))
        })?;
        if parsed.cache_key != expected_cache_key {
            return Ok(None);
        }
        Ok(Some(parsed.graph))
    }

    fn save_cached_meaning_graph(
        root: &std::path::Path,
        graph: &ApiMeaningGraph,
    ) -> ServiceResult<()> {
        let cache_path = Self::meaning_graph_cache_path(root);
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ServiceError::internal(&format!("Unable to create graph cache dir: {e}"))
            })?;
        }
        let wrapper = MeaningGraphCacheFile {
            cache_key: graph.cache_key.clone(),
            graph: graph.clone(),
        };
        let content = serde_json::to_string_pretty(&wrapper).map_err(|e| {
            ServiceError::internal(&format!("Unable to serialize meaning graph cache: {e}"))
        })?;
        std::fs::write(cache_path, content)
            .map_err(|e| ServiceError::internal(&format!("Unable to persist meaning graph cache: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_core::{
        ApiContract, ApiMetadata, Confidence, EvidenceIndex, HttpMethod, RequestBodyDefinition,
        ResponseDefinition, SchemaReference, SchemaRegistry, SecurityRequirement, ServerDefinition,
    };
    use tempfile::tempdir;

    fn fake_endpoint(method: HttpMethod, path: &str, summary: Option<&str>) -> ApiEndpoint {
        ApiEndpoint {
            id: "ep-1".to_string(),
            operation_id: Some("createOrderWorkflowStep".to_string()),
            method,
            path: path.to_string(),
            summary: summary.map(|s| s.to_string()),
            parameters: vec![],
            request_bodies: vec![RequestBodyDefinition {
                content_type: "application/json".to_string(),
                required: true,
                schema: SchemaReference {
                    id: "OrderCommand".to_string(),
                },
                example: None,
            }],
            responses: vec![ResponseDefinition {
                status: 200,
                content_type: Some("application/json".to_string()),
                schema: Some(SchemaReference {
                    id: "OrderView".to_string(),
                }),
                example: None,
            }],
            security: SecurityRequirement::default(),
            confidence: Confidence::medium(),
            evidence: vec![],
        }
    }

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

    #[test]
    fn test_infer_semantic_intent_with_behavioral_fingerprint() {
        let endpoint = fake_endpoint(
            HttpMethod::POST,
            "/orders/workflow/submit",
            Some("Submit order workflow step"),
        );

        let intent = ExplorerService::infer_endpoint_semantic_intent(
            &endpoint,
            &["OrderView".to_string(), "OrderCommand".to_string()],
        );

        assert!(!intent.primary_intent.is_empty());
        assert!(!intent.capabilities.is_empty());
        assert!(!intent.domain_concepts.is_empty());
        assert!(!intent.data_models.is_empty());
        assert!(!intent.behavioral_fingerprint.dominant.is_empty());
    }

    #[test]
    fn test_meaning_graph_cache_key_changes_with_endpoint_path() {
        let mut contract = ApiContract {
            version: "v1".to_string(),
            metadata: ApiMetadata {
                title: "Test".to_string(),
                version: "1.0.0".to_string(),
                repository_root: None,
            },
            servers: vec![ServerDefinition {
                url: "http://localhost".to_string(),
            }],
            endpoints: vec![fake_endpoint(HttpMethod::GET, "/users", Some("List users"))],
            schemas: SchemaRegistry::default(),
            security_schemes: vec![],
            diagnostics: vec![],
            evidence: EvidenceIndex {
                endpoint_evidence: vec![],
                schema_evidence: vec![],
                security_evidence: vec![],
            },
        };

        let first = ExplorerService::meaning_graph_cache_key(&contract);
        contract.endpoints[0].path = "/users/{id}".to_string();
        let second = ExplorerService::meaning_graph_cache_key(&contract);
        assert_ne!(first, second);
    }
}
