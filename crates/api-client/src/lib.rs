use api_core::{ApiContract, ApiEnvironment, HttpMethod};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn render_template(template: &str, vars: &BTreeMap<String, String>) -> String {
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

pub async fn execute_endpoint(
    root: &std::path::Path,
    contract: &ApiContract,
    endpoint_id: &str,
    environment: &ApiEnvironment,
    body: Option<Value>,
) -> anyhow::Result<Value> {
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

    let client = reqwest::Client::new();
    let mut req = client.request(method_to_reqwest(&ep.method), url);
    if let Some(b) = body.clone() {
        req = req.json(&b);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let payload = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
    api_storage::append_request_history(
        root,
        &json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "endpoint": ep.operation_id,
            "method": format!("{:?}", ep.method),
            "path": ep.path,
            "status": status,
            "response": payload,
        }),
    )?;
    Ok(json!({"status": status, "body": payload}))
}

pub async fn execute_direct(
    root: &std::path::Path,
    method: HttpMethod,
    path: &str,
    environment: &ApiEnvironment,
    body: Option<Value>,
) -> anyhow::Result<Value> {
    let base_url = environment
        .variables
        .get("baseUrl")
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:4010".into());
    let url = format!("{base_url}{path}");
    let client = reqwest::Client::new();
    let mut req = client.request(method_to_reqwest(&method), &url);
    if let Some(b) = body.clone() {
        req = req.json(&b);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let payload = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
    api_storage::append_request_history(
        root,
        &json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "method": format!("{:?}", method),
            "path": path,
            "status": status,
            "response": payload,
        }),
    )?;
    Ok(json!({"status": status, "body": payload}))
}
