use api_core::{ApiContract, ApiEnvironment, HttpMethod};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Replace `{{key}}` placeholders in `template` with values from `vars`.
pub fn render_template(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn method_to_reqwest(m: &HttpMethod) -> reqwest::Method {
    match m {
        HttpMethod::GET => reqwest::Method::GET,
        HttpMethod::POST => reqwest::Method::POST,
        HttpMethod::PUT => reqwest::Method::PUT,
        HttpMethod::PATCH => reqwest::Method::PATCH,
        HttpMethod::DELETE => reqwest::Method::DELETE,
        HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        HttpMethod::HEAD => reqwest::Method::HEAD,
    }
}

/// Full outcome of an executed request, including response headers - needed
/// by callers (like the desktop app) that display headers/duration to users,
/// not just the status/body the CLI prints.
pub struct RequestOutcome {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

async fn send(
    root: &std::path::Path,
    method: &HttpMethod,
    url: &str,
    path_for_history: &str,
    endpoint_operation_id: Option<&str>,
    headers: Option<&BTreeMap<String, String>>,
    body: Option<Value>,
) -> anyhow::Result<RequestOutcome> {
    let client = reqwest::Client::new();
    let mut req = client.request(method_to_reqwest(method), url);
    if let Some(headers) = headers {
        for (name, value) in headers {
            req = req.header(name, value);
        }
    }
    if let Some(b) = &body {
        req = req.json(b);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let response_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let payload = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));

    api_storage::append_request_history(
        root,
        &json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "endpoint": endpoint_operation_id,
            "method": format!("{:?}", method),
            "path": path_for_history,
            "status": status,
            "response": payload,
        }),
    )?;

    Ok(RequestOutcome {
        status,
        headers: response_headers,
        body: payload,
    })
}

pub async fn execute_endpoint_full(
    root: &std::path::Path,
    contract: &ApiContract,
    endpoint_id: &str,
    environment: &ApiEnvironment,
    headers: Option<&BTreeMap<String, String>>,
    body: Option<Value>,
) -> anyhow::Result<RequestOutcome> {
    let ep = contract
        .endpoints
        .iter()
        .find(|e| e.id == endpoint_id || e.operation_id.as_deref() == Some(endpoint_id))
        .ok_or_else(|| anyhow::anyhow!("endpoint not found"))?;

    let base_url = environment
        .variables
        .get("baseUrl")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:4010".into());
    let url = render_template(
        &format!("{{{{baseUrl}}}}{}", ep.path),
        &BTreeMap::from([(String::from("baseUrl"), base_url)]),
    );

    send(
        root,
        &ep.method,
        &url,
        &ep.path,
        ep.operation_id.as_deref(),
        headers,
        body,
    )
    .await
}

pub async fn execute_endpoint(
    root: &std::path::Path,
    contract: &ApiContract,
    endpoint_id: &str,
    environment: &ApiEnvironment,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    let outcome =
        execute_endpoint_full(root, contract, endpoint_id, environment, None, body).await?;
    Ok(json!({"status": outcome.status, "body": outcome.body}))
}

pub async fn execute_direct_full(
    root: &std::path::Path,
    method: HttpMethod,
    path: &str,
    environment: &ApiEnvironment,
    headers: Option<&BTreeMap<String, String>>,
    body: Option<Value>,
) -> anyhow::Result<RequestOutcome> {
    let base_url = environment
        .variables
        .get("baseUrl")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:4010".into());
    let url = format!("{base_url}{path}");

    send(root, &method, &url, path, None, headers, body).await
}

/// Execute a request against a fully-resolved URL (already had any `{{var}}`
/// templates substituted). Used by callers - like the desktop request
/// builder - that let the user type an arbitrary URL rather than a path
/// relative to an environment's `baseUrl`.
pub async fn execute_url_full(
    root: &std::path::Path,
    method: HttpMethod,
    url: &str,
    headers: Option<&BTreeMap<String, String>>,
    body: Option<Value>,
) -> anyhow::Result<RequestOutcome> {
    send(root, &method, url, url, None, headers, body).await
}

pub async fn execute_direct(
    root: &std::path::Path,
    method: HttpMethod,
    path: &str,
    environment: &ApiEnvironment,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    let outcome = execute_direct_full(root, method, path, environment, None, body).await?;
    Ok(json!({"status": outcome.status, "body": outcome.body}))
}
