use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub type SnapshotId = String;
pub type ContractVersion = String;
pub type EndpointId = String;
pub type SchemaId = String;
pub type AnalyzerId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confidence {
    pub score: f32,
    pub level: ConfidenceLevel,
}

impl Confidence {
    pub fn high() -> Self {
        Self {
            score: 0.95,
            level: ConfidenceLevel::High,
        }
    }
    pub fn medium() -> Self {
        Self {
            score: 0.7,
            level: ConfidenceLevel::Medium,
        }
    }
    pub fn low() -> Self {
        Self {
            score: 0.4,
            level: ConfidenceLevel::Low,
        }
    }
    pub fn unknown() -> Self {
        Self {
            score: 0.1,
            level: ConfidenceLevel::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceReference {
    pub file: String,
    pub line_start: Option<usize>,
    pub line_end: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub evidence: Vec<EvidenceReference>,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub id: SnapshotId,
    pub repository: RepositoryIdentity,
    pub revision: RepositoryRevision,
    pub files: Vec<RepositoryFile>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    pub root: PathBuf,
    pub remote_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryRevision {
    pub commit: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryFile {
    pub path: PathBuf,
    pub content_hash: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DetectedLanguage {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DetectedFramework {
    Express,
    FastApi,
    Axum,
    OpenApi,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificationFile {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: DetectedLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInventory {
    pub languages: Vec<DetectedLanguage>,
    pub frameworks: Vec<DetectedFramework>,
    pub specifications: Vec<SpecificationFile>,
    pub source_files: Vec<SourceFile>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    OPTIONS,
    HEAD,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GET => "get",
            Self::POST => "post",
            Self::PUT => "put",
            Self::PATCH => "patch",
            Self::DELETE => "delete",
            Self::OPTIONS => "options",
            Self::HEAD => "head",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiMetadata {
    pub title: String,
    pub version: String,
    pub repository_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerDefinition {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParameter {
    pub name: String,
    pub location: ParameterLocation,
    pub required: bool,
    pub schema: SchemaReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterLocation {
    Path,
    Query,
    Header,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBodyDefinition {
    pub content_type: String,
    pub required: bool,
    pub schema: SchemaReference,
    pub example: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseDefinition {
    pub status: u16,
    pub content_type: Option<String>,
    pub schema: Option<SchemaReference>,
    pub example: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityRequirement {
    pub schemes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub id: EndpointId,
    pub operation_id: Option<String>,
    pub method: HttpMethod,
    pub path: String,
    pub summary: Option<String>,
    pub parameters: Vec<ApiParameter>,
    pub request_bodies: Vec<RequestBodyDefinition>,
    pub responses: Vec<ResponseDefinition>,
    pub security: SecurityRequirement,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaRegistry {
    pub schemas: BTreeMap<SchemaId, ApiSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiSchema {
    Null,
    Boolean,
    Integer(IntegerSchema),
    Number(NumberSchema),
    String(StringSchema),
    Array(ArraySchema),
    Object(ObjectSchema),
    Enum(EnumSchema),
    OneOf(Vec<SchemaReference>),
    AnyOf(Vec<SchemaReference>),
    AllOf(Vec<SchemaReference>),
    Reference(SchemaReference),
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegerSchema {
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub example: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberSchema {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub example: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringSchema {
    pub format: Option<String>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub example: Option<String>,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArraySchema {
    pub items: SchemaReference,
    pub min_items: Option<usize>,
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSchema {
    pub properties: BTreeMap<String, SchemaReference>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnumSchema {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaReference {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceIndex {
    pub endpoint_evidence: Vec<EndpointEvidence>,
    pub schema_evidence: Vec<SchemaEvidence>,
    pub security_evidence: Vec<SecurityEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiContract {
    pub version: ContractVersion,
    pub metadata: ApiMetadata,
    pub servers: Vec<ServerDefinition>,
    pub endpoints: Vec<ApiEndpoint>,
    pub schemas: SchemaRegistry,
    pub security_schemes: Vec<SecurityScheme>,
    pub diagnostics: Vec<Diagnostic>,
    pub evidence: EvidenceIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointEvidence {
    pub analyzer_id: AnalyzerId,
    pub method: HttpMethod,
    pub path: String,
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub parameters: Vec<ApiParameter>,
    pub request_bodies: Vec<RequestBodyDefinition>,
    pub responses: Vec<ResponseDefinition>,
    pub security: SecurityRequirement,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEvidence {
    pub analyzer_id: AnalyzerId,
    pub schema_id: SchemaId,
    pub schema: ApiSchema,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvidence {
    pub analyzer_id: AnalyzerId,
    pub scheme: SecurityScheme,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCollection {
    pub name: String,
    pub requests: Vec<SavedRequest>,
    pub environments: Vec<ApiEnvironment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRequest {
    pub id: String,
    pub name: String,
    pub method: HttpMethod,
    pub url_template: String,
    pub headers: Vec<HeaderDefinition>,
    pub query: Vec<QueryParameter>,
    pub body: Option<RequestBody>,
    pub source_endpoint: EndpointId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderDefinition {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParameter {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    pub content_type: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEnvironment {
    pub name: String,
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiToolError {
    #[error("repository not found")]
    RepositoryNotFound,
    #[error("repository unreadable")]
    RepositoryUnreadable,
    #[error("unsupported repository")]
    UnsupportedRepository,
    #[error("analyzer failed")]
    AnalyzerFailed,
    #[error("contract compilation failed")]
    ContractCompilationFailed,
    #[error("contract validation failed")]
    ContractValidationFailed,
    #[error("mock runtime failed")]
    MockRuntimeFailed,
    #[error("request execution failed")]
    RequestExecutionFailed,
    #[error("environment not found")]
    EnvironmentNotFound,
}
