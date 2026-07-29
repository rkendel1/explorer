use api_core::{ApiContract, ApiSchema, HttpMethod, StringSchema};
use api_runtime_events::{
    EventEmitter, ResponseSource, StateOperation, ValidationViolation,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use chrono::Utc;
use rand::{SeedableRng, rngs::StdRng, Rng};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockScenarioFile {
    pub version: u32,
    pub scenarios: Vec<MockScenario>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockScenario {
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub r#match: ScenarioMatch,
    pub response: ScenarioResponse,
    #[serde(default)]
    pub priority: i32,
}

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioMatch {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub headers: HashMap<String, MatchCondition>,
    pub body: Option<HashMap<String, MatchCondition>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MatchCondition {
    Equals { equals: Value },
    Contains { contains: String },
    Matches { matches: String },
    Exists { exists: bool },
    Value(Value),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: Value,
}

/// Stateful resource storage
#[derive(Debug, Default)]
pub struct ResourceState {
    pub resources: HashMap<String, HashMap<String, Value>>,
    pub counters: HashMap<String, u64>,
}

impl ResourceState {
    pub fn create(&mut self, resource_type: &str, id: &str, value: Value) {
        self.resources
            .entry(resource_type.to_string())
            .or_default()
            .insert(id.to_string(), value);
    }

    pub fn get(&self, resource_type: &str, id: &str) -> Option<&Value> {
        self.resources.get(resource_type)?.get(id)
    }

    pub fn list(&self, resource_type: &str) -> Vec<&Value> {
        self.resources
            .get(resource_type)
            .map(|r| r.values().collect())
            .unwrap_or_default()
    }

    pub fn update(&mut self, resource_type: &str, id: &str, value: Value) -> bool {
        if let Some(resources) = self.resources.get_mut(resource_type) {
            if resources.contains_key(id) {
                resources.insert(id.to_string(), value);
                return true;
            }
        }
        false
    }

    pub fn delete(&mut self, resource_type: &str, id: &str) -> bool {
        if let Some(resources) = self.resources.get_mut(resource_type) {
            return resources.remove(id).is_some();
        }
        false
    }

    pub fn reset(&mut self) {
        self.resources.clear();
        self.counters.clear();
    }

    pub fn next_id(&mut self, resource_type: &str) -> String {
        let counter = self.counters.entry(resource_type.to_string()).or_insert(0);
        *counter += 1;
        format!("{}_{}", resource_type, counter)
    }

    pub fn export(&self) -> Value {
        json!({
            "resources": self.resources,
            "counters": self.counters,
        })
    }

    pub fn import(&mut self, data: &Value) {
        if let Some(resources) = data.get("resources").and_then(|v| v.as_object()) {
            for (rt, items) in resources {
                if let Some(items_obj) = items.as_object() {
                    for (id, value) in items_obj {
                        self.create(rt, id, value.clone());
                    }
                }
            }
        }
        if let Some(counters) = data.get("counters").and_then(|v| v.as_object()) {
            for (rt, count) in counters {
                if let Some(n) = count.as_u64() {
                    self.counters.insert(rt.clone(), n);
                }
            }
        }
    }

    pub fn resource_count(&self) -> usize {
        self.resources.values().map(|r| r.len()).sum()
    }
}

#[derive(Clone)]
pub struct RuntimeState {
    pub contract: Arc<ApiContract>,
    pub seed: u64,
    pub scenarios: Arc<Vec<MockScenario>>,
    pub stateful: bool,
    pub state: Arc<RwLock<ResourceState>>,
    pub events: Option<Arc<EventEmitter>>,
    pub strict_validation: bool,
}

impl RuntimeState {
    pub fn new(contract: ApiContract, seed: u64, scenarios: Vec<MockScenario>, stateful: bool) -> Self {
        Self {
            contract: Arc::new(contract),
            seed,
            scenarios: Arc::new(scenarios),
            stateful,
            state: Arc::new(RwLock::new(ResourceState::default())),
            events: None,
            strict_validation: false,
        }
    }

    pub fn with_events(mut self, emitter: EventEmitter) -> Self {
        self.events = Some(Arc::new(emitter));
        self
    }

    pub fn with_strict_validation(mut self) -> Self {
        self.strict_validation = true;
        self
    }
}

/// Enhanced request validation with schema support
pub fn validate_request(
    endpoint: &api_core::ApiEndpoint,
    body: Option<&Value>,
    schemas: &api_core::SchemaRegistry,
) -> Vec<ValidationViolation> {
    let mut violations = Vec::new();

    // Check required body
    if endpoint.request_bodies.iter().any(|rb| rb.required) && body.is_none() {
        violations.push(ValidationViolation {
            location: "body".into(),
            rule: "required".into(),
            expected: "request body".into(),
            actual: None,
        });
        return violations;
    }

    // Validate body against schema
    if let Some(body_value) = body {
        for rb in &endpoint.request_bodies {
            if let Some(schema) = schemas.schemas.get(&rb.schema.id) {
                validate_value_against_schema("body", body_value, schema, schemas, &mut violations);
            }
            
            // Check required fields from example
            if rb.required
                && let Some(Value::Object(example_obj)) = &rb.example
                && let Value::Object(body_obj) = body_value
            {
                for key in example_obj.keys() {
                    if !body_obj.contains_key(key) {
                        violations.push(ValidationViolation {
                            location: format!("body.{key}"),
                            rule: "required".into(),
                            expected: "property".into(),
                            actual: None,
                        });
                    }
                }
            }
        }
    }

    violations
}

fn validate_value_against_schema(
    location: &str,
    value: &Value,
    schema: &ApiSchema,
    registry: &api_core::SchemaRegistry,
    violations: &mut Vec<ValidationViolation>,
) {
    match schema {
        ApiSchema::Null => {
            if !value.is_null() {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "null".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Boolean => {
            if !value.is_boolean() {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "boolean".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Integer(int_schema) => {
            if let Some(n) = value.as_i64() {
                if let Some(min) = int_schema.minimum {
                    if n < min {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "minimum".into(),
                            expected: min.to_string(),
                            actual: Some(n.to_string()),
                        });
                    }
                }
                if let Some(max) = int_schema.maximum {
                    if n > max {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "maximum".into(),
                            expected: max.to_string(),
                            actual: Some(n.to_string()),
                        });
                    }
                }
            } else {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "integer".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Number(num_schema) => {
            if let Some(n) = value.as_f64() {
                if let Some(min) = num_schema.minimum {
                    if n < min {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "minimum".into(),
                            expected: min.to_string(),
                            actual: Some(n.to_string()),
                        });
                    }
                }
                if let Some(max) = num_schema.maximum {
                    if n > max {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "maximum".into(),
                            expected: max.to_string(),
                            actual: Some(n.to_string()),
                        });
                    }
                }
            } else {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "number".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::String(str_schema) => {
            if let Some(s) = value.as_str() {
                validate_string(location, s, str_schema, violations);
            } else if !str_schema.nullable || !value.is_null() {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "string".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Array(arr_schema) => {
            if let Some(arr) = value.as_array() {
                if let Some(min) = arr_schema.min_items {
                    if arr.len() < min {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "minItems".into(),
                            expected: min.to_string(),
                            actual: Some(arr.len().to_string()),
                        });
                    }
                }
                if let Some(max) = arr_schema.max_items {
                    if arr.len() > max {
                        violations.push(ValidationViolation {
                            location: location.into(),
                            rule: "maxItems".into(),
                            expected: max.to_string(),
                            actual: Some(arr.len().to_string()),
                        });
                    }
                }
                // Validate items
                if let Some(item_schema) = registry.schemas.get(&arr_schema.items.id) {
                    for (i, item) in arr.iter().enumerate() {
                        validate_value_against_schema(
                            &format!("{location}[{i}]"),
                            item,
                            item_schema,
                            registry,
                            violations,
                        );
                    }
                }
            } else {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "array".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Object(obj_schema) => {
            if let Some(obj) = value.as_object() {
                // Check required properties
                for req in &obj_schema.required {
                    if !obj.contains_key(req) {
                        violations.push(ValidationViolation {
                            location: format!("{location}.{req}"),
                            rule: "required".into(),
                            expected: "property".into(),
                            actual: None,
                        });
                    }
                }
                // Validate properties
                for (prop_name, prop_ref) in &obj_schema.properties {
                    if let Some(prop_value) = obj.get(prop_name) {
                        if let Some(prop_schema) = registry.schemas.get(&prop_ref.id) {
                            validate_value_against_schema(
                                &format!("{location}.{prop_name}"),
                                prop_value,
                                prop_schema,
                                registry,
                                violations,
                            );
                        }
                    }
                }
            } else {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "object".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Enum(enum_schema) => {
            if let Some(s) = value.as_str() {
                if !enum_schema.values.contains(&s.to_string()) {
                    violations.push(ValidationViolation {
                        location: location.into(),
                        rule: "enum".into(),
                        expected: format!("one of: {}", enum_schema.values.join(", ")),
                        actual: Some(s.into()),
                    });
                }
            } else {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "type".into(),
                    expected: "string (enum)".into(),
                    actual: Some(json_type_name(value)),
                });
            }
        }
        ApiSchema::Reference(ref_schema) => {
            if let Some(resolved) = registry.schemas.get(&ref_schema.id) {
                validate_value_against_schema(location, value, resolved, registry, violations);
            }
        }
        ApiSchema::OneOf(variants) | ApiSchema::AnyOf(variants) => {
            let mut any_valid = false;
            for variant in variants {
                if let Some(variant_schema) = registry.schemas.get(&variant.id) {
                    let mut variant_violations = Vec::new();
                    validate_value_against_schema(
                        location,
                        value,
                        variant_schema,
                        registry,
                        &mut variant_violations,
                    );
                    if variant_violations.is_empty() {
                        any_valid = true;
                        break;
                    }
                }
            }
            if !any_valid {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "oneOf/anyOf".into(),
                    expected: "match one variant".into(),
                    actual: None,
                });
            }
        }
        ApiSchema::AllOf(variants) => {
            for variant in variants {
                if let Some(variant_schema) = registry.schemas.get(&variant.id) {
                    validate_value_against_schema(location, value, variant_schema, registry, violations);
                }
            }
        }
        ApiSchema::Unknown => {}
    }
}

fn validate_string(
    location: &str,
    value: &str,
    schema: &StringSchema,
    violations: &mut Vec<ValidationViolation>,
) {
    if let Some(min_len) = schema.min_length {
        if value.len() < min_len {
            violations.push(ValidationViolation {
                location: location.into(),
                rule: "minLength".into(),
                expected: min_len.to_string(),
                actual: Some(value.len().to_string()),
            });
        }
    }
    if let Some(max_len) = schema.max_length {
        if value.len() > max_len {
            violations.push(ValidationViolation {
                location: location.into(),
                rule: "maxLength".into(),
                expected: max_len.to_string(),
                actual: Some(value.len().to_string()),
            });
        }
    }
    if let Some(pattern) = &schema.pattern {
        if let Ok(re) = Regex::new(pattern) {
            if !re.is_match(value) {
                violations.push(ValidationViolation {
                    location: location.into(),
                    rule: "pattern".into(),
                    expected: pattern.clone(),
                    actual: Some(value.into()),
                });
            }
        }
    }
    if let Some(format) = &schema.format {
        if !validate_format(value, format) {
            violations.push(ValidationViolation {
                location: location.into(),
                rule: "format".into(),
                expected: format.clone(),
                actual: Some(value.into()),
            });
        }
    }
}

fn validate_format(value: &str, format: &str) -> bool {
    match format {
        "email" => {
            value.contains('@') && value.contains('.') && value.len() >= 5
        }
        "uuid" => {
            Uuid::parse_str(value).is_ok()
        }
        "date" => {
            // Simple YYYY-MM-DD check
            let re = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
            re.is_match(value)
        }
        "date-time" => {
            // ISO 8601 basic check
            value.contains('T') && (value.ends_with('Z') || value.contains('+') || value.contains('-'))
        }
        "uri" | "url" => {
            value.starts_with("http://") || value.starts_with("https://")
        }
        "ipv4" => {
            let re = Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").unwrap();
            re.is_match(value)
        }
        "ipv6" => {
            value.contains(':') && !value.contains('.')
        }
        _ => true, // Unknown formats pass
    }
}

fn json_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "boolean".into(),
        Value::Number(_) => "number".into(),
        Value::String(_) => "string".into(),
        Value::Array(_) => "array".into(),
        Value::Object(_) => "object".into(),
    }
}

fn generated_value(schema: Option<&ApiSchema>, rng: &mut StdRng, registry: &api_core::SchemaRegistry) -> Value {
    match schema {
        Some(ApiSchema::Boolean) => json!(rng.random::<bool>()),
        Some(ApiSchema::Integer(int_schema)) => {
            let min = int_schema.minimum.unwrap_or(0);
            let max = int_schema.maximum.unwrap_or(1000);
            if let Some(example) = int_schema.example {
                json!(example)
            } else {
                json!(rng.random_range(min..=max))
            }
        }
        Some(ApiSchema::Number(num_schema)) => {
            if let Some(example) = num_schema.example {
                json!(example)
            } else {
                json!(42.5)
            }
        }
        Some(ApiSchema::String(s)) => {
            if let Some(example) = &s.example {
                return json!(example);
            }
            let format = s.format.clone().unwrap_or_default();
            match format.as_str() {
                "email" => json!("alex@example.com"),
                "uuid" => json!(Uuid::new_v4().to_string()),
                "date" => json!(Utc::now().format("%Y-%m-%d").to_string()),
                "date-time" => json!(Utc::now().to_rfc3339()),
                "uri" | "url" => json!("https://example.com/resource"),
                "ipv4" => json!("192.168.1.1"),
                "ipv6" => json!("::1"),
                _ => json!("example-string"),
            }
        }
        Some(ApiSchema::Enum(e)) => {
            let idx = rng.random_range(0..e.values.len().max(1));
            json!(e.values.get(idx).cloned().unwrap_or_else(|| "value".into()))
        }
        Some(ApiSchema::Array(arr)) => {
            if let Some(item_schema) = registry.schemas.get(&arr.items.id) {
                let count = arr.min_items.unwrap_or(1);
                let items: Vec<Value> = (0..count)
                    .map(|_| generated_value(Some(item_schema), rng, registry))
                    .collect();
                json!(items)
            } else {
                json!(["example-item"])
            }
        }
        Some(ApiSchema::Object(obj)) => {
            let mut result = serde_json::Map::new();
            for (prop_name, prop_ref) in &obj.properties {
                let prop_schema = registry.schemas.get(&prop_ref.id);
                result.insert(prop_name.clone(), generated_value(prop_schema, rng, registry));
            }
            Value::Object(result)
        }
        Some(ApiSchema::Reference(r)) => {
            if let Some(resolved) = registry.schemas.get(&r.id) {
                generated_value(Some(resolved), rng, registry)
            } else {
                json!({"id": "example-id"})
            }
        }
        Some(ApiSchema::OneOf(variants)) | Some(ApiSchema::AnyOf(variants)) => {
            if let Some(first) = variants.first() {
                if let Some(variant_schema) = registry.schemas.get(&first.id) {
                    return generated_value(Some(variant_schema), rng, registry);
                }
            }
            json!({})
        }
        Some(ApiSchema::AllOf(variants)) => {
            let mut result = serde_json::Map::new();
            for variant in variants {
                if let Some(variant_schema) = registry.schemas.get(&variant.id) {
                    if let Value::Object(obj) = generated_value(Some(variant_schema), rng, registry) {
                        for (k, v) in obj {
                            result.insert(k, v);
                        }
                    }
                }
            }
            Value::Object(result)
        }
        _ => json!({"ok": true}),
    }
}

/// Response template engine for scenario responses
pub struct TemplateEngine {
    seed: u64,
}

impl TemplateEngine {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn render(&self, template: &Value, request: &RequestContext) -> Value {
        match template {
            Value::String(s) => Value::String(self.render_string(s, request)),
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| self.render(v, request)).collect())
            }
            Value::Object(obj) => {
                let mut result = serde_json::Map::new();
                for (k, v) in obj {
                    result.insert(k.clone(), self.render(v, request));
                }
                Value::Object(result)
            }
            other => other.clone(),
        }
    }

    fn render_string(&self, template: &str, request: &RequestContext) -> String {
        let mut result = template.to_string();
        let mut rng = StdRng::seed_from_u64(self.seed);

        // Replace template expressions
        let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();
        
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let expr = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            self.evaluate_expression(expr, request, &mut rng)
        }).to_string();

        result
    }

    fn evaluate_expression(&self, expr: &str, request: &RequestContext, rng: &mut StdRng) -> String {
        let parts: Vec<&str> = expr.split('.').collect();
        if parts.is_empty() {
            return expr.into();
        }

        match parts[0] {
            "request" if parts.len() >= 2 => {
                match parts[1] {
                    "path" if parts.len() >= 3 => {
                        request.path_params.get(parts[2]).cloned().unwrap_or_default()
                    }
                    "query" if parts.len() >= 3 => {
                        request.query_params.get(parts[2]).cloned().unwrap_or_default()
                    }
                    "header" if parts.len() >= 3 => {
                        request.headers.get(parts[2]).cloned().unwrap_or_default()
                    }
                    "body" if parts.len() >= 3 => {
                        if let Some(body) = &request.body {
                            get_json_path(body, &parts[2..]).unwrap_or_default()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                }
            }
            "random" if parts.len() >= 2 => {
                match parts[1] {
                    "uuid" => Uuid::new_v4().to_string(),
                    "integer" => rng.random_range(0..=1000).to_string(),
                    _ => String::new(),
                }
            }
            "now" if parts.len() >= 2 => {
                match parts[1] {
                    "iso8601" => Utc::now().to_rfc3339(),
                    _ => Utc::now().to_string(),
                }
            }
            "runtime" if parts.len() >= 2 => {
                match parts[1] {
                    "seed" => self.seed.to_string(),
                    _ => String::new(),
                }
            }
            _ => expr.into(),
        }
    }
}

fn get_json_path(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    match current {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => Some(current.to_string()),
    }
}

/// Request context for template rendering
pub struct RequestContext {
    pub method: HttpMethod,
    pub path: String,
    pub path_params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Option<Value>,
}

/// Match a scenario against a request
fn match_scenario(scenario: &MockScenario, request: &RequestContext) -> bool {
    if !scenario.enabled {
        return false;
    }

    // Match method
    if !scenario.r#match.method.eq_ignore_ascii_case(request.method.as_str()) {
        return false;
    }

    // Match path (handle path parameters)
    if !match_path(&scenario.r#match.path, &request.path) {
        return false;
    }

    // Match headers
    for (header_name, condition) in &scenario.r#match.headers {
        let header_value = request.headers.get(&header_name.to_lowercase());
        if !match_condition(condition, header_value.map(|s| s.as_str())) {
            return false;
        }
    }

    // Match body
    if let Some(body_conditions) = &scenario.r#match.body {
        if let Some(body) = &request.body {
            for (field_path, condition) in body_conditions {
                let field_value = get_json_path(body, &field_path.split('.').collect::<Vec<_>>());
                if !match_condition(condition, field_value.as_deref()) {
                    return false;
                }
            }
        } else {
            return false;
        }
    }

    true
}

fn match_path(pattern: &str, actual: &str) -> bool {
    // Simple path matching with {param} support
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let actual_segments: Vec<&str> = actual.split('/').filter(|s| !s.is_empty()).collect();

    if pattern_segments.len() != actual_segments.len() {
        return false;
    }

    for (p, a) in pattern_segments.iter().zip(actual_segments.iter()) {
        if p.starts_with('{') && p.ends_with('}') {
            // Parameter segment - matches anything
            continue;
        }
        if p != a {
            return false;
        }
    }

    true
}

fn match_condition(condition: &MatchCondition, value: Option<&str>) -> bool {
    match condition {
        MatchCondition::Equals { equals } => {
            value.map(|v| {
                match equals {
                    Value::String(s) => v == s,
                    Value::Number(n) => v == &n.to_string(),
                    Value::Bool(b) => v == &b.to_string(),
                    _ => false,
                }
            }).unwrap_or(false)
        }
        MatchCondition::Contains { contains } => {
            value.map(|v| v.contains(contains)).unwrap_or(false)
        }
        MatchCondition::Matches { matches } => {
            if let Ok(re) = Regex::new(matches) {
                value.map(|v| re.is_match(v)).unwrap_or(false)
            } else {
                false
            }
        }
        MatchCondition::Exists { exists } => {
            value.is_some() == *exists
        }
        MatchCondition::Value(v) => {
            value.map(|val| {
                match v {
                    Value::String(s) => val == s,
                    _ => false,
                }
            }).unwrap_or(false)
        }
    }
}

/// Find matching scenario with highest priority
fn find_matching_scenario<'a>(
    scenarios: &'a [MockScenario],
    request: &RequestContext,
) -> Option<&'a MockScenario> {
    let mut matches: Vec<&MockScenario> = scenarios
        .iter()
        .filter(|s| match_scenario(s, request))
        .collect();

    // Sort by priority (higher first), then by specificity
    matches.sort_by(|a, b| b.priority.cmp(&a.priority));

    matches.first().copied()
}

pub fn resolve_response(
    state: &RuntimeState,
    request: &RequestContext,
) -> (u16, Value, ResponseSource, Option<String>) {
    let template_engine = TemplateEngine::new(state.seed);

    // Check scenarios first
    if let Some(scenario) = find_matching_scenario(&state.scenarios, request) {
        let response_body = template_engine.render(&scenario.response.body, request);
        return (
            scenario.response.status,
            response_body,
            ResponseSource::Scenario,
            Some(scenario.id.clone()),
        );
    }

    // Check contract examples
    if let Some(ep) = state
        .contract
        .endpoints
        .iter()
        .find(|e| e.method == request.method && match_path(&e.path, &request.path))
        && let Some(r) = ep.responses.first()
        && let Some(example) = &r.example
    {
        return (r.status, example.clone(), ResponseSource::ContractExample, None);
    }

    // Generate from schema
    let mut rng = StdRng::seed_from_u64(state.seed);
    if let Some(ep) = state
        .contract
        .endpoints
        .iter()
        .find(|e| e.method == request.method && match_path(&e.path, &request.path))
        && let Some(resp) = ep.responses.first()
    {
        let schema = resp
            .schema
            .as_ref()
            .and_then(|s| state.contract.schemas.schemas.get(&s.id));
        return (
            resp.status,
            generated_value(schema, &mut rng, &state.contract.schemas),
            ResponseSource::SchemaGenerated,
            None,
        );
    }

    (200, json!({"status": "ok"}), ResponseSource::Fallback, None)
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

/// Extract path parameters from pattern matching
fn extract_path_params(pattern: &str, actual: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let actual_segments: Vec<&str> = actual.split('/').filter(|s| !s.is_empty()).collect();

    for (p, a) in pattern_segments.iter().zip(actual_segments.iter()) {
        if p.starts_with('{') && p.ends_with('}') {
            let name = &p[1..p.len() - 1];
            params.insert(name.to_string(), a.to_string());
        }
    }

    params
}

/// Infer resource type from path
fn infer_resource_type(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }
    
    // Look for collection patterns like /users, /users/{id}
    let first = segments[0];
    if !first.starts_with('{') {
        return Some(first.to_string());
    }
    None
}

/// Check if path is a collection endpoint
fn is_collection_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() == 1 {
        return !segments[0].starts_with('{');
    }
    false
}

/// Check if path is a resource endpoint
fn is_resource_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() == 2 {
        return !segments[0].starts_with('{') && segments[1].starts_with('{');
    }
    false
}

async fn handle_stateful_request(
    state: &RuntimeState,
    request: &RequestContext,
    resource_state: &Arc<RwLock<ResourceState>>,
) -> Option<(u16, Value)> {
    let resource_type = infer_resource_type(&request.path)?;
    let mut rs = resource_state.write().await;

    match request.method {
        HttpMethod::POST if is_collection_path(&request.path) => {
            // Create resource
            if let Some(body) = &request.body {
                let id = rs.next_id(&resource_type);
                let mut resource = body.clone();
                if let Value::Object(ref mut obj) = resource {
                    obj.insert("id".into(), json!(id.clone()));
                }
                rs.create(&resource_type, &id, resource.clone());
                
                if let Some(events) = &state.events {
                    events.state_changed("req", &resource_type, &id, StateOperation::Created);
                }
                
                return Some((201, resource));
            }
        }
        HttpMethod::GET if is_collection_path(&request.path) => {
            // List resources
            let items: Vec<Value> = rs.list(&resource_type).into_iter().cloned().collect();
            return Some((200, json!(items)));
        }
        HttpMethod::GET if is_resource_path(&request.path) => {
            // Get single resource
            let path_params = extract_path_params(&request.path, &request.path);
            if let Some(id) = path_params.values().next() {
                if let Some(resource) = rs.get(&resource_type, id) {
                    return Some((200, resource.clone()));
                }
                return Some((404, json!({"error": "not_found"})));
            }
        }
        HttpMethod::PUT if is_resource_path(&request.path) => {
            // Replace resource
            let path_params = extract_path_params(&request.path, &request.path);
            if let Some(id) = path_params.values().next() {
                if let Some(body) = &request.body {
                    if rs.update(&resource_type, id, body.clone()) {
                        if let Some(events) = &state.events {
                            events.state_changed("req", &resource_type, id, StateOperation::Replaced);
                        }
                        return Some((200, body.clone()));
                    }
                    return Some((404, json!({"error": "not_found"})));
                }
            }
        }
        HttpMethod::PATCH if is_resource_path(&request.path) => {
            // Update resource
            let path_params = extract_path_params(&request.path, &request.path);
            if let Some(id) = path_params.values().next() {
                if let Some(existing) = rs.get(&resource_type, id).cloned() {
                    if let (Value::Object(mut existing_obj), Some(Value::Object(patch))) = (existing, &request.body) {
                        for (k, v) in patch {
                            existing_obj.insert(k.clone(), v.clone());
                        }
                        let updated = Value::Object(existing_obj);
                        rs.update(&resource_type, id, updated.clone());
                        if let Some(events) = &state.events {
                            events.state_changed("req", &resource_type, id, StateOperation::Updated);
                        }
                        return Some((200, updated));
                    }
                }
                return Some((404, json!({"error": "not_found"})));
            }
        }
        HttpMethod::DELETE if is_resource_path(&request.path) => {
            // Delete resource
            let path_params = extract_path_params(&request.path, &request.path);
            if let Some(id) = path_params.values().next() {
                if rs.delete(&resource_type, id) {
                    if let Some(events) = &state.events {
                        events.state_changed("req", &resource_type, id, StateOperation::Deleted);
                    }
                    return Some((204, json!(null)));
                }
                return Some((404, json!({"error": "not_found"})));
            }
        }
        _ => {}
    }

    None
}

async fn catch_all(
    State(state): State<RuntimeState>,
    method: Method,
    Path(path): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let start = Instant::now();
    let request_id = Uuid::new_v4().to_string();
    let path = format!("/{path}");

    // Internal API endpoints
    if path == "/__api/health" {
        return (StatusCode::OK, Json(json!({"status": "ok"}))).into_response();
    }
    if path == "/__api/contract.json" {
        return Json(serde_json::to_value(&*state.contract).unwrap_or(json!({}))).into_response();
    }
    if path == "/__api/openapi.json" {
        let openapi = api_compiler::to_openapi(&state.contract);
        return Json(openapi).into_response();
    }
    if path == "/__api/state" && method == Method::GET {
        let rs = state.state.read().await;
        return Json(rs.export()).into_response();
    }
    if path == "/__api/state/reset" && method == Method::POST {
        let mut rs = state.state.write().await;
        rs.reset();
        return Json(json!({"status": "reset"})).into_response();
    }
    if path == "/__api/state/export" && method == Method::GET {
        let rs = state.state.read().await;
        return Json(rs.export()).into_response();
    }
    if path == "/__api/state/import" && method == Method::POST {
        if let Some(Json(data)) = body {
            let mut rs = state.state.write().await;
            rs.import(&data);
            return Json(json!({"status": "imported"})).into_response();
        }
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "body required"}))).into_response();
    }

    let Some(core_method) = method_to_core(&method) else {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({"error": "method not supported"})),
        )
            .into_response();
    };

    // Emit request received event
    if let Some(events) = &state.events {
        events.request_received(
            &request_id,
            core_method.clone(),
            path.clone(),
            vec![],
            body.as_ref().map(|b| serde_json::to_string(&b.0).unwrap_or_default()),
        );
    }

    // Find matching endpoint
    let endpoint = state
        .contract
        .endpoints
        .iter()
        .find(|e| e.method == core_method && match_path(&e.path, &path));

    // Build request context
    let path_params = endpoint
        .map(|ep| extract_path_params(&ep.path, &path))
        .unwrap_or_default();

    let request_context = RequestContext {
        method: core_method.clone(),
        path: path.clone(),
        path_params,
        query_params: HashMap::new(),
        headers: HashMap::new(),
        body: body.as_ref().map(|b| b.0.clone()),
    };

    // Validate request if endpoint found
    if let Some(ep) = endpoint {
        let violations = validate_request(ep, body.as_ref().map(|b| &b.0), &state.contract.schemas);
        if !violations.is_empty() {
            if let Some(events) = &state.events {
                events.validation_failed(&request_id, Some(&ep.id), violations.clone());
            }
            
            let violation_json: Vec<Value> = violations
                .iter()
                .map(|v| json!({
                    "location": v.location,
                    "rule": v.rule,
                    "expected": v.expected,
                    "actual": v.actual,
                }))
                .collect();

            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "REQUEST_VALIDATION_FAILED",
                        "message": "Request does not satisfy the API contract",
                        "violations": violation_json
                    }
                })),
            )
                .into_response();
        }

        if let Some(events) = &state.events {
            events.request_validated(&request_id, &ep.id, true);
        }
    }

    // Handle stateful mode
    if state.stateful {
        if let Some((status, response)) = handle_stateful_request(&state, &request_context, &state.state).await {
            let duration = start.elapsed().as_millis() as u64;
            if let Some(events) = &state.events {
                events.response_generated(&request_id, ResponseSource::StatefulResource, None);
                events.response_sent(&request_id, status, duration, Some(response.to_string().len()));
            }
            return (
                StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
                Json(response),
            )
                .into_response();
        }
    }

    // Resolve response (scenarios, examples, or generated)
    let (status, payload, source, scenario_id) = resolve_response(&state, &request_context);

    // Emit events
    if let Some(events) = &state.events {
        if let Some(sid) = &scenario_id {
            events.scenario_matched(&request_id, sid, sid);
        }
        events.response_generated(&request_id, source, None);
        let duration = start.elapsed().as_millis() as u64;
        events.response_sent(&request_id, status, duration, Some(payload.to_string().len()));
    }

    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        Json(payload),
    )
        .into_response()
}

async fn handle_state_endpoint(
    State(state): State<RuntimeState>,
    method: Method,
) -> Response {
    match method {
        Method::GET => {
            let rs = state.state.read().await;
            Json(rs.export()).into_response()
        }
        _ => (StatusCode::METHOD_NOT_ALLOWED, Json(json!({"error": "method not allowed"}))).into_response()
    }
}

pub async fn start_mock_server(
    contract: ApiContract,
    bind: SocketAddr,
    seed: u64,
    scenarios: Vec<MockScenario>,
    stateful: bool,
) -> anyhow::Result<()> {
    let state = RuntimeState::new(contract, seed, scenarios, stateful);

    let app = Router::new()
        .route("/{*path}", any(catch_all))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start mock server with event emitter
pub async fn start_mock_server_with_events(
    contract: ApiContract,
    bind: SocketAddr,
    seed: u64,
    scenarios: Vec<MockScenario>,
    stateful: bool,
    events: EventEmitter,
) -> anyhow::Result<()> {
    let state = RuntimeState::new(contract, seed, scenarios, stateful)
        .with_events(events);

    let app = Router::new()
        .route("/{*path}", any(catch_all))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Load scenarios from a file
pub fn load_scenarios(path: &std::path::Path) -> anyhow::Result<Vec<MockScenario>> {
    let content = std::fs::read_to_string(path)?;
    let file: MockScenarioFile = serde_yaml::from_str(&content)?;
    Ok(file.scenarios)
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
        let state = RuntimeState::new(sample_contract(), 42, vec![], false);
        let request = RequestContext {
            method: HttpMethod::POST,
            path: "/users".into(),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
        };
        let a = resolve_response(&state, &request);
        let b = resolve_response(&state, &request);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
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
        let violations = validate_request(&contract.endpoints[0], None, &contract.schemas);
        assert!(!violations.is_empty());
    }

    #[test]
    fn scenario_matching() {
        let scenario = MockScenario {
            id: "s1".into(),
            name: "Test".into(),
            enabled: true,
            r#match: ScenarioMatch {
                method: "POST".into(),
                path: "/users".into(),
                headers: HashMap::new(),
                body: None,
            },
            response: ScenarioResponse {
                status: 201,
                headers: HashMap::new(),
                body: json!({"id": "usr_created"}),
            },
            priority: 0,
        };
        let request = RequestContext {
            method: HttpMethod::POST,
            path: "/users".into(),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
        };
        assert!(match_scenario(&scenario, &request));
    }

    #[test]
    fn template_rendering() {
        let engine = TemplateEngine::new(42);
        let request = RequestContext {
            method: HttpMethod::POST,
            path: "/users".into(),
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: Some(json!({"email": "test@example.com"})),
        };
        let template = json!({"email": "{{request.body.email}}"});
        let result = engine.render(&template, &request);
        assert_eq!(result, json!({"email": "test@example.com"}));
    }

    #[test]
    fn path_parameter_extraction() {
        let params = extract_path_params("/users/{id}", "/users/123");
        assert_eq!(params.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn resource_state_crud() {
        let mut state = ResourceState::default();
        
        // Create
        state.create("users", "u1", json!({"name": "Alice"}));
        assert!(state.get("users", "u1").is_some());
        
        // List
        let list = state.list("users");
        assert_eq!(list.len(), 1);
        
        // Update
        state.update("users", "u1", json!({"name": "Bob"}));
        assert_eq!(state.get("users", "u1").unwrap().get("name"), Some(&json!("Bob")));
        
        // Delete
        state.delete("users", "u1");
        assert!(state.get("users", "u1").is_none());
    }

    #[test]
    fn format_validation() {
        assert!(validate_format("test@example.com", "email"));
        assert!(!validate_format("invalid", "email"));
        
        assert!(validate_format("550e8400-e29b-41d4-a716-446655440000", "uuid"));
        assert!(!validate_format("not-a-uuid", "uuid"));
        
        assert!(validate_format("2026-01-01", "date"));
        assert!(!validate_format("01-01-2026", "date"));
    }

    #[test]
    fn match_path_with_params() {
        assert!(match_path("/users/{id}", "/users/123"));
        assert!(match_path("/users/{id}/posts/{postId}", "/users/123/posts/456"));
        assert!(!match_path("/users/{id}", "/users/123/extra"));
        assert!(!match_path("/users", "/posts"));
    }
}
