use api_core::{
    ApiCollection, ApiContract, ApiEndpoint, ApiEnvironment, ApiMetadata, ApiSchema, Diagnostic,
    DiagnosticSeverity, EndpointEvidence, EvidenceIndex, HeaderDefinition, HttpMethod,
    QueryParameter, RequestBody, SavedRequest, SchemaRegistry, SecurityScheme, ServerDefinition,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub fn normalize_path(path: &str) -> String {
    let mut p = path.trim().to_string();
    if !p.starts_with('/') {
        p = format!("/{p}");
    }
    p = p.replace(":", "{");
    p = p
        .split('/')
        .map(|seg| {
            if seg.contains('{') && !seg.ends_with('}') {
                format!("{seg}}}")
            } else {
                seg.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    while p.contains("//") {
        p = p.replace("//", "/");
    }
    p
}

fn endpoint_key(method: &HttpMethod, path: &str) -> String {
    format!(
        "{} {}",
        method.as_str().to_uppercase(),
        normalize_path(path)
    )
}

pub fn compile_contract(
    metadata: ApiMetadata,
    endpoint_evidence: Vec<EndpointEvidence>,
    schemas: SchemaRegistry,
    mut diagnostics: Vec<Diagnostic>,
    security_schemes: Vec<SecurityScheme>,
) -> ApiContract {
    let mut merged: HashMap<String, ApiEndpoint> = HashMap::new();
    let mut evidence_copy = Vec::new();

    for ev in endpoint_evidence {
        let key = endpoint_key(&ev.method, &ev.path);
        evidence_copy.push(ev.clone());
        if let Some(existing) = merged.get_mut(&key) {
            existing.evidence.extend(ev.evidence.clone());
            if existing.summary.is_none() {
                existing.summary = ev.summary.clone();
            }
            if existing.operation_id.is_none() {
                existing.operation_id = ev.operation_id.clone();
            }
            for p in ev.parameters {
                if !existing.parameters.iter().any(|ep| ep.name == p.name) {
                    existing.parameters.push(p);
                }
            }
            for rb in ev.request_bodies {
                if !existing
                    .request_bodies
                    .iter()
                    .any(|e| e.content_type == rb.content_type)
                {
                    existing.request_bodies.push(rb);
                }
            }
            for r in ev.responses {
                if !existing.responses.iter().any(|e| e.status == r.status) {
                    existing.responses.push(r);
                }
            }
            if ev.confidence.score > existing.confidence.score {
                existing.confidence = ev.confidence;
            }
        } else {
            merged.insert(
                key.clone(),
                ApiEndpoint {
                    id: format!("ep_{}", uuid::Uuid::new_v4().simple()),
                    operation_id: ev.operation_id,
                    method: ev.method,
                    path: normalize_path(&ev.path),
                    summary: ev.summary,
                    parameters: ev.parameters,
                    request_bodies: ev.request_bodies,
                    responses: ev.responses,
                    security: ev.security,
                    confidence: ev.confidence,
                    evidence: ev.evidence,
                },
            );
        }
    }

    for endpoint in merged.values() {
        if endpoint.responses.is_empty() {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "API_NO_RESPONSE".into(),
                message: format!("Endpoint {} has no response evidence", endpoint.path),
                evidence: endpoint.evidence.clone(),
                remediation: Some("Add response evidence or override response".into()),
            });
        }
    }

    ApiContract {
        version: "1".into(),
        metadata,
        servers: vec![ServerDefinition {
            url: "http://127.0.0.1:4010".into(),
        }],
        endpoints: merged.into_values().collect(),
        schemas,
        security_schemes,
        diagnostics,
        evidence: EvidenceIndex {
            endpoint_evidence: evidence_copy,
            schema_evidence: Vec::new(),
            security_evidence: Vec::new(),
        },
    }
}

pub fn validate_contract(contract: &ApiContract) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for ep in &contract.endpoints {
        let k = endpoint_key(&ep.method, &ep.path);
        if !seen.insert(k.clone()) {
            anyhow::bail!("duplicate endpoint {k}");
        }
    }
    Ok(())
}

pub fn to_openapi(contract: &ApiContract) -> Value {
    let mut paths: Map<String, Value> = Map::new();
    for ep in &contract.endpoints {
        let path = ep.path.clone();
        let mut params = Vec::new();
        for p in &ep.parameters {
            params.push(json!({
                "name": p.name,
                "in": match p.location { api_core::ParameterLocation::Path => "path", api_core::ParameterLocation::Query => "query", api_core::ParameterLocation::Header => "header" },
                "required": p.required,
                "schema": {"type": "string"}
            }));
        }
        let mut responses = Map::new();
        for r in &ep.responses {
            responses.insert(
                r.status.to_string(),
                json!({
                    "description": format!("HTTP {}", r.status),
                    "content": {
                        "application/json": {
                            "example": r.example
                        }
                    }
                }),
            );
        }
        let op = json!({
            "operationId": ep.operation_id,
            "summary": ep.summary,
            "parameters": params,
            "responses": Value::Object(responses),
            "x-repository-evidence": ep.evidence,
            "x-confidence": {"score": ep.confidence.score, "level": format!("{:?}", ep.confidence.level).to_lowercase()}
        });
        let entry = paths.entry(path).or_insert_with(|| json!({}));
        let map = entry.as_object_mut().expect("object path item");
        map.insert(ep.method.as_str().to_string(), op);
    }

    let mut schemas = Map::new();
    for (id, schema) in &contract.schemas.schemas {
        schemas.insert(id.clone(), schema_to_openapi(schema));
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": contract.metadata.title,
            "version": contract.metadata.version
        },
        "servers": contract.servers.iter().map(|s| json!({"url": s.url})).collect::<Vec<_>>(),
        "paths": paths,
        "components": {
            "schemas": schemas
        }
    })
}

fn schema_to_openapi(schema: &ApiSchema) -> Value {
    match schema {
        ApiSchema::Null => json!({"type": "null"}),
        ApiSchema::Boolean => json!({"type": "boolean"}),
        ApiSchema::Integer(_) => json!({"type": "integer"}),
        ApiSchema::Number(_) => json!({"type": "number"}),
        ApiSchema::String(_) => json!({"type": "string"}),
        ApiSchema::Array(_) => json!({"type": "array"}),
        ApiSchema::Object(_) => json!({"type": "object"}),
        ApiSchema::Enum(e) => json!({"type": "string", "enum": e.values}),
        ApiSchema::OneOf(v) => {
            json!({"oneOf": v.iter().map(|r| json!({"$ref": format!("#/components/schemas/{}", r.id)})).collect::<Vec<_>>()})
        }
        ApiSchema::AnyOf(v) => {
            json!({"anyOf": v.iter().map(|r| json!({"$ref": format!("#/components/schemas/{}", r.id)})).collect::<Vec<_>>()})
        }
        ApiSchema::AllOf(v) => {
            json!({"allOf": v.iter().map(|r| json!({"$ref": format!("#/components/schemas/{}", r.id)})).collect::<Vec<_>>()})
        }
        ApiSchema::Reference(r) => json!({"$ref": format!("#/components/schemas/{}", r.id)}),
        ApiSchema::Unknown => json!({}),
    }
}

pub fn to_request_collection(contract: &ApiContract) -> ApiCollection {
    ApiCollection {
        name: contract.metadata.title.clone(),
        requests: contract
            .endpoints
            .iter()
            .map(|ep| SavedRequest {
                id: format!("req_{}", uuid::Uuid::new_v4().simple()),
                name: ep.operation_id.clone().unwrap_or_else(|| {
                    format!("{} {}", ep.method.as_str().to_uppercase(), ep.path)
                }),
                method: ep.method.clone(),
                url_template: format!("{{{{baseUrl}}}}{}", ep.path),
                headers: vec![HeaderDefinition {
                    name: "Content-Type".into(),
                    value: "application/json".into(),
                }],
                query: ep
                    .parameters
                    .iter()
                    .filter(|p| matches!(p.location, api_core::ParameterLocation::Query))
                    .map(|p| QueryParameter {
                        name: p.name.clone(),
                        value: "example".into(),
                    })
                    .collect(),
                body: ep.request_bodies.first().map(|rb| RequestBody {
                    content_type: rb.content_type.clone(),
                    body: rb.example.clone().unwrap_or_else(|| json!({})),
                }),
                source_endpoint: ep.id.clone(),
            })
            .collect(),
        environments: vec![ApiEnvironment {
            name: "mock".into(),
            variables: BTreeMap::from([(
                String::from("baseUrl"),
                String::from("http://127.0.0.1:4010"),
            )]),
        }],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Modified,
    Breaking,
    NonBreaking,
    Uncertain,
}

pub fn diff_contracts(before: &ApiContract, after: &ApiContract) -> Vec<(String, DiffKind)> {
    let mut out = Vec::new();
    let mut before_map = HashMap::new();
    let mut after_map = HashMap::new();
    for ep in &before.endpoints {
        before_map.insert(endpoint_key(&ep.method, &ep.path), ep);
    }
    for ep in &after.endpoints {
        after_map.insert(endpoint_key(&ep.method, &ep.path), ep);
    }
    for k in before_map.keys() {
        if !after_map.contains_key(k) {
            out.push((k.clone(), DiffKind::Removed));
            out.push((k.clone(), DiffKind::Breaking));
        }
    }
    for k in after_map.keys() {
        if !before_map.contains_key(k) {
            out.push((k.clone(), DiffKind::Added));
            out.push((k.clone(), DiffKind::NonBreaking));
        }
    }
    for (k, b) in before_map {
        if let Some(a) = after_map.get(&k)
            && b.responses.len() != a.responses.len()
        {
            out.push((k, DiffKind::Modified));
        }
    }
    if out.is_empty() {
        out.push(("no-change".into(), DiffKind::Uncertain));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn normalizes_colon_param_paths() {
        assert_eq!(normalize_path("users/:id"), "/users/{id}");
    }
}
