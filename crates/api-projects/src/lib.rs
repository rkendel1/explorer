use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryReference {
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractReference {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentReference {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultReference {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowReference {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub name: String,
    pub environment: String,
    pub runtime_target: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiProject {
    pub id: String,
    pub name: String,
    pub repository: RepositoryReference,
    pub contract: ContractReference,
    pub environments: Vec<EnvironmentReference>,
    pub vault: VaultReference,
    pub workflows: Vec<WorkflowReference>,
    pub runtime_profiles: Vec<RuntimeProfile>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectCatalog {
    pub projects: BTreeMap<String, ApiProject>,
}

fn project_file(root: &Path) -> std::path::PathBuf {
    root.join(".repo-api/project.json")
}

pub fn load_project(root: &Path) -> anyhow::Result<Option<ApiProject>> {
    let file = project_file(root);
    if !file.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(file)?)?))
}

pub fn save_project(root: &Path, project: &ApiProject) -> anyhow::Result<()> {
    let file = project_file(root);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(file, serde_json::to_vec_pretty(project)?)?;
    Ok(())
}

pub fn create_project(root: &Path, name: impl Into<String>) -> anyhow::Result<ApiProject> {
    let root_display = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let project = ApiProject {
        id: format!("proj_{}", Uuid::new_v4().simple()),
        name: name.into(),
        repository: RepositoryReference {
            root: root_display.display().to_string(),
        },
        contract: ContractReference {
            path: ".repo-api/contract/effective.json".into(),
        },
        environments: vec![EnvironmentReference {
            name: "mock".into(),
        }],
        vault: VaultReference {
            path: ".repo-api/vault/encrypted.db".into(),
        },
        workflows: vec![],
        runtime_profiles: default_runtime_profiles(),
        created_at: Utc::now(),
    };
    save_project(root, &project)?;
    Ok(project)
}

pub fn default_runtime_profiles() -> Vec<RuntimeProfile> {
    vec![
        RuntimeProfile {
            name: "Development".into(),
            environment: "mock".into(),
            runtime_target: "http://127.0.0.1:4010".into(),
            requires_confirmation: false,
        },
        RuntimeProfile {
            name: "Mock".into(),
            environment: "mock".into(),
            runtime_target: "http://127.0.0.1:4010".into(),
            requires_confirmation: false,
        },
        RuntimeProfile {
            name: "Testing".into(),
            environment: "mock".into(),
            runtime_target: "http://127.0.0.1:4010".into(),
            requires_confirmation: false,
        },
        RuntimeProfile {
            name: "Staging".into(),
            environment: "staging".into(),
            runtime_target: "https://staging.example.com".into(),
            requires_confirmation: true,
        },
        RuntimeProfile {
            name: "Production".into(),
            environment: "production".into(),
            runtime_target: "https://api.example.com".into(),
            requires_confirmation: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_reload_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = create_project(dir.path(), "My API").expect("create project");
        assert_eq!(project.name, "My API");

        let loaded = load_project(dir.path())
            .expect("load")
            .expect("project exists");
        assert_eq!(loaded.id, project.id);
        assert_eq!(loaded.vault.path, ".repo-api/vault/encrypted.db");
    }
}
