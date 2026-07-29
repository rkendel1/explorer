//! Testing service for API test execution.
//!
//! This service handles:
//! - Test suite listing
//! - Test execution through api-testing
//! - Result aggregation
//! - Export in standard formats

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::DesktopStateManager;
use crate::{TestResultDetail, TestSuiteSummary};

use super::{ServiceError, ServiceResult};

/// Test run configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunConfig {
    pub suite_id: Option<String>,
    pub test_ids: Option<Vec<String>>,
    pub environment_id: Option<String>,
    pub parallel: bool,
    pub fail_fast: bool,
}

impl Default for TestRunConfig {
    fn default() -> Self {
        Self {
            suite_id: None,
            test_ids: None,
            environment_id: None,
            parallel: true,
            fail_fast: false,
        }
    }
}

/// Test run result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunResult {
    pub run_id: String,
    pub suite_id: String,
    pub suite_name: String,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: u64,
    pub results: Vec<TestResultDetail>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Test case definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseInfo {
    pub id: String,
    pub name: String,
    pub endpoint_id: String,
    pub assertions: Vec<String>,
    pub last_result: Option<TestResultSummary>,
}

/// Test result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultSummary {
    pub passed: bool,
    pub duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Export format
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestExportFormat {
    JUnit,
    Json,
    Html,
}

/// Testing service implementation
pub struct TestingService;

impl TestingService {
    /// List all test suites
    pub async fn list_suites(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<TestSuiteSummary>> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let _root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        // Load test suites from api-testing
        // For now, return placeholder
        Ok(Vec::new())
    }

    /// Get test suite detail
    pub async fn get_suite(
        state: &Arc<DesktopStateManager>,
        _suite_id: &str,
    ) -> ServiceResult<TestSuiteSummary> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        // Load specific suite
        Err(ServiceError::not_found("Test suite"))
    }

    /// Run tests
    pub async fn run(
        state: &Arc<DesktopStateManager>,
        config: TestRunConfig,
    ) -> ServiceResult<TestRunResult> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        let root = state.active_root.read().await;
        let _root = root.as_ref().ok_or_else(ServiceError::no_project)?;

        let run_id = format!("run_{}", Uuid::new_v4().simple());
        let suite_id = config.suite_id.unwrap_or_else(|| "default".to_string());

        // In production, this would use api-testing to execute tests
        Ok(TestRunResult {
            run_id,
            suite_id: suite_id.clone(),
            suite_name: "API Tests".to_string(),
            passed: 0,
            failed: 0,
            skipped: 0,
            duration_ms: 0,
            results: Vec::new(),
            timestamp: chrono::Utc::now(),
        })
    }

    /// Get test result detail
    pub async fn get_result(
        state: &Arc<DesktopStateManager>,
        _test_id: &str,
    ) -> ServiceResult<TestResultDetail> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        Err(ServiceError::not_found("Test result"))
    }

    /// Export test results
    pub async fn export(
        state: &Arc<DesktopStateManager>,
        format: TestExportFormat,
        _suite_id: Option<&str>,
    ) -> ServiceResult<String> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        match format {
            TestExportFormat::JUnit => Ok(Self::generate_junit_xml(&[])),
            TestExportFormat::Json => Ok(Self::generate_json(&[])?),
            TestExportFormat::Html => Ok(Self::generate_html(&[])),
        }
    }

    /// Create a new test case
    pub async fn create_test(
        state: &Arc<DesktopStateManager>,
        endpoint_id: &str,
        name: &str,
        assertions: Vec<String>,
    ) -> ServiceResult<TestCaseInfo> {
        let project = state.project.read().await;

        if project.is_none() {
            return Err(ServiceError::no_project());
        }

        Ok(TestCaseInfo {
            id: format!("test_{}", Uuid::new_v4().simple()),
            name: name.to_string(),
            endpoint_id: endpoint_id.to_string(),
            assertions,
            last_result: None,
        })
    }

    // Private helpers

    fn generate_junit_xml(results: &[TestResultDetail]) -> String {
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="API Tests" tests="{}" failures="{}" errors="0" skipped="0">
{}  </testsuite>
</testsuites>"#,
            results.len(),
            failed,
            results
                .iter()
                .map(|r| {
                    if r.passed {
                        format!(
                            "    <testcase name=\"{}\" time=\"{:.3}\"/>\n",
                            r.test_name,
                            r.duration_ms as f64 / 1000.0
                        )
                    } else {
                        format!(
                            "    <testcase name=\"{}\" time=\"{:.3}\">\n      <failure message=\"Test failed\"/>\n    </testcase>\n",
                            r.test_name,
                            r.duration_ms as f64 / 1000.0
                        )
                    }
                })
                .collect::<String>()
        )
    }

    fn generate_json(results: &[TestResultDetail]) -> ServiceResult<String> {
        let output = serde_json::json!({
            "suites": [],
            "results": results,
            "summary": {
                "passed": results.iter().filter(|r| r.passed).count(),
                "failed": results.iter().filter(|r| !r.passed).count(),
                "skipped": 0
            }
        });

        serde_json::to_string_pretty(&output).map_err(|e| ServiceError::internal(&e.to_string()))
    }

    fn generate_html(results: &[TestResultDetail]) -> String {
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
  <title>API Test Results</title>
  <style>
    body {{ font-family: sans-serif; margin: 2em; }}
    .summary {{ margin: 1em 0; padding: 1em; background: #f0f0f0; }}
    .passed {{ color: green; }}
    .failed {{ color: red; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
    th {{ background: #f5f5f5; }}
  </style>
</head>
<body>
  <h1>API Test Results</h1>
  <div class="summary">
    <span class="passed">Passed: {}</span> |
    <span class="failed">Failed: {}</span> |
    Total: {}
  </div>
  <table>
    <tr><th>Test</th><th>Status</th><th>Duration</th></tr>
    {}
  </table>
</body>
</html>"#,
            passed,
            failed,
            results.len(),
            results
                .iter()
                .map(|r| format!(
                    "<tr><td>{}</td><td class=\"{}\">{}</td><td>{:.3}s</td></tr>",
                    r.test_name,
                    if r.passed { "passed" } else { "failed" },
                    if r.passed { "PASSED" } else { "FAILED" },
                    r.duration_ms as f64 / 1000.0
                ))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_list_suites_no_project() {
        let app_dir = tempdir().unwrap();
        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));

        let result = TestingService::list_suites(&state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_tests() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let result = TestingService::run(&state, TestRunConfig::default())
            .await
            .unwrap();
        assert!(!result.run_id.is_empty());
    }

    #[test]
    fn test_junit_export() {
        let results = vec![
            TestResultDetail {
                test_id: "t1".to_string(),
                test_name: "Test 1".to_string(),
                passed: true,
                duration_ms: 100,
                assertions: vec![],
            },
            TestResultDetail {
                test_id: "t2".to_string(),
                test_name: "Test 2".to_string(),
                passed: false,
                duration_ms: 50,
                assertions: vec![],
            },
        ];

        let xml = TestingService::generate_junit_xml(&results);
        assert!(xml.contains("tests=\"2\""));
        assert!(xml.contains("failures=\"1\""));
    }
}
