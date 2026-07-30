use api_core::{ApiCollection, ApiContract, ApiEnvironment};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn init_layout(root: &Path) -> anyhow::Result<PathBuf> {
    let base = root.join(".repo-api");
    fs::create_dir_all(base.join("contract"))?;
    fs::create_dir_all(base.join("evidence"))?;
    fs::create_dir_all(base.join("overrides"))?;
    fs::create_dir_all(base.join("scenarios"))?;
    fs::create_dir_all(base.join("environments"))?;
    fs::create_dir_all(base.join("snapshots"))?;
    fs::create_dir_all(base.join("history"))?;
    fs::write(base.join("config.yaml"), "version: 1\n")?;
    Ok(base)
}

pub fn save_generated_contract(root: &Path, contract: &ApiContract) -> anyhow::Result<PathBuf> {
    let file = root.join(".repo-api/contract/generated.json");
    fs::create_dir_all(file.parent().expect("parent"))?;
    fs::write(&file, serde_json::to_vec_pretty(contract)?)?;
    Ok(file)
}

pub fn save_effective_contract(root: &Path, contract: &ApiContract) -> anyhow::Result<PathBuf> {
    let file = root.join(".repo-api/contract/effective.json");
    fs::create_dir_all(file.parent().expect("parent"))?;
    fs::write(&file, serde_json::to_vec_pretty(contract)?)?;
    Ok(file)
}

pub fn load_effective_contract(root: &Path) -> anyhow::Result<ApiContract> {
    Ok(serde_json::from_slice(&fs::read(
        root.join(".repo-api/contract/effective.json"),
    )?)?)
}

pub fn save_openapi(root: &Path, openapi: &serde_json::Value) -> anyhow::Result<PathBuf> {
    let file = root.join(".repo-api/contract/openapi.yaml");
    fs::create_dir_all(file.parent().expect("parent"))?;
    fs::write(&file, serde_yaml::to_string(openapi)?)?;
    Ok(file)
}

pub fn save_collection(root: &Path, collection: &ApiCollection) -> anyhow::Result<PathBuf> {
    let file = root.join(".repo-api/contract/collection.json");
    fs::create_dir_all(file.parent().expect("parent"))?;
    fs::write(&file, serde_json::to_vec_pretty(collection)?)?;
    Ok(file)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractOverrides {
    pub overrides: EndpointOverrideSet,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointOverrideSet {
    pub endpoints: BTreeMap<String, EndpointOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointOverride {
    pub summary: Option<String>,
    pub responses: BTreeMap<String, serde_json::Value>,
}

pub fn load_overrides(root: &Path) -> anyhow::Result<ContractOverrides> {
    let file = root.join(".repo-api/overrides/contract-overrides.yaml");
    if !file.exists() {
        return Ok(ContractOverrides {
            overrides: EndpointOverrideSet::default(),
        });
    }
    Ok(serde_yaml::from_str(&fs::read_to_string(file)?)?)
}

pub fn apply_overrides(contract: &mut ApiContract, overrides: &ContractOverrides) {
    for (k, ov) in &overrides.overrides.endpoints {
        if let Some(ep) = contract
            .endpoints
            .iter_mut()
            .find(|ep| format!("{} {}", ep.method.as_str().to_uppercase(), ep.path) == *k)
        {
            if let Some(summary) = &ov.summary {
                ep.summary = Some(summary.clone());
            }
        } else {
            contract.diagnostics.push(api_core::Diagnostic {
                severity: api_core::DiagnosticSeverity::Warning,
                code: "API_OVERRIDE_UNMATCHED".into(),
                message: format!("Override key '{k}' no longer matches generated contract"),
                evidence: vec![],
                remediation: Some("Update override key to current endpoint".into()),
            });
        }
    }
}

pub fn load_environments(root: &Path) -> anyhow::Result<Vec<ApiEnvironment>> {
    let file = root.join(".repo-api/environments/default.yaml");
    if !file.exists() {
        let env = ApiEnvironment {
            name: "mock".into(),
            variables: BTreeMap::from([(
                String::from("baseUrl"),
                String::from("http://127.0.0.1:4010"),
            )]),
        };
        save_environments(root, std::slice::from_ref(&env))?;
        return Ok(vec![env]);
    }
    Ok(serde_yaml::from_str(&fs::read_to_string(file)?)?)
}

pub fn save_environments(root: &Path, envs: &[ApiEnvironment]) -> anyhow::Result<()> {
    let file = root.join(".repo-api/environments/default.yaml");
    fs::create_dir_all(file.parent().expect("parent"))?;
    fs::write(file, serde_yaml::to_string(envs)?)?;
    Ok(())
}

/// A request saved for reuse (and referenced by test suites via `request_id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequest {
    pub name: String,
    pub method: String,
    /// Raw URL/path to execute; used when `endpoint_id` is absent.
    #[serde(default)]
    pub url: Option<String>,
    /// Endpoint id (or operation id) to execute against the compiled contract.
    #[serde(default)]
    pub endpoint_id: Option<String>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

fn saved_requests_dir(root: &Path) -> PathBuf {
    root.join(".repo-api/requests/saved")
}

pub fn save_request(root: &Path, request: &SavedRequest) -> anyhow::Result<PathBuf> {
    let dir = saved_requests_dir(root);
    fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}.json", request.name));
    fs::write(&file, serde_json::to_vec_pretty(request)?)?;
    Ok(file)
}

pub fn load_saved_request(root: &Path, name: &str) -> anyhow::Result<SavedRequest> {
    let file = saved_requests_dir(root).join(format!("{name}.json"));
    Ok(serde_json::from_slice(&fs::read(file)?)?)
}

pub fn list_saved_requests(root: &Path) -> anyhow::Result<Vec<SavedRequest>> {
    let dir = saved_requests_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut requests = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = fs::read(entry.path())
            && let Ok(request) = serde_json::from_slice::<SavedRequest>(&bytes)
        {
            requests.push(request);
        }
    }
    requests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(requests)
}

pub fn delete_saved_request(root: &Path, name: &str) -> anyhow::Result<bool> {
    let file = saved_requests_dir(root).join(format!("{name}.json"));
    if file.exists() {
        fs::remove_file(file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn append_request_history(root: &Path, entry: &serde_json::Value) -> anyhow::Result<()> {
    let file = root.join(".repo-api/history/requests.jsonl");
    fs::create_dir_all(file.parent().expect("parent"))?;
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(file)?;
    writeln!(f, "{}", serde_json::to_string(entry)?)?;
    Ok(())
}
