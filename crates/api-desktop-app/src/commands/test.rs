//! Test execution commands

use serde::Deserialize;

use crate::services::TestingService;
use crate::services::testing_service::{TestExportFormat, TestRunConfig, TestRunResult};
use crate::{TestResultDetail, TestSuiteSummary};

use super::{AppState, CommandResult, from_service, state_handle};

/// Run tests request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTestsRequest {
    pub suite_id: Option<String>,
    pub test_ids: Option<Vec<String>>,
}

impl From<RunTestsRequest> for TestRunConfig {
    fn from(request: RunTestsRequest) -> Self {
        Self {
            suite_id: request.suite_id,
            test_ids: request.test_ids,
            environment_id: None,
            parallel: true,
            fail_fast: false,
        }
    }
}

/// Get test result request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTestResultRequest {
    pub test_id: String,
}

/// Export test results request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTestResultsRequest {
    pub format: String, // "junit" | "json" | "html"
    pub suite_id: Option<String>,
}

fn parse_format(format: &str) -> Option<TestExportFormat> {
    match format {
        "junit" => Some(TestExportFormat::JUnit),
        "json" => Some(TestExportFormat::Json),
        "html" => Some(TestExportFormat::Html),
        _ => None,
    }
}

/// List test suites
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn test_list(state: AppState<'_>) -> CommandResult<Vec<TestSuiteSummary>> {
    let state = state_handle(&state);
    from_service(TestingService::list_suites(&state).await)
}

/// Run tests
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn test_run(
    state: AppState<'_>,
    request: RunTestsRequest,
) -> CommandResult<TestRunResult> {
    let state = state_handle(&state);
    from_service(TestingService::run(&state, request.into()).await)
}

/// Get test result details
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn test_result(
    state: AppState<'_>,
    request: GetTestResultRequest,
) -> CommandResult<TestResultDetail> {
    let state = state_handle(&state);
    from_service(TestingService::get_result(&state, &request.test_id).await)
}

/// Export test results
#[cfg_attr(feature = "tauri", tauri::command)]
pub async fn test_export(
    state: AppState<'_>,
    request: ExportTestResultsRequest,
) -> CommandResult<String> {
    let Some(format) = parse_format(&request.format) else {
        return Err(super::CommandError::validation_error("Unsupported export format"));
    };

    let state = state_handle(&state);
    from_service(TestingService::export(&state, format, request.suite_id.as_deref()).await)
}
