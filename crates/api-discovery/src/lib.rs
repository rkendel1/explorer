use api_core::{
    AnalyzerId, ApiToolError, Diagnostic, EndpointEvidence, RepositoryInventory,
    RepositorySnapshot, SchemaEvidence, SecurityEvidence,
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct AnalyzerSupport {
    pub supported: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnalyzerContext {
    pub root: PathBuf,
    pub snapshot: RepositorySnapshot,
    pub inventory: RepositoryInventory,
}

#[derive(Debug, Clone, Default)]
pub struct AnalyzerOutput {
    pub endpoint_evidence: Vec<EndpointEvidence>,
    pub schema_evidence: Vec<SchemaEvidence>,
    pub security_evidence: Vec<SecurityEvidence>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    #[error("analyzer failed: {0}")]
    Generic(String),
}

#[async_trait]
pub trait ApiAnalyzer: Send + Sync {
    fn id(&self) -> AnalyzerId;
    fn supports(&self, inventory: &RepositoryInventory) -> AnalyzerSupport;
    async fn analyze(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, AnalyzerError>;
}

#[derive(Default)]
pub struct DiscoveryEngine {
    analyzers: Vec<Arc<dyn ApiAnalyzer>>,
}

impl DiscoveryEngine {
    pub fn register(&mut self, analyzer: Arc<dyn ApiAnalyzer>) {
        self.analyzers.push(analyzer);
    }

    pub async fn discover(&self, context: AnalyzerContext) -> Result<AnalyzerOutput, ApiToolError> {
        let mut merged = AnalyzerOutput::default();
        for analyzer in &self.analyzers {
            let support = analyzer.supports(&context.inventory);
            if !support.supported {
                continue;
            }
            let output = analyzer
                .analyze(context.clone())
                .await
                .map_err(|_| ApiToolError::AnalyzerFailed)?;
            merged.endpoint_evidence.extend(output.endpoint_evidence);
            merged.schema_evidence.extend(output.schema_evidence);
            merged.security_evidence.extend(output.security_evidence);
            merged.diagnostics.extend(output.diagnostics);
        }
        Ok(merged)
    }
}

pub fn build_context(root: PathBuf) -> anyhow::Result<AnalyzerContext> {
    let snapshot = api_repository::create_snapshot(&root)?;
    let inventory = api_repository::inventory(&root)?;
    Ok(AnalyzerContext {
        root,
        snapshot,
        inventory,
    })
}
