use api_core::{ApiContract, ApiSchema, HttpMethod};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockScenarioFile {
    pub version: u32,
    pub scenarios: Vec<MockScenario>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockScenario {
    pub name: String,
    pub r#match: ScenarioMatch,
    pub response: ScenarioResponse,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioMatch {
    pub method: String,
    pub path: String,
    pub body: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioResponse {
    pub status: u16,
    pub body: Value,
}

#[derive(Clone)]
pub struct RuntimeState {
    pub contract: Arc<ApiContract>,
    pub seed: u64,
    pub scenarios: Arc<Vec<MockScenario>>,
    pub stateful: bool,
}

pub fn validate_request(endpoint: &api_core::ApiEndpoint, body: Option<&Value>) -> Vec<Value> {
    let mut violations = Vec::new();
    if endpoint.request_bodies.iter().any(|rb| rb.required) && body.is_none() {
        violations.push(json!({"location":"body","rule":"required","expected":"request body"}));
    }
    if let Some(Value::Object(obj)) = body {
        for rb in &endpoint.request_bodies {
            if rb.required
                && let Some(Value::Object(example_obj)) = &rb.example
            {
                for key in example_obj.keys() {
                    if !obj.contains_key(key) {
                        violations.push(json!({"location":format!("body.{key}"),"rule":"required","expected":"property"}));
                    }
                }
            }
        }
    }
    violations
}

fn generated_value(schema: Option<&ApiSchema>, _rng: &mut StdRng) -> Value {
    match schema {
        Some(ApiSchema::Boolean) => json!(true),
        Some(ApiSchema::Integer(_)) => json!(42),
        Some(ApiSchema::Number(_)) => json!(42.5),
        Some(ApiSchema::String(s)) => {
            let format = s.format.clone().unwrap_or_default();
            match format.as_str() {
                "email" => json!("alex@example.com"),
                "uuid" => json!(uuid::Uuid::new_v4().to_string()),
                "date" => json!("2026-01-01"),
                "date-time" => json!("2026-01-01T00:00:00Z"),
                _ => json!("example-string"),
            }
        }
        Some(ApiSchema::Enum(e)) => {
            json!(e.values.first().cloned().unwrap_or_else(|| "value".into()))
        }
        Some(ApiSchema::Array(_)) => json!(["example-string"]),
        Some(ApiSchema::Object(_)) => json!({"id":"example-id"}),
        _ => json!({"ok":true}),
    }
}

pub fn resolve_response(
    state: &RuntimeState,
    method: &HttpMethod,
    path: &str,
    body: Option<&Value>,
) -> (u16, Value) {
    for scenario in state.scenarios.iter() {
        if scenario
            .r#match
            .method
            .eq_ignore_ascii_case(method.as_str())
            && scenario.r#match.path == path
        {
            if let Some(expected_body) = &scenario.r#match.body {
                if body == Some(expected_body) {
                    return (scenario.response.status, scenario.response.body.clone());
                }
                continue;
            }
            return (scenario.response.status, scenario.response.body.clone());
        }
    }

    if let Some(ep) = state
        .contract
        .endpoints
        .iter()
        .find(|e| &e.method == method && e.path == path)
        && let Some(r) = ep.responses.first()
        && let Some(example) = &r.example
    {
        return (r.status, example.clone());
    }

    let mut rng = StdRng::seed_from_u64(state.seed);
    if let Some(ep) = state
        .contract
        .endpoints
        .iter()
        .find(|e| &e.method == method && e.path == path)
        && let Some(resp) = ep.responses.first()
    {
        let schema = resp
            .schema
            .as_ref()
            .and_then(|s| state.contract.schemas.schemas.get(&s.id));
        return (resp.status, generated_value(schema, &mut rng));
    }

    (200, json!({"status":"ok"}))
}

fn method_to_core(method: &Method) -> Option<HttpMethod> {
    match *method {
        Method::GET => Some(HttpMethod::GET),
        Method::POST => Some(HttpMethod::POST),
        Method::PUT => Some(HttpMethod::PUT),
        Method::PATCH => Some(HttpMethod::PATCH),
        Method::DELETE => Some(HttpMethod::DELETE),
        Method::OPTIONS => Some(HttpMethod::OPTIONS),
        Method::HEAD => Some(HttpMethod::HEAD),
        _ => None,
    }
}

async fn catch_all(
    State(state): State<RuntimeState>,
    method: Method,
    Path(path): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let path = format!("/{path}");
    if path == "/__api/health" {
        return (StatusCode::OK, Json(json!({"status":"ok"}))).into_response();
    }
    if path == "/__api/contract.json" {
        return (
            StatusCode::OK,
            Json(serde_json::to_value(&*state.contract).unwrap_or(json!({}))),
        )
            .into_response();
    }

    let Some(core_method) = method_to_core(&method) else {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({"error":"method not supported"})),
        )
            .into_response();
    };

    let Some(endpoint) = state
        .contract
        .endpoints
        .iter()
        .find(|e| e.method == core_method && e.path == path)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"endpoint not found"})),
        )
            .into_response();
    };

    let violations = validate_request(endpoint, body.as_ref().map(|b| &b.0));
    if !violations.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "REQUEST_VALIDATION_FAILED",
                    "message": "Request does not satisfy the API contract",
                    "violations": violations
                }
            })),
        )
            .into_response();
    }

    let (status, payload) =
        resolve_response(&state, &core_method, &path, body.as_ref().map(|b| &b.0));
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        Json(payload),
    )
        .into_response()
}

pub async fn start_mock_server(
    contract: ApiContract,
    bind: SocketAddr,
    seed: u64,
    scenarios: Vec<MockScenario>,
    stateful: bool,
) -> anyhow::Result<()> {
    let state = RuntimeState {
        contract: Arc::new(contract),
        seed,
        scenarios: Arc::new(scenarios),
        stateful,
    };

    let app = Router::new()
        .route("/{*path}", any(catch_all))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_core::{
        ApiContract, ApiMetadata, Confidence, EvidenceIndex, ResponseDefinition, SchemaRegistry,
        SecurityRequirement, ServerDefinition,
    };

    fn sample_contract() -> ApiContract {
        ApiContract {
            version: "1".into(),
            metadata: ApiMetadata {
                title: "t".into(),
                version: "1".into(),
                repository_root: None,
            },
            servers: vec![ServerDefinition {
                url: "http://127.0.0.1:4010".into(),
            }],
            endpoints: vec![api_core::ApiEndpoint {
                id: "ep1".into(),
                operation_id: Some("create-user".into()),
                method: HttpMethod::POST,
                path: "/users".into(),
                summary: None,
                parameters: vec![],
                request_bodies: vec![],
                responses: vec![ResponseDefinition {
                    status: 201,
                    content_type: Some("application/json".into()),
                    schema: None,
                    example: Some(json!({"id":"usr_1"})),
                }],
                security: SecurityRequirement::default(),
                confidence: Confidence::high(),
                evidence: vec![],
            }],
            schemas: SchemaRegistry::default(),
            security_schemes: vec![],
            diagnostics: vec![],
            evidence: EvidenceIndex {
                endpoint_evidence: vec![],
                schema_evidence: vec![],
                security_evidence: vec![],
            },
        }
    }

    #[test]
    fn deterministic_response_for_same_seed() {
        let state = RuntimeState {
            contract: Arc::new(sample_contract()),
            seed: 42,
            scenarios: Arc::new(vec![]),
            stateful: false,
        };
        let a = resolve_response(&state, &HttpMethod::POST, "/users", None);
        let b = resolve_response(&state, &HttpMethod::POST, "/users", None);
        assert_eq!(a, b);
    }

    #[test]
    fn request_body_required_validation() {
        let mut contract = sample_contract();
        contract.endpoints[0].request_bodies = vec![api_core::RequestBodyDefinition {
            content_type: "application/json".into(),
            required: true,
            schema: api_core::SchemaReference {
                id: "unknown".into(),
            },
            example: Some(json!({"email":"a@example.com"})),
        }];
        let violations = validate_request(&contract.endpoints[0], None);
        assert!(!violations.is_empty());
    }
}
