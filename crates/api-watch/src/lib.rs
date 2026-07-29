//! Repository watch mode with incremental analysis.
//!
//! This crate owns:
//! - Filesystem watching
//! - Change debouncing
//! - Changed-file classification
//! - Incremental scan planning
//! - Watch lifecycle
//! - Repository synchronization events

use api_compiler::{DiffKind, compile_contract, diff_contracts};
use api_core::{ApiContract, ApiMetadata, SchemaRegistry};
use api_discovery::{DiscoveryEngine, build_context};
use api_runtime_events::EventEmitter;
use chrono::{DateTime, Utc};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{RwLock, broadcast, mpsc};
use uuid::Uuid;

pub type ChangeSetId = String;
pub type ContractRevisionId = String;
pub type SnapshotId = String;

/// Watch state for repository synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    Synchronized,
    Scanning,
    ChangesDetected,
    ReviewRequired,
    Applying,
    Degraded,
    Error,
    Stopped,
}

impl std::fmt::Display for WatchState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synchronized => write!(f, "synchronized"),
            Self::Scanning => write!(f, "scanning"),
            Self::ChangesDetected => write!(f, "changes_detected"),
            Self::ReviewRequired => write!(f, "review_required"),
            Self::Applying => write!(f, "applying"),
            Self::Degraded => write!(f, "degraded"),
            Self::Error => write!(f, "error"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// Contract revision metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRevision {
    pub id: ContractRevisionId,
    pub generated_from: SnapshotId,
    pub generated_at: DateTime<Utc>,
    pub content_hash: String,
    pub parent: Option<ContractRevisionId>,
}

impl ContractRevision {
    pub fn new(snapshot_id: &str, content_hash: String, parent: Option<String>) -> Self {
        Self {
            id: format!("ctr_{}", Uuid::new_v4().simple()),
            generated_from: snapshot_id.to_string(),
            generated_at: Utc::now(),
            content_hash,
            parent,
        }
    }

    pub fn from_contract(
        contract: &ApiContract,
        snapshot_id: &str,
        parent: Option<String>,
    ) -> Self {
        let hash = compute_contract_hash(contract);
        Self::new(snapshot_id, hash, parent)
    }
}

/// Contract provenance tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractProvenance {
    pub repository_root: PathBuf,
    pub analyzer_versions: HashMap<String, String>,
    pub generated_at: DateTime<Utc>,
}

/// Change classification for contract changes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    EndpointAdded,
    EndpointRemoved,
    EndpointModified,
    ParameterAdded,
    ParameterRemoved,
    ParameterModified,
    RequestSchemaChanged,
    ResponseSchemaChanged,
    SecurityChanged,
    ExampleChanged,
    Breaking,
    NonBreaking,
    Uncertain,
}

/// Individual contract change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChange {
    pub key: String,
    pub categories: Vec<ChangeCategory>,
    pub description: Option<String>,
}

/// Contract change set for review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractChangeSet {
    pub id: ChangeSetId,
    pub from_revision: ContractRevisionId,
    pub to_revision: ContractRevisionId,
    pub changes: Vec<ContractChange>,
    pub override_impacts: Vec<OverrideImpact>,
    pub status: ChangeSetStatus,
    pub created_at: DateTime<Utc>,
}

impl ContractChangeSet {
    pub fn new(
        from_revision: &str,
        to_revision: &str,
        changes: Vec<ContractChange>,
        override_impacts: Vec<OverrideImpact>,
    ) -> Self {
        Self {
            id: format!("cs_{}", Uuid::new_v4().simple()),
            from_revision: from_revision.to_string(),
            to_revision: to_revision.to_string(),
            changes,
            override_impacts,
            status: ChangeSetStatus::Pending,
            created_at: Utc::now(),
        }
    }

    pub fn has_breaking_changes(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.categories.contains(&ChangeCategory::Breaking))
    }

    pub fn has_override_conflicts(&self) -> bool {
        self.override_impacts.iter().any(|i| {
            i.outcome == OverrideOutcome::RequiresReview || i.outcome == OverrideOutcome::Orphaned
        })
    }
}

/// Change set status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetStatus {
    Pending,
    Accepted,
    Rejected,
    PartiallyAccepted,
    Superseded,
    Failed,
}

/// Override impact from contract changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideImpact {
    pub override_key: String,
    pub outcome: OverrideOutcome,
    pub description: String,
}

/// Override rebase outcome
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideOutcome {
    Applied,
    Rebased,
    RequiresReview,
    Orphaned,
    Invalid,
}

/// File change type for classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeType {
    SourceFile,
    OpenApiSpec,
    RouteFile,
    SchemaFile,
    ConfigFile,
    Unknown,
}

/// Analysis graph for incremental analysis
#[derive(Debug, Clone, Default)]
pub struct AnalysisGraph {
    pub files: HashMap<PathBuf, FileNode>,
    pub dependencies: HashMap<PathBuf, Vec<PathBuf>>,
    pub outputs: HashMap<PathBuf, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub content_hash: String,
    pub file_type: FileChangeType,
    pub last_analyzed: Option<DateTime<Utc>>,
}

impl AnalysisGraph {
    pub fn get_affected_files(&self, changed: &[PathBuf]) -> Vec<PathBuf> {
        let mut affected: HashSet<PathBuf> = changed.iter().cloned().collect();
        let mut to_check: Vec<PathBuf> = changed.to_vec();

        while let Some(file) = to_check.pop() {
            for (dependent, deps) in &self.dependencies {
                if deps.contains(&file) && affected.insert(dependent.clone()) {
                    to_check.push(dependent.clone());
                }
            }
        }

        affected.into_iter().collect()
    }
}

/// Watch configuration
#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub debounce_ms: u64,
    pub auto_accept_non_breaking: bool,
    pub strict_validation: bool,
    pub incremental: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 300,
            auto_accept_non_breaking: false,
            strict_validation: false,
            incremental: true,
        }
    }
}

/// Watch event for subscribers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchEvent {
    StateChanged {
        previous: WatchState,
        current: WatchState,
    },
    FilesChanged {
        files: Vec<String>,
        file_count: usize,
    },
    AnalysisStarted {
        incremental: bool,
        file_count: usize,
    },
    AnalysisCompleted {
        duration_ms: u64,
        endpoint_count: usize,
    },
    ContractChanged {
        change_set_id: String,
        added: usize,
        modified: usize,
        removed: usize,
        breaking: bool,
    },
    ChangeSetAccepted {
        change_set_id: String,
    },
    ChangeSetRejected {
        change_set_id: String,
    },
    Error {
        code: String,
        message: String,
    },
}

/// Repository watcher state
#[allow(dead_code)]
pub struct RepositoryWatcher {
    root: PathBuf,
    config: WatchConfig,
    state: Arc<RwLock<WatchState>>,
    current_contract: Arc<RwLock<Option<ApiContract>>>,
    current_revision: Arc<RwLock<Option<ContractRevision>>>,
    pending_change_set: Arc<RwLock<Option<ContractChangeSet>>>,
    analysis_graph: Arc<RwLock<AnalysisGraph>>,
    event_tx: broadcast::Sender<WatchEvent>,
    runtime_events: Option<EventEmitter>,
    analyzers: Vec<Arc<dyn api_discovery::ApiAnalyzer>>,
}

impl RepositoryWatcher {
    pub fn new(root: PathBuf, config: WatchConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            root,
            config,
            state: Arc::new(RwLock::new(WatchState::Stopped)),
            current_contract: Arc::new(RwLock::new(None)),
            current_revision: Arc::new(RwLock::new(None)),
            pending_change_set: Arc::new(RwLock::new(None)),
            analysis_graph: Arc::new(RwLock::new(AnalysisGraph::default())),
            event_tx,
            runtime_events: None,
            analyzers: api_analyzers::default_analyzers(),
        }
    }

    pub fn with_event_emitter(mut self, emitter: EventEmitter) -> Self {
        self.runtime_events = Some(emitter);
        self
    }

    /// Simple start method that wraps self in Arc and calls start_watching
    pub async fn start(self) -> anyhow::Result<()> {
        Arc::new(self).start_watching().await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WatchEvent> {
        self.event_tx.subscribe()
    }

    pub async fn state(&self) -> WatchState {
        *self.state.read().await
    }

    pub async fn current_contract(&self) -> Option<ApiContract> {
        self.current_contract.read().await.clone()
    }

    pub async fn current_revision(&self) -> Option<ContractRevision> {
        self.current_revision.read().await.clone()
    }

    pub async fn pending_changes(&self) -> Option<ContractChangeSet> {
        self.pending_change_set.read().await.clone()
    }

    async fn set_state(&self, new_state: WatchState) {
        let previous = {
            let mut state = self.state.write().await;
            let prev = *state;
            *state = new_state;
            prev
        };
        if previous != new_state {
            let _ = self.event_tx.send(WatchEvent::StateChanged {
                previous,
                current: new_state,
            });
        }
    }

    async fn emit_event(&self, event: WatchEvent) {
        let _ = self.event_tx.send(event);
    }

    pub async fn initial_scan(&self) -> anyhow::Result<()> {
        self.set_state(WatchState::Scanning).await;

        let start = std::time::Instant::now();
        let context = build_context(self.root.clone())?;

        let mut engine = DiscoveryEngine::default();
        for analyzer in &self.analyzers {
            engine.register(analyzer.clone());
        }

        let discovery = engine
            .discover(context.clone())
            .await
            .map_err(|_| anyhow::anyhow!("Analysis failed"))?;

        let contract = compile_contract(
            ApiMetadata {
                title: format!("Repository API ({})", self.root.display()),
                version: "1.0.0".into(),
                repository_root: Some(self.root.display().to_string()),
            },
            discovery.endpoint_evidence,
            SchemaRegistry::default(),
            discovery.diagnostics,
            vec![],
        );

        let revision = ContractRevision::from_contract(&contract, &context.snapshot.id, None);

        // Save to storage
        api_storage::init_layout(&self.root)?;
        api_storage::save_generated_contract(&self.root, &contract)?;
        api_storage::save_effective_contract(&self.root, &contract)?;

        let endpoint_count = contract.endpoints.len();
        {
            let mut current = self.current_contract.write().await;
            *current = Some(contract);
        }
        {
            let mut rev = self.current_revision.write().await;
            *rev = Some(revision);
        }

        self.emit_event(WatchEvent::AnalysisCompleted {
            duration_ms: start.elapsed().as_millis() as u64,
            endpoint_count,
        })
        .await;

        self.set_state(WatchState::Synchronized).await;
        Ok(())
    }

    pub async fn analyze_changes(&self, changed_files: Vec<PathBuf>) -> anyhow::Result<()> {
        let previous_contract = self.current_contract.read().await.clone();
        let previous_revision = self.current_revision.read().await.clone();

        self.set_state(WatchState::Scanning).await;

        let file_count = changed_files.len();
        self.emit_event(WatchEvent::AnalysisStarted {
            incremental: self.config.incremental,
            file_count,
        })
        .await;

        let start = std::time::Instant::now();
        let context = build_context(self.root.clone())?;

        let mut engine = DiscoveryEngine::default();
        for analyzer in &self.analyzers {
            engine.register(analyzer.clone());
        }

        let discovery = engine
            .discover(context.clone())
            .await
            .map_err(|_| anyhow::anyhow!("Analysis failed"))?;

        let new_contract = compile_contract(
            ApiMetadata {
                title: format!("Repository API ({})", self.root.display()),
                version: "1.0.0".into(),
                repository_root: Some(self.root.display().to_string()),
            },
            discovery.endpoint_evidence,
            SchemaRegistry::default(),
            discovery.diagnostics,
            vec![],
        );

        let new_revision = ContractRevision::from_contract(
            &new_contract,
            &context.snapshot.id,
            previous_revision.as_ref().map(|r| r.id.clone()),
        );

        self.emit_event(WatchEvent::AnalysisCompleted {
            duration_ms: start.elapsed().as_millis() as u64,
            endpoint_count: new_contract.endpoints.len(),
        })
        .await;

        // Check for changes
        if let Some(prev) = &previous_contract {
            let diffs = diff_contracts(prev, &new_contract);
            let has_changes = !diffs.iter().all(|(k, _)| k == "no-change");

            if has_changes {
                let changes: Vec<ContractChange> = diffs
                    .into_iter()
                    .filter(|(k, _)| k != "no-change")
                    .map(|(key, kind)| {
                        let categories = match kind {
                            DiffKind::Added => {
                                vec![ChangeCategory::EndpointAdded, ChangeCategory::NonBreaking]
                            }
                            DiffKind::Removed => {
                                vec![ChangeCategory::EndpointRemoved, ChangeCategory::Breaking]
                            }
                            DiffKind::Modified => {
                                vec![ChangeCategory::EndpointModified, ChangeCategory::Uncertain]
                            }
                            DiffKind::Breaking => vec![ChangeCategory::Breaking],
                            DiffKind::NonBreaking => vec![ChangeCategory::NonBreaking],
                            DiffKind::Uncertain => vec![ChangeCategory::Uncertain],
                        };
                        ContractChange {
                            key,
                            categories,
                            description: None,
                        }
                    })
                    .collect();

                let change_set = ContractChangeSet::new(
                    &previous_revision
                        .as_ref()
                        .map(|r| r.id.clone())
                        .unwrap_or_default(),
                    &new_revision.id,
                    changes.clone(),
                    vec![],
                );

                let breaking = change_set.has_breaking_changes();
                let added = changes
                    .iter()
                    .filter(|c| c.categories.contains(&ChangeCategory::EndpointAdded))
                    .count();
                let modified = changes
                    .iter()
                    .filter(|c| c.categories.contains(&ChangeCategory::EndpointModified))
                    .count();
                let removed = changes
                    .iter()
                    .filter(|c| c.categories.contains(&ChangeCategory::EndpointRemoved))
                    .count();

                self.emit_event(WatchEvent::ContractChanged {
                    change_set_id: change_set.id.clone(),
                    added,
                    modified,
                    removed,
                    breaking,
                })
                .await;

                // Store pending changes
                {
                    let mut pending = self.pending_change_set.write().await;
                    *pending = Some(change_set);
                }

                // Auto-accept non-breaking if configured
                if self.config.auto_accept_non_breaking && !breaking {
                    self.accept_changes().await?;
                } else {
                    self.set_state(WatchState::ReviewRequired).await;
                    // Store new contract for preview but don't make it effective yet
                    api_storage::save_generated_contract(&self.root, &new_contract)?;
                    {
                        let mut rev = self.current_revision.write().await;
                        *rev = Some(new_revision);
                    }
                }
            } else {
                self.set_state(WatchState::Synchronized).await;
            }
        } else {
            // No previous contract, just set as current
            api_storage::save_generated_contract(&self.root, &new_contract)?;
            api_storage::save_effective_contract(&self.root, &new_contract)?;
            {
                let mut current = self.current_contract.write().await;
                *current = Some(new_contract);
            }
            {
                let mut rev = self.current_revision.write().await;
                *rev = Some(new_revision);
            }
            self.set_state(WatchState::Synchronized).await;
        }

        Ok(())
    }

    pub async fn accept_changes(&self) -> anyhow::Result<()> {
        self.set_state(WatchState::Applying).await;

        let change_set = {
            let mut pending = self.pending_change_set.write().await;
            pending.take()
        };

        if let Some(mut cs) = change_set {
            // Load the generated contract and make it effective
            let contract = api_storage::load_effective_contract(&self.root).or_else(|_| {
                let data = std::fs::read(self.root.join(".repo-api/contract/generated.json"))?;
                serde_json::from_slice(&data).map_err(anyhow::Error::from)
            })?;

            // Apply overrides
            let overrides = api_storage::load_overrides(&self.root)?;
            let mut effective = contract.clone();
            api_storage::apply_overrides(&mut effective, &overrides);

            api_storage::save_effective_contract(&self.root, &effective)?;

            {
                let mut current = self.current_contract.write().await;
                *current = Some(effective);
            }

            cs.status = ChangeSetStatus::Accepted;
            self.emit_event(WatchEvent::ChangeSetAccepted {
                change_set_id: cs.id,
            })
            .await;
        }

        self.set_state(WatchState::Synchronized).await;
        Ok(())
    }

    pub async fn reject_changes(&self) -> anyhow::Result<()> {
        let change_set = {
            let mut pending = self.pending_change_set.write().await;
            pending.take()
        };

        if let Some(cs) = change_set {
            self.emit_event(WatchEvent::ChangeSetRejected {
                change_set_id: cs.id,
            })
            .await;
        }

        self.set_state(WatchState::Synchronized).await;
        Ok(())
    }

    pub async fn start_watching(self: Arc<Self>) -> anyhow::Result<()> {
        // Initial scan
        self.initial_scan().await?;

        let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(100);

        let root = self.root.clone();
        let debounce_ms = self.config.debounce_ms;

        // Spawn file watcher
        std::thread::spawn(move || {
            let (event_tx, event_rx) = std::sync::mpsc::channel();
            let mut watcher = RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = event_tx.send(event);
                    }
                },
                notify::Config::default(),
            )
            .expect("watcher");

            watcher
                .watch(&root, RecursiveMode::Recursive)
                .expect("watch");

            let mut pending: HashSet<PathBuf> = HashSet::new();
            let mut last_event = std::time::Instant::now();

            loop {
                match event_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(event) => {
                        for path in event.paths {
                            if should_watch_file(&path) {
                                pending.insert(path);
                                last_event = std::time::Instant::now();
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if !pending.is_empty()
                            && last_event.elapsed() >= Duration::from_millis(debounce_ms)
                        {
                            let files: Vec<PathBuf> = pending.drain().collect();
                            let _ = tx.blocking_send(files);
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        // Process file changes
        let watcher = self.clone();
        tokio::spawn(async move {
            while let Some(files) = rx.recv().await {
                let file_names: Vec<String> = files
                    .iter()
                    .filter_map(|p| p.file_name())
                    .filter_map(|n| n.to_str())
                    .map(String::from)
                    .collect();

                watcher
                    .emit_event(WatchEvent::FilesChanged {
                        files: file_names,
                        file_count: files.len(),
                    })
                    .await;

                if let Err(e) = watcher.analyze_changes(files).await {
                    watcher
                        .emit_event(WatchEvent::Error {
                            code: "ANALYSIS_FAILED".into(),
                            message: e.to_string(),
                        })
                        .await;
                    watcher.set_state(WatchState::Degraded).await;
                }
            }
        });

        Ok(())
    }
}

fn should_watch_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Ignore common non-source directories
    if path_str.contains("node_modules")
        || path_str.contains("target")
        || path_str.contains(".git")
        || path_str.contains(".repo-api")
        || path_str.contains("__pycache__")
        || path_str.contains(".venv")
    {
        return false;
    }

    // Check for relevant extensions
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    matches!(
        ext,
        "ts" | "tsx" | "js" | "mjs" | "cjs" | "py" | "rs" | "yaml" | "yml" | "json"
    )
}

#[allow(dead_code)]
fn classify_file_change(path: &Path) -> FileChangeType {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let path_str = path.to_str().unwrap_or("").to_lowercase();

    if name.contains("openapi") || name.contains("swagger") {
        return FileChangeType::OpenApiSpec;
    }

    // Check filename or path contains route indicators
    if name.contains("route")
        || name.contains("router")
        || name.contains("endpoint")
        || path_str.contains("/routes/")
        || path_str.contains("/router/")
        || path_str.contains("/endpoints/")
    {
        return FileChangeType::RouteFile;
    }

    if name.contains("schema") || name.contains("model") || name.contains("type") {
        return FileChangeType::SchemaFile;
    }

    if name.contains("config") || name == "package.json" || name == "cargo.toml" {
        return FileChangeType::ConfigFile;
    }

    match ext {
        "ts" | "tsx" | "js" | "mjs" | "cjs" | "py" | "rs" => FileChangeType::SourceFile,
        "yaml" | "yml" | "json" => {
            if name.contains("openapi") || name.contains("swagger") {
                FileChangeType::OpenApiSpec
            } else {
                FileChangeType::ConfigFile
            }
        }
        _ => FileChangeType::Unknown,
    }
}

fn compute_contract_hash(contract: &ApiContract) -> String {
    let json = serde_json::to_string(contract).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_openapi_file() {
        let path = PathBuf::from("api/openapi.yaml");
        assert_eq!(classify_file_change(&path), FileChangeType::OpenApiSpec);
    }

    #[test]
    fn classify_route_file() {
        let path = PathBuf::from("src/routes/users.ts");
        assert_eq!(classify_file_change(&path), FileChangeType::RouteFile);
    }

    #[test]
    fn classify_source_file() {
        let path = PathBuf::from("src/handlers/auth.ts");
        assert_eq!(classify_file_change(&path), FileChangeType::SourceFile);
    }

    #[test]
    fn should_ignore_node_modules() {
        let path = PathBuf::from("node_modules/express/index.js");
        assert!(!should_watch_file(&path));
    }

    #[test]
    fn watch_state_display() {
        assert_eq!(WatchState::Synchronized.to_string(), "synchronized");
        assert_eq!(WatchState::ReviewRequired.to_string(), "review_required");
    }

    #[test]
    fn analysis_graph_affected_files() {
        let mut graph = AnalysisGraph::default();
        graph.dependencies.insert(
            PathBuf::from("handler.ts"),
            vec![PathBuf::from("schema.ts")],
        );

        let affected = graph.get_affected_files(&[PathBuf::from("schema.ts")]);
        assert!(affected.contains(&PathBuf::from("schema.ts")));
        assert!(affected.contains(&PathBuf::from("handler.ts")));
    }
}
