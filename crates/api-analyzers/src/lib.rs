use api_core::{
    ApiParameter, Confidence, Diagnostic, DiagnosticSeverity, EndpointEvidence, EvidenceReference,
    HttpMethod, ParameterLocation, RepositoryInventory, RequestBodyDefinition, ResponseDefinition,
    SchemaReference, SecurityRequirement,
};
use api_discovery::{AnalyzerContext, AnalyzerError, AnalyzerOutput, AnalyzerSupport, ApiAnalyzer};
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::{path::Path, sync::Arc};

fn method_from_str(s: &str) -> Option<HttpMethod> {
    match s.to_uppercase().as_str() {
        "GET" => Some(HttpMethod::GET),
        "POST" => Some(HttpMethod::POST),
        "PUT" => Some(HttpMethod::PUT),
        "PATCH" => Some(HttpMethod::PATCH),
        "DELETE" => Some(HttpMethod::DELETE),
        "OPTIONS" => Some(HttpMethod::OPTIONS),
        "HEAD" => Some(HttpMethod::HEAD),
        _ => None,
    }
}

fn normalize_path(path: &str) -> String {
    let mut p = path.trim().to_string();
    if !p.starts_with('/') {
        p = format!("/{p}");
    }
    // Convert Rocket-style params (<id>) into OpenAPI-style ({id}).
    p = Regex::new(r"<([A-Za-z0-9_]+)>")
        .expect("valid rocket param regex")
        .replace_all(&p, "{$1}")
        .to_string();

    // Convert Express-style params (:id) into OpenAPI-style ({id}).
    p = Regex::new(r":([A-Za-z0-9_]+)")
        .expect("valid express param regex")
        .replace_all(&p, "{$1}")
        .to_string();

    p
}

fn add_route(
    out: &mut AnalyzerOutput,
    analyzer_id: &str,
    method: HttpMethod,
    path: String,
    file: String,
    line: usize,
    confidence: Confidence,
) {
    out.endpoint_evidence.push(EndpointEvidence {
        analyzer_id: analyzer_id.to_string(),
        method,
        path,
        operation_id: None,
        summary: None,
        parameters: Vec::new(),
        request_bodies: Vec::new(),
        responses: vec![ResponseDefinition {
            status: 200,
            content_type: Some("application/json".into()),
            schema: None,
            example: None,
        }],
        security: SecurityRequirement::default(),
        confidence,
        evidence: vec![EvidenceReference {
            file,
            line_start: Some(line),
            line_end: Some(line),
        }],
    });
}

pub struct GenericRouteAnalyzer;

fn line_from_byte_offset(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset].bytes().filter(|b| *b == b'\n').count() + 1
}

#[async_trait]
impl ApiAnalyzer for GenericRouteAnalyzer {
    fn id(&self) -> String {
        "generic-route".to_string()
    }

    fn supports(&self, _inventory: &RepositoryInventory) -> AnalyzerSupport {
        AnalyzerSupport {
            supported: true,
            reason: None,
        }
    }

    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError> {
        let mut out = AnalyzerOutput::default();
        let js_re = Regex::new(
            r#"(?m)(?:app|router)\s*\.\s*(get|post|put|patch|delete|options|head)\s*\(\s*["']([^"']+)["']"#,
        )
        .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let py_re = Regex::new(
            r#"(?m)@(?:app|router)\s*\.\s*(get|post|put|patch|delete|options|head)\s*\(\s*["']([^"']+)["']"#,
        )
        .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let axum_route_re = Regex::new(
            r#"(?s)\.route\(\s*["']([^"']+)["']\s*,\s*([^)]+?)\)"#,
        )
        .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let method_call_re = Regex::new(r#"\b(get|post|put|patch|delete|options|head)\s*\("#)
            .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let express_chain_re = Regex::new(
            r#"(?s)(?:app|router)\s*\.\s*route\(\s*["']([^"']+)["']\s*\)([^;]+)"#,
        )
        .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let rocket_attr_re = Regex::new(
            r#"(?m)#\s*\[\s*(get|post|put|patch|delete|options|head)\s*\(\s*["']([^"']+)["']"#,
        )
        .map_err(|e| AnalyzerError::Generic(e.to_string()))?;

        for sf in &context.inventory.source_files {
            let text = api_repository::read_file(&context.root, &sf.path)
                .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
            for c in js_re.captures_iter(&text) {
                if let Some(m) = method_from_str(&c[1]) {
                    let start = c.get(0).map(|mch| mch.start()).unwrap_or(0);
                    add_route(
                        &mut out,
                        "generic-route",
                        m,
                        normalize_path(&c[2]),
                        sf.path.display().to_string(),
                        line_from_byte_offset(&text, start),
                        Confidence::medium(),
                    );
                }
            }

            for c in py_re.captures_iter(&text) {
                if let Some(m) = method_from_str(&c[1]) {
                    let start = c.get(0).map(|mch| mch.start()).unwrap_or(0);
                    add_route(
                        &mut out,
                        "generic-route",
                        m,
                        normalize_path(&c[2]),
                        sf.path.display().to_string(),
                        line_from_byte_offset(&text, start),
                        Confidence::high(),
                    );
                }
            }

            for c in axum_route_re.captures_iter(&text) {
                let path = normalize_path(&c[1]);
                let route_expr = &c[2];
                let start = c.get(0).map(|mch| mch.start()).unwrap_or(0);
                let line = line_from_byte_offset(&text, start);

                for method in method_call_re.captures_iter(route_expr) {
                    if let Some(m) = method_from_str(&method[1]) {
                        add_route(
                            &mut out,
                            "generic-route",
                            m,
                            path.clone(),
                            sf.path.display().to_string(),
                            line,
                            Confidence::medium(),
                        );
                    }
                }
            }

            for c in express_chain_re.captures_iter(&text) {
                let path = normalize_path(&c[1]);
                let chain_expr = &c[2];
                let start = c.get(0).map(|mch| mch.start()).unwrap_or(0);
                let line = line_from_byte_offset(&text, start);

                for method in method_call_re.captures_iter(chain_expr) {
                    if let Some(m) = method_from_str(&method[1]) {
                        add_route(
                            &mut out,
                            "generic-route",
                            m,
                            path.clone(),
                            sf.path.display().to_string(),
                            line,
                            Confidence::medium(),
                        );
                    }
                }
            }

            for c in rocket_attr_re.captures_iter(&text) {
                if let Some(m) = method_from_str(&c[1]) {
                    let start = c.get(0).map(|mch| mch.start()).unwrap_or(0);
                    add_route(
                        &mut out,
                        "generic-route",
                        m,
                        normalize_path(&c[2]),
                        sf.path.display().to_string(),
                        line_from_byte_offset(&text, start),
                        Confidence::medium(),
                    );
                }
            }

            for (idx, line) in text.lines().enumerate() {
                if line.contains("format!(") && line.contains("route") {
                    out.diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        code: "API_DYNAMIC_ROUTE".into(),
                        message: "Unable to resolve a static route.".into(),
                        evidence: vec![EvidenceReference {
                            file: sf.path.display().to_string(),
                            line_start: Some(idx + 1),
                            line_end: Some(idx + 1),
                        }],
                        remediation: Some("Use static route definitions for analysis".into()),
                    });
                }
            }
        }
        Ok(out)
    }
}

pub struct OpenApiAnalyzer;

fn load_spec(path: &Path, content: &str) -> Result<Value, AnalyzerError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if ext == "json" {
        serde_json::from_str(content).map_err(|e| AnalyzerError::Generic(e.to_string()))
    } else {
        serde_yaml::from_str(content).map_err(|e| AnalyzerError::Generic(e.to_string()))
    }
}

#[async_trait]
impl ApiAnalyzer for OpenApiAnalyzer {
    fn id(&self) -> String {
        "openapi".to_string()
    }

    fn supports(&self, inventory: &RepositoryInventory) -> AnalyzerSupport {
        AnalyzerSupport {
            supported: !inventory.specifications.is_empty(),
            reason: None,
        }
    }

    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError> {
        let mut out = AnalyzerOutput::default();
        for spec in &context.inventory.specifications {
            let text = api_repository::read_file(&context.root, &spec.path)
                .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
            let doc = load_spec(&spec.path, &text)?;
            if let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) {
                for (path, item) in paths {
                    if let Some(op_map) = item.as_object() {
                        for (m, operation) in op_map {
                            let Some(method) = method_from_str(m) else {
                                continue;
                            };
                            let summary = operation
                                .get("summary")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string);
                            let operation_id = operation
                                .get("operationId")
                                .and_then(|v| v.as_str())
                                .map(ToString::to_string);
                            let mut responses = Vec::new();
                            if let Some(r) = operation.get("responses").and_then(|v| v.as_object())
                            {
                                for (status, rv) in r {
                                    let code = status.parse::<u16>().unwrap_or(200);
                                    let example = rv
                                        .get("content")
                                        .and_then(|c| c.get("application/json"))
                                        .and_then(|v| v.get("example"))
                                        .cloned();
                                    responses.push(ResponseDefinition {
                                        status: code,
                                        content_type: Some("application/json".into()),
                                        schema: None,
                                        example,
                                    });
                                }
                            }
                            let mut params = Vec::new();
                            if let Some(p) = operation.get("parameters").and_then(|v| v.as_array())
                            {
                                for v in p {
                                    let name = v
                                        .get("name")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("param")
                                        .to_string();
                                    let location = match v
                                        .get("in")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("query")
                                    {
                                        "path" => ParameterLocation::Path,
                                        "header" => ParameterLocation::Header,
                                        _ => ParameterLocation::Query,
                                    };
                                    let required = v
                                        .get("required")
                                        .and_then(|b| b.as_bool())
                                        .unwrap_or(false);
                                    params.push(ApiParameter {
                                        name,
                                        location,
                                        required,
                                        schema: SchemaReference {
                                            id: "unknown".into(),
                                        },
                                    });
                                }
                            }
                            let mut request_bodies = Vec::new();
                            if let Some(rb) = operation.get("requestBody") {
                                let required = rb
                                    .get("required")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let example = rb
                                    .get("content")
                                    .and_then(|c| c.get("application/json"))
                                    .and_then(|v| v.get("example"))
                                    .cloned();
                                request_bodies.push(RequestBodyDefinition {
                                    content_type: "application/json".into(),
                                    required,
                                    schema: SchemaReference {
                                        id: "unknown".into(),
                                    },
                                    example,
                                });
                            }

                            out.endpoint_evidence.push(EndpointEvidence {
                                analyzer_id: "openapi".into(),
                                method,
                                path: path.to_string(),
                                operation_id,
                                summary,
                                parameters: params,
                                request_bodies,
                                responses,
                                security: SecurityRequirement::default(),
                                confidence: Confidence::high(),
                                evidence: vec![EvidenceReference {
                                    file: spec.path.display().to_string(),
                                    line_start: None,
                                    line_end: None,
                                }],
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

pub struct ExpressAnalyzer;

#[async_trait]
impl ApiAnalyzer for ExpressAnalyzer {
    fn id(&self) -> String {
        "express".into()
    }
    fn supports(&self, inventory: &RepositoryInventory) -> AnalyzerSupport {
        let hit = inventory
            .frameworks
            .iter()
            .any(|f| matches!(f, api_core::DetectedFramework::Express));
        AnalyzerSupport {
            supported: hit,
            reason: None,
        }
    }
    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError> {
        GenericRouteAnalyzer.analyze(context).await
    }
}

pub struct FastApiAnalyzer;

#[async_trait]
impl ApiAnalyzer for FastApiAnalyzer {
    fn id(&self) -> String {
        "fastapi".into()
    }
    fn supports(&self, inventory: &RepositoryInventory) -> AnalyzerSupport {
        let hit = inventory
            .frameworks
            .iter()
            .any(|f| matches!(f, api_core::DetectedFramework::FastApi));
        AnalyzerSupport {
            supported: hit,
            reason: None,
        }
    }
    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError> {
        GenericRouteAnalyzer.analyze(context).await
    }
}

pub struct AxumAnalyzer;

#[async_trait]
impl ApiAnalyzer for AxumAnalyzer {
    fn id(&self) -> String {
        "axum".into()
    }
    fn supports(&self, inventory: &RepositoryInventory) -> AnalyzerSupport {
        let hit = inventory
            .frameworks
            .iter()
            .any(|f| matches!(f, api_core::DetectedFramework::Axum));
        AnalyzerSupport {
            supported: hit,
            reason: None,
        }
    }
    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError> {
        GenericRouteAnalyzer.analyze(context).await
    }
}

pub fn default_analyzers() -> Vec<Arc<dyn ApiAnalyzer>> {
    vec![
        Arc::new(OpenApiAnalyzer),
        Arc::new(GenericRouteAnalyzer),
        Arc::new(ExpressAnalyzer),
        Arc::new(FastApiAnalyzer),
        Arc::new(AxumAnalyzer),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[tokio::test]
    async fn discovers_express_route() {
        let root = PathBuf::from("/tmp/repo_api_analyzer_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("package.json"), "{}").expect("pkg");
        fs::write(root.join("src/routes.js"), "app.get('/users', handler)").expect("route");

        let context = api_discovery::build_context(root.clone()).expect("context");
        let out = GenericRouteAnalyzer
            .analyze(context)
            .await
            .expect("analyze");
        assert!(
            out.endpoint_evidence
                .iter()
                .any(|e| e.path.contains("/users"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn discovers_express_route_chain_methods() {
        let root = PathBuf::from("/tmp/repo_api_analyzer_chain_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("package.json"), "{}").expect("pkg");
        fs::write(
            root.join("src/routes.js"),
            "router.route('/orders').get(listOrders).post(createOrder);",
        )
        .expect("route");

        let context = api_discovery::build_context(root.clone()).expect("context");
        let out = GenericRouteAnalyzer
            .analyze(context)
            .await
            .expect("analyze");

        assert!(out
            .endpoint_evidence
            .iter()
            .any(|e| e.path == "/orders" && e.method == HttpMethod::GET));
        assert!(out
            .endpoint_evidence
            .iter()
            .any(|e| e.path == "/orders" && e.method == HttpMethod::POST));

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn discovers_fastapi_router_decorator() {
        let root = PathBuf::from("/tmp/repo_api_analyzer_fastapi_router_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("requirements.txt"), "fastapi==0.114.0").expect("deps");
        fs::write(
            root.join("src/main.py"),
            "@router.get('/v1/items')\ndef list_items():\n    return []\n",
        )
        .expect("route");

        let context = api_discovery::build_context(root.clone()).expect("context");
        let out = GenericRouteAnalyzer
            .analyze(context)
            .await
            .expect("analyze");

        assert!(out
            .endpoint_evidence
            .iter()
            .any(|e| e.path == "/v1/items" && e.method == HttpMethod::GET));

        let _ = fs::remove_dir_all(root);
    }
}
