//! Contract discovery bootstrap.
//!
//! Nothing else in the desktop app runs discovery + compile on its own - every
//! other service only reads a contract if one already happens to exist on
//! disk. This mirrors `scan_contract` in `api-cli` (`Commands::Scan`/`Mock`)
//! so the desktop produces the same endpoints/schemas the CLI would for the
//! same repository.

use std::path::Path;

use api_analyzers::default_analyzers;
use api_compiler::{compile_contract, to_openapi, to_request_collection, validate_contract};
use api_core::{ApiContract, ApiMetadata, ApiToolError, SchemaRegistry};
use api_discovery::{DiscoveryEngine, build_context};

/// Run discovery + compile for `repository`, persisting the generated and
/// effective contracts (plus OpenAPI/collection exports) under `.repo-api/`.
/// Always re-scans, even if a contract already exists.
pub async fn scan_and_persist(repository: &Path) -> anyhow::Result<ApiContract> {
    api_storage::init_layout(repository)?;
    let context = build_context(repository.to_path_buf())?;

    let mut engine = DiscoveryEngine::default();
    for analyzer in default_analyzers() {
        engine.register(analyzer);
    }
    let discovery = engine
        .discover(context.clone())
        .await
        .map_err(|_| ApiToolError::AnalyzerFailed)?;

    let mut contract = compile_contract(
        ApiMetadata {
            title: format!("Repository API ({})", repository.display()),
            version: "1.0.0".into(),
            repository_root: Some(repository.display().to_string()),
        },
        discovery.endpoint_evidence,
        SchemaRegistry::default(),
        discovery.diagnostics,
        vec![],
    );

    let overrides = api_storage::load_overrides(repository)?;
    api_storage::apply_overrides(&mut contract, &overrides);

    validate_contract(&contract)?;
    api_storage::save_generated_contract(repository, &contract)?;
    api_storage::save_effective_contract(repository, &contract)?;
    let openapi = to_openapi(&contract);
    api_storage::save_openapi(repository, &openapi)?;
    let collection = to_request_collection(&contract);
    api_storage::save_collection(repository, &collection)?;

    Ok(contract)
}

/// Return the existing effective contract if one is already on disk,
/// otherwise run discovery to produce one.
pub async fn ensure_contract(repository: &Path) -> anyhow::Result<ApiContract> {
    if let Ok(contract) = api_storage::load_effective_contract(repository) {
        return Ok(contract);
    }
    scan_and_persist(repository).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn ensure_contract_bootstraps_when_missing() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("app.js"),
            r#"
            const express = require('express');
            const app = express();
            app.get('/health', (req, res) => res.json({ status: 'ok' }));
            "#,
        )
        .unwrap();

        let contract = ensure_contract(dir.path()).await.unwrap();
        assert!(
            dir.path()
                .join(".repo-api/contract/effective.json")
                .exists()
        );
        // Second call should hit the already-persisted path, not re-scan.
        let cached = ensure_contract(dir.path()).await.unwrap();
        assert_eq!(contract.endpoints.len(), cached.endpoints.len());
    }
}
