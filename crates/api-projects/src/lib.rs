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

/// Environment safety classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentSafety {
    /// Safe environment - no confirmation required
    #[default]
    Safe,
    /// Caution - confirmation recommended for mutations
    Caution,
    /// Production - confirmation required for all mutations
    Production,
}

impl EnvironmentSafety {
    /// Check if confirmation is required for a given HTTP method
    pub fn requires_confirmation(&self, method: &str) -> bool {
        match self {
            Self::Safe => false,
            Self::Caution => {
                let m = method.to_uppercase();
                matches!(m.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
            }
            Self::Production => {
                let m = method.to_uppercase();
                matches!(m.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
            }
        }
    }
}

/// Runtime target type
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RuntimeTarget {
    /// Local mock runtime
    MockRuntime,
    /// Local development server (process management deferred)
    LocalServer,
    /// Remote HTTP endpoint
    RemoteHttp { url: String },
}

impl Default for RuntimeTarget {
    fn default() -> Self {
        Self::MockRuntime
    }
}

impl<'de> serde::Deserialize<'de> for RuntimeTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct RuntimeTargetVisitor;

        impl<'de> Visitor<'de> for RuntimeTargetVisitor {
            type Value = RuntimeTarget;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string URL or a runtime target object")
            }

            // Handle legacy string format: "http://127.0.0.1:4010"
            fn visit_str<E>(self, value: &str) -> Result<RuntimeTarget, E>
            where
                E: de::Error,
            {
                if value.starts_with("http") {
                    Ok(RuntimeTarget::RemoteHttp { url: value.to_string() })
                } else if value == "mock" || value == "mock_runtime" {
                    Ok(RuntimeTarget::MockRuntime)
                } else if value == "local" || value == "local_server" {
                    Ok(RuntimeTarget::LocalServer)
                } else {
                    // Default to remote HTTP
                    Ok(RuntimeTarget::RemoteHttp { url: value.to_string() })
                }
            }

            // Handle new object format
            fn visit_map<M>(self, mut map: M) -> Result<RuntimeTarget, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut target_type: Option<String> = None;
                let mut url: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "mock_runtime" => {
                            let _: serde_json::Value = map.next_value()?;
                            return Ok(RuntimeTarget::MockRuntime);
                        }
                        "local_server" => {
                            let _: serde_json::Value = map.next_value()?;
                            return Ok(RuntimeTarget::LocalServer);
                        }
                        "remote_http" => {
                            #[derive(serde::Deserialize)]
                            struct RemoteHttp {
                                url: String,
                            }
                            let rh: RemoteHttp = map.next_value()?;
                            return Ok(RuntimeTarget::RemoteHttp { url: rh.url });
                        }
                        "url" => {
                            url = Some(map.next_value()?);
                        }
                        "type" => {
                            target_type = Some(map.next_value()?);
                        }
                        _ => {
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                // Handle flat object format with "type" field
                match target_type.as_deref() {
                    Some("mock_runtime") | Some("mock") => Ok(RuntimeTarget::MockRuntime),
                    Some("local_server") | Some("local") => Ok(RuntimeTarget::LocalServer),
                    Some("remote_http") | Some("remote") => {
                        let url = url.unwrap_or_default();
                        Ok(RuntimeTarget::RemoteHttp { url })
                    }
                    _ => {
                        if let Some(u) = url {
                            Ok(RuntimeTarget::RemoteHttp { url: u })
                        } else {
                            Ok(RuntimeTarget::MockRuntime)
                        }
                    }
                }
            }
        }

        deserializer.deserialize_any(RuntimeTargetVisitor)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub target: RuntimeTarget,
    pub safety: EnvironmentSafety,
}

fn default_profile_id() -> String {
    format!("profile_{}", uuid::Uuid::new_v4().simple())
}

/// Intermediate struct for deserializing RuntimeProfile (handles legacy format)
#[derive(Deserialize)]
struct RuntimeProfileRaw {
    #[serde(default = "default_profile_id")]
    id: String,
    name: String,
    environment: String,
    #[serde(default)]
    target: Option<RuntimeTarget>,
    #[serde(default)]
    safety: Option<EnvironmentSafety>,
    // Legacy fields
    #[serde(default)]
    runtime_target: Option<String>,
    #[serde(default)]
    requires_confirmation: Option<bool>,
}

impl<'de> serde::Deserialize<'de> for RuntimeProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RuntimeProfileRaw::deserialize(deserializer)?;
        
        // Determine target - prefer new format, fall back to legacy
        let target = if let Some(t) = raw.target {
            t
        } else if let Some(url) = raw.runtime_target {
            if url.starts_with("http") {
                RuntimeTarget::RemoteHttp { url }
            } else {
                RuntimeTarget::MockRuntime
            }
        } else {
            RuntimeTarget::MockRuntime
        };

        // Determine safety - prefer new format, fall back to legacy
        let safety = if let Some(s) = raw.safety {
            s
        } else if raw.requires_confirmation == Some(true) {
            EnvironmentSafety::Production
        } else {
            EnvironmentSafety::Safe
        };

        Ok(RuntimeProfile {
            id: raw.id,
            name: raw.name,
            environment: raw.environment,
            target,
            safety,
        })
    }
}

impl RuntimeProfile {
    /// Check if this profile requires confirmation for a given HTTP method
    pub fn requires_confirmation(&self, method: &str) -> bool {
        self.safety.requires_confirmation(method)
    }
}

#[allow(dead_code)]
/// Legacy runtime profile for backwards compatibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyRuntimeProfile {
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
    #[serde(default)]
    pub active_environment: Option<String>,
    #[serde(default)]
    pub active_runtime_profile: Option<String>,
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
        active_environment: Some("mock".into()),
        active_runtime_profile: Some("profile_mock".into()),
    };
    save_project(root, &project)?;
    Ok(project)
}

pub fn default_runtime_profiles() -> Vec<RuntimeProfile> {
    vec![
        RuntimeProfile {
            id: "profile_mock".into(),
            name: "Mock".into(),
            environment: "mock".into(),
            target: RuntimeTarget::MockRuntime,
            safety: EnvironmentSafety::Safe,
        },
        RuntimeProfile {
            id: "profile_development".into(),
            name: "Development".into(),
            environment: "development".into(),
            target: RuntimeTarget::RemoteHttp {
                url: "http://localhost:3000".into(),
            },
            safety: EnvironmentSafety::Safe,
        },
        RuntimeProfile {
            id: "profile_staging".into(),
            name: "Staging".into(),
            environment: "staging".into(),
            target: RuntimeTarget::RemoteHttp {
                url: "https://staging.example.com".into(),
            },
            safety: EnvironmentSafety::Caution,
        },
        RuntimeProfile {
            id: "profile_production".into(),
            name: "Production".into(),
            environment: "production".into(),
            target: RuntimeTarget::RemoteHttp {
                url: "https://api.example.com".into(),
            },
            safety: EnvironmentSafety::Production,
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
        assert_eq!(loaded.active_environment, Some("mock".into()));
    }

    #[test]
    fn environment_safety_requires_confirmation() {
        assert!(!EnvironmentSafety::Safe.requires_confirmation("GET"));
        assert!(!EnvironmentSafety::Safe.requires_confirmation("POST"));
        
        assert!(!EnvironmentSafety::Caution.requires_confirmation("GET"));
        assert!(EnvironmentSafety::Caution.requires_confirmation("POST"));
        assert!(EnvironmentSafety::Caution.requires_confirmation("DELETE"));
        
        assert!(!EnvironmentSafety::Production.requires_confirmation("GET"));
        assert!(EnvironmentSafety::Production.requires_confirmation("POST"));
        assert!(EnvironmentSafety::Production.requires_confirmation("PUT"));
        assert!(EnvironmentSafety::Production.requires_confirmation("PATCH"));
        assert!(EnvironmentSafety::Production.requires_confirmation("DELETE"));
    }

    #[test]
    fn runtime_profiles_have_correct_safety() {
        let profiles = default_runtime_profiles();
        
        let mock = profiles.iter().find(|p| p.name == "Mock").unwrap();
        assert_eq!(mock.safety, EnvironmentSafety::Safe);
        
        let prod = profiles.iter().find(|p| p.name == "Production").unwrap();
        assert_eq!(prod.safety, EnvironmentSafety::Production);
    }
}
