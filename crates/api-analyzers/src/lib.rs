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
    p.replace(':', "{").replace("{", "{").replace("/", "/")
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
    let normalized = path.replace(":", "{").replace("{", "{");
    let normalized = Regex::new(r"\{([A-Za-z0-9_]+)")
        .unwrap()
        .replace_all(&normalized, "{$1")
        .to_string();
    let path = normalized.replace("/", "/");
    let path = path.replace("{", "{").replace("}", "}");
    let path = Regex::new(r":([A-Za-z0-9_]+)")
        .unwrap()
        .replace_all(&path, "{$1}")
        .to_string();

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
        let js_re =
            Regex::new(r#"(?:app|router)\.(get|post|put|patch|delete)\(\s*["']([^"']+)["']"#)
                .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let py_re = Regex::new(r#"@app\.(get|post|put|patch|delete)\(\s*["']([^"']+)["']"#)
            .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
        let axum_re =
            Regex::new(r#"\.route\(\s*["']([^"']+)["']\s*,\s*(get|post|put|patch|delete)\("#)
                .map_err(|e| AnalyzerError::Generic(e.to_string()))?;

        for sf in &context.inventory.source_files {
            let text = api_repository::read_file(&context.root, &sf.path)
                .map_err(|e| AnalyzerError::Generic(e.to_string()))?;
            for (idx, line) in text.lines().enumerate() {
                if let Some(c) = js_re.captures(line) {
                    if let Some(m) = method_from_str(&c[1]) {
                        add_route(
                            &mut out,
                            "generic-route",
                            m,
                            normalize_path(&c[2]),
                            sf.path.display().to_string(),
                            idx + 1,
                            Confidence::medium(),
                        );
                    }
                }
                if let Some(c) = py_re.captures(line) {
                    if let Some(m) = method_from_str(&c[1]) {
                        add_route(
                            &mut out,
                            "generic-route",
                            m,
                            normalize_path(&c[2]),
                            sf.path.display().to_string(),
                            idx + 1,
                            Confidence::high(),
                        );
                    }
                }
                if let Some(c) = axum_re.captures(line) {
                    if let Some(m) = method_from_str(&c[2]) {
                        add_route(
                            &mut out,
                            "generic-route",
                            m,
                            normalize_path(&c[1]),
                            sf.path.display().to_string(),
                            idx + 1,
                            Confidence::medium(),
                        );
                    }
                }
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
}
