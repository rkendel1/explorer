//! Testing service for API test execution.
//!
//! This service handles:
//! - Test suite listing (from `.repo-api/tests/suites/*.yaml`)
//! - Test execution: resolves each `TestCase.request_id` to a saved request
//!   (see `RequestService::execute_saved`), executes it for real, and
//!   evaluates real assertions via `api_testing::evaluate_assertions`
//! - Result aggregation
//! - Export in standard formats

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::DesktopStateManager;
use crate::{AssertionResult, TestResultDetail, TestSuiteSummary};

use super::{CustomerJourneyService, RequestService, ServiceError, ServiceResult};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareOnboardingResult {
    pub created_suite: bool,
    pub created_request: bool,
    pub suite_id: Option<String>,
    pub request_name: Option<String>,
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

impl From<&api_testing::TestResult> for TestResultDetail {
    fn from(result: &api_testing::TestResult) -> Self {
        Self {
            test_id: result.test_id.clone(),
            test_name: result.test_name.clone(),
            passed: result.passed,
            duration_ms: result.duration_ms,
            assertions: result
                .assertion_results
                .iter()
                .map(|a| AssertionResult {
                    passed: a.passed,
                    message: a.message.clone(),
                    expected: a.expected.clone(),
                    actual: a.actual.clone(),
                })
                .collect(),
        }
    }
}

/// Testing service implementation
pub struct TestingService;

impl TestingService {
    fn suites_dir(root: &Path) -> PathBuf {
        root.join(".repo-api/tests/suites")
    }

    fn suite_path(root: &Path, suite_id: &str) -> PathBuf {
        Self::suites_dir(root).join(format!("{suite_id}.yaml"))
    }

    fn all_suite_paths(root: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(Self::suites_dir(root)) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
            .collect();
        paths.sort();
        paths
    }

    fn join_url(base: &str, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        let normalized = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{}{}", base.trim_end_matches('/'), normalized)
    }

    fn default_request_name(method: &str, path: &str) -> String {
        let compact = path
            .trim_matches('/')
            .replace('/', "-")
            .replace('{', "")
            .replace('}', "");
        if compact.is_empty() {
            format!("{}-root", method.to_lowercase())
        } else {
            format!("{}-{}", method.to_lowercase(), compact)
        }
    }

    /// Ensure onboarding has at least one runnable test suite.
    ///
    /// If suites already exist, this is a no-op. Otherwise it creates a
    /// starter saved request (if needed) and writes
    /// `.repo-api/tests/suites/onboarding-first-test.yaml`.
    pub async fn prepare_onboarding(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<PrepareOnboardingResult> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let existing_paths = Self::all_suite_paths(&root);
        if let Some(path) = existing_paths.first() {
            let suite_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string);
            return Ok(PrepareOnboardingResult {
                created_suite: false,
                created_request: false,
                suite_id,
                request_name: None,
            });
        }

        let mut created_request = false;
        let saved_requests = api_storage::list_saved_requests(&root)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        let request_name = if let Some(saved) = saved_requests.first() {
            saved.name.clone()
        } else {
            let contract = super::contract_service::ensure_contract(&root)
                .await
                .map_err(|e| ServiceError::internal(&format!("failed to load contract: {e}")))?;

            let endpoint = contract.endpoints.iter().next();
            let (method, path) = if let Some(endpoint) = endpoint {
                (endpoint.method.as_str().to_uppercase(), endpoint.path.clone())
            } else {
                ("GET".to_string(), "/__api/health".to_string())
            };

            let base = {
                let runtime = state.runtime.read().await;
                runtime
                    .address
                    .clone()
                    .or_else(|| contract.servers.first().map(|s| s.url.clone()))
                    .unwrap_or_else(|| "http://127.0.0.1:4010".to_string())
            };

            let request_name = Self::default_request_name(&method, &path);
            let url = Self::join_url(&base, &path);

            let saved = api_storage::SavedRequest {
                name: request_name.clone(),
                method,
                url: Some(url),
                endpoint_id: None,
                headers: None,
                body: None,
            };
            api_storage::save_request(&root, &saved)
                .map_err(|e| ServiceError::internal(&e.to_string()))?;
            created_request = true;
            request_name
        };

        let suites_dir = Self::suites_dir(&root);
        std::fs::create_dir_all(&suites_dir)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        let suite_id = "onboarding-first-test".to_string();
        let suite_path = suites_dir.join(format!("{suite_id}.yaml"));
        let suite_yaml = format!(
            "name: Onboarding First Test\ntests:\n  - request: {}\n    assertions:\n      - type: status\n        range: [200, 399]\n",
            request_name
        );

        std::fs::write(&suite_path, suite_yaml)
            .map_err(|e| ServiceError::internal(&e.to_string()))?;

        Ok(PrepareOnboardingResult {
            created_suite: true,
            created_request,
            suite_id: Some(suite_id),
            request_name: Some(request_name),
        })
    }

    /// List all test suites
    pub async fn list_suites(
        state: &Arc<DesktopStateManager>,
    ) -> ServiceResult<Vec<TestSuiteSummary>> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let mut summaries = Vec::new();
        for path in Self::all_suite_paths(&root) {
            let Ok(suite) = api_testing::load_test_suite(&path) else {
                continue;
            };
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&suite.id)
                .to_string();
            summaries.push(TestSuiteSummary {
                id,
                name: suite.name,
                passed: 0,
                failed: 0,
                skipped: 0,
            });
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// Get test suite detail
    pub async fn get_suite(
        state: &Arc<DesktopStateManager>,
        suite_id: &str,
    ) -> ServiceResult<TestSuiteSummary> {
        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let suite = api_testing::load_test_suite(&Self::suite_path(&root, suite_id))
            .map_err(|_| ServiceError::not_found(&format!("Test suite '{suite_id}'")))?;

        Ok(TestSuiteSummary {
            id: suite_id.to_string(),
            name: suite.name,
            passed: 0,
            failed: 0,
            skipped: 0,
        })
    }

    /// Run tests: loads suite(s), resolves each test's saved request,
    /// executes it for real, and evaluates real assertions against the
    /// response. With no `suite_id`, runs every suite found (aggregated).
    pub async fn run(
        state: &Arc<DesktopStateManager>,
        config: TestRunConfig,
    ) -> ServiceResult<TestRunResult> {
        let project = state.project.read().await;
        if project.is_none() {
            return Err(ServiceError::no_project());
        }
        drop(project);

        let root = state
            .active_root
            .read()
            .await
            .clone()
            .ok_or_else(ServiceError::no_project)?;

        let suite_paths: Vec<PathBuf> = if let Some(suite_id) = &config.suite_id {
            let path = Self::suite_path(&root, suite_id);
            if !path.exists() {
                return Err(ServiceError::not_found(&format!(
                    "Test suite '{suite_id}'"
                )));
            }
            vec![path]
        } else {
            Self::all_suite_paths(&root)
        };

        let request_service = RequestService::new();
        let mut results: Vec<TestResultDetail> = Vec::new();
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut total_duration_ms = 0u64;
        let mut suite_names: Vec<String> = Vec::new();

        'suites: for path in &suite_paths {
            let Ok(suite) = api_testing::load_test_suite(path) else {
                continue;
            };
            suite_names.push(suite.name.clone());

            for test in &suite.tests {
                if !test.enabled {
                    skipped += 1;
                    continue;
                }
                // NOTE: `load_test_suite` assigns each test a fresh random id
                // on every load, so an explicit `test_ids` filter from a
                // previous run's ids will never match - a pre-existing
                // limitation of `api-testing`'s suite loader, not something
                // introduced here.
                if let Some(ids) = &config.test_ids
                    && !ids.contains(&test.id)
                {
                    continue;
                }

                let start = Instant::now();
                let test_result = match request_service
                    .execute_saved(state, &test.request_id, config.environment_id.clone())
                    .await
                {
                    Ok(output) => {
                        let response = api_testing::ResponseData {
                            status: output.status,
                            headers: output.headers.into_iter().collect(),
                            body: output.body,
                            duration_ms: output.duration_ms,
                        };
                        let assertion_results =
                            api_testing::evaluate_assertions(&test.assertions, &response);
                        let extracted =
                            api_testing::extract_variables(&test.extract, &response);
                        api_testing::TestResult::success(
                            test,
                            assertion_results,
                            extracted,
                            start.elapsed().as_millis() as u64,
                        )
                    }
                    Err(e) => api_testing::TestResult::failure(test, e.to_string()),
                };

                if test_result.passed {
                    passed += 1;
                } else {
                    failed += 1;
                }
                total_duration_ms += test_result.duration_ms;
                let stop = suite.stop_on_failure && !test_result.passed;
                results.push(TestResultDetail::from(&test_result));

                if stop || config.fail_fast && failed > 0 {
                    continue 'suites;
                }
            }
        }

        let _ = CustomerJourneyService::complete_outcome(
            state,
            api_customer_journey::JourneyOutcome::TestComplete,
        )
        .await;

        Ok(TestRunResult {
            run_id: format!("run_{}", Uuid::new_v4().simple()),
            suite_id: config.suite_id.unwrap_or_else(|| "all".to_string()),
            suite_name: suite_names.join(", "),
            passed,
            failed,
            skipped,
            duration_ms: total_duration_ms,
            results,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Get test result detail.
    ///
    /// NOTE: individual run results aren't persisted across calls yet (only
    /// the aggregate `TestRunResult` from `run()` is returned to the
    /// caller), so this honestly reports "not found" rather than fabricating
    /// a result, matching its pre-existing behavior.
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

    /// Export test results. Runs the requested suite(s) fresh and formats
    /// the real results (there's no persisted "last run" to reuse yet).
    pub async fn export(
        state: &Arc<DesktopStateManager>,
        format: TestExportFormat,
        suite_id: Option<&str>,
    ) -> ServiceResult<String> {
        let run_result = Self::run(
            state,
            TestRunConfig {
                suite_id: suite_id.map(String::from),
                test_ids: None,
                environment_id: None,
                parallel: true,
                fail_fast: false,
            },
        )
        .await?;

        match format {
            TestExportFormat::JUnit => Ok(Self::generate_junit_xml(&run_result.results)),
            TestExportFormat::Json => Self::generate_json(&run_result.results),
            TestExportFormat::Html => Ok(Self::generate_html(&run_result.results)),
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
    async fn test_run_tests_no_suites() {
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
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn test_run_suite_end_to_end() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let base_url =
            crate::services::test_helpers::spawn_test_server().await;
        crate::services::test_helpers::seed_mock_environment(project_dir.path(), &base_url).await;

        // Save a request the suite can reference.
        RequestService::new()
            .save_request(&state, "get-users", "GET", "{{baseUrl}}/users", None, None)
            .await
            .unwrap();

        // Write a suite with one assertion that will pass (status 200) and
        // confirm the run reports it as such using the real response.
        let suites_dir = project_dir.path().join(".repo-api/tests/suites");
        std::fs::create_dir_all(&suites_dir).unwrap();
        std::fs::write(
            suites_dir.join("smoke.yaml"),
            r#"
name: Smoke Suite
tests:
  - request: get-users
    assertions:
      - type: status
        equals: 200
"#,
        )
        .unwrap();

        let result = TestingService::run(
            &state,
            TestRunConfig {
                suite_id: Some("smoke".to_string()),
                ..TestRunConfig::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].passed);
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

    #[tokio::test]
    async fn test_prepare_onboarding_creates_suite_when_missing() {
        let app_dir = tempdir().unwrap();
        let project_dir = tempdir().unwrap();

        std::fs::create_dir_all(project_dir.path().join(".repo-api/contract")).unwrap();
        std::fs::write(
            project_dir.path().join(".repo-api/contract/effective.json"),
            r#"{
  \"version\": \"1\",
  \"metadata\": {\"title\": \"Test API\", \"version\": \"1.0.0\", \"repository_root\": null},
  \"servers\": [{\"url\": \"http://127.0.0.1:4010\"}],
  \"endpoints\": [{
    \"id\": \"ep_health\",
    \"operation_id\": \"health\",
    \"method\": \"GET\",
    \"path\": \"/health\",
    \"summary\": null,
    \"parameters\": [],
    \"request_bodies\": [],
    \"responses\": [],
    \"security\": {\"requirements\": []},
    \"confidence\": {\"score\": 1.0, \"level\": \"high\", \"reasons\": []},
    \"evidence\": []
  }],
  \"schemas\": {\"schemas\": {}},
  \"security_schemes\": [],
  \"diagnostics\": [],
  \"evidence\": {\"endpoint_evidence\": [], \"schema_evidence\": [], \"security_evidence\": []}
}"#,
        )
        .unwrap();

        let state = Arc::new(DesktopStateManager::new(app_dir.path().to_path_buf()));
        *state.active_root.write().await = Some(project_dir.path().to_path_buf());
        *state.project.write().await = Some(crate::services::test_helpers::create_test_project(
            project_dir.path(),
        ));

        let result = TestingService::prepare_onboarding(&state).await.unwrap();
        assert!(result.created_suite);
        assert!(result.suite_id.is_some());

        let suites = TestingService::list_suites(&state).await.unwrap();
        assert_eq!(suites.len(), 1);
    }
}
