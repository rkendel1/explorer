use api_core::{
    Confidence, DetectedFramework, DetectedLanguage, RepositoryFile, RepositoryIdentity,
    RepositoryInventory, RepositoryRevision, RepositorySnapshot, SourceFile, SpecificationFile,
};
use chrono::Utc;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn file_hash(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 4096];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn detect_language(path: &Path) -> DetectedLanguage {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "rs" => DetectedLanguage::Rust,
        "ts" | "tsx" => DetectedLanguage::TypeScript,
        "js" | "mjs" | "cjs" => DetectedLanguage::JavaScript,
        "py" => DetectedLanguage::Python,
        _ => DetectedLanguage::Unknown,
    }
}

fn is_spec_candidate(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).unwrap_or_default(),
        "yaml" | "yml" | "json"
    )
}

fn looks_like_openapi_spec(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if file_name.contains("openapi") || file_name.contains("swagger") {
        return true;
    }

    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };

    // Keep this heuristic cheap and resilient across JSON/YAML formatting.
    content.contains("openapi:")
        || content.contains("\"openapi\"")
        || content.contains("swagger:")
        || content.contains("\"swagger\"")
}

pub fn create_snapshot(root: &Path) -> anyhow::Result<RepositorySnapshot> {
    let mut files = Vec::new();
    let mut content = Sha256::new();
    for entry in WalkBuilder::new(root).standard_filters(true).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.into_path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let h = file_hash(&path)?;
        let size = fs::metadata(&path)?.len();
        content.update(h.as_bytes());
        content.update(rel.to_string_lossy().as_bytes());
        files.push(RepositoryFile {
            path: rel,
            content_hash: h,
            size_bytes: size,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let git_commit = fs::read_to_string(root.join(".git/HEAD"))
        .ok()
        .map(|s| s.trim().to_string())
        .and_then(|head| {
            if head.starts_with("ref:") {
                let ref_path = head.trim_start_matches("ref:").trim();
                fs::read_to_string(root.join(".git").join(ref_path))
                    .ok()
                    .map(|v| v.trim().to_string())
            } else {
                Some(head)
            }
        });

    Ok(RepositorySnapshot {
        id: uuid::Uuid::new_v4().to_string(),
        repository: RepositoryIdentity {
            root: root.to_path_buf(),
            remote_url: None,
        },
        revision: RepositoryRevision {
            commit: git_commit,
            content_hash: format!("{:x}", content.finalize()),
        },
        files,
        created_at: Utc::now(),
    })
}

pub fn inventory(root: &Path) -> anyhow::Result<RepositoryInventory> {
    let mut languages = BTreeSet::new();
    let mut frameworks = BTreeSet::new();
    let mut specifications = Vec::new();
    let mut source_files = Vec::new();

    for entry in WalkBuilder::new(root).standard_filters(true).build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.into_path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let lang = detect_language(&rel);
        if lang != DetectedLanguage::Unknown {
            languages.insert(format!("{:?}", lang));
            source_files.push(SourceFile {
                path: rel.clone(),
                language: lang,
            });
        }
        if is_spec_candidate(&rel) && looks_like_openapi_spec(&path) {
            specifications.push(SpecificationFile { path: rel.clone() });
            frameworks.insert(format!("{:?}", DetectedFramework::OpenApi));
        }

        let name = rel.to_string_lossy().to_lowercase();
        if name.ends_with("package.json") || name.contains("express") {
            frameworks.insert(format!("{:?}", DetectedFramework::Express));
        }
        if name.ends_with("pyproject.toml")
            || name.ends_with("requirements.txt")
            || name.contains("fastapi")
        {
            frameworks.insert(format!("{:?}", DetectedFramework::FastApi));
        }
        if name.ends_with("cargo.toml") || name.contains("axum") {
            frameworks.insert(format!("{:?}", DetectedFramework::Axum));
        }
    }

    let map_lang = |s: String| match s.as_str() {
        "Rust" => DetectedLanguage::Rust,
        "TypeScript" => DetectedLanguage::TypeScript,
        "JavaScript" => DetectedLanguage::JavaScript,
        "Python" => DetectedLanguage::Python,
        _ => DetectedLanguage::Unknown,
    };
    let map_fw = |s: String| match s.as_str() {
        "Express" => DetectedFramework::Express,
        "FastApi" => DetectedFramework::FastApi,
        "Axum" => DetectedFramework::Axum,
        "OpenApi" => DetectedFramework::OpenApi,
        _ => DetectedFramework::Unknown,
    };

    Ok(RepositoryInventory {
        languages: languages.into_iter().map(map_lang).collect(),
        frameworks: frameworks.into_iter().map(map_fw).collect(),
        specifications,
        source_files,
        confidence: Confidence::medium(),
    })
}

pub fn read_file(root: &Path, rel: &Path) -> anyhow::Result<String> {
    Ok(fs::read_to_string(root.join(rel))?)
}

pub fn list_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    Ok(create_snapshot(root)?
        .files
        .into_iter()
        .map(|f| f.path)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_openapi_spec_in_inventory() {
        let root = PathBuf::from("/tmp/repo_api_repository_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("openapi.yaml"), "openapi: 3.1.0").expect("write spec");
        let inv = inventory(&root).expect("inventory");
        assert_eq!(inv.specifications.len(), 1);
        let snap = create_snapshot(&root).expect("snapshot");
        assert!(!snap.revision.content_hash.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
