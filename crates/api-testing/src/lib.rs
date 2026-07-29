//! API testing framework with assertions, test suites, and batch execution.
//!
//! This crate owns:
//! - Request assertions
//! - Test cases
//! - Test suites
//! - Test execution
//! - Result aggregation
//! - Test reports

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use uuid::Uuid;

pub type TestId = String;
pub type SuiteId = String;
pub type AssertionId = String;

/// Assertion types for request validation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    Status(StatusAssertion),
    Header(HeaderAssertion),
    Body(BodyAssertion),
    Duration(DurationAssertion),
    Schema(SchemaAssertion),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusAssertion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderAssertion {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyAssertion {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurationAssertion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub less_than_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub greater_than_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaAssertion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
}

/// Assertion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    pub assertion: Assertion,
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

impl AssertionResult {
    pub fn pass(assertion: Assertion, message: impl Into<String>) -> Self {
        Self {
            assertion,
            passed: true,
            message: message.into(),
            actual: None,
            expected: None,
        }
    }

    pub fn fail(
        assertion: Assertion,
        message: impl Into<String>,
        expected: Option<String>,
        actual: Option<String>,
    ) -> Self {
        Self {
            assertion,
            passed: false,
            message: message.into(),
            actual,
            expected,
        }
    }
}

/// Response data for assertion evaluation
#[derive(Debug, Clone)]
pub struct ResponseData {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    pub duration_ms: u64,
}

/// Evaluate assertions against a response
pub fn evaluate_assertions(
    assertions: &[Assertion],
    response: &ResponseData,
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|a| evaluate_assertion(a, response))
        .collect()
}

fn evaluate_assertion(assertion: &Assertion, response: &ResponseData) -> AssertionResult {
    match assertion {
        Assertion::Status(s) => evaluate_status(assertion.clone(), s, response.status),
        Assertion::Header(h) => evaluate_header(assertion.clone(), h, &response.headers),
        Assertion::Body(b) => evaluate_body(assertion.clone(), b, &response.body),
        Assertion::Duration(d) => evaluate_duration(assertion.clone(), d, response.duration_ms),
        Assertion::Schema(_) => {
            // Schema validation is handled separately
            AssertionResult::pass(assertion.clone(), "Schema validation skipped")
        }
    }
}

fn evaluate_status(assertion: Assertion, spec: &StatusAssertion, actual: u16) -> AssertionResult {
    if let Some(expected) = spec.equals {
        if actual == expected {
            AssertionResult::pass(assertion, format!("Status is {}", actual))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Status mismatch"),
                Some(expected.to_string()),
                Some(actual.to_string()),
            )
        }
    } else if let Some((min, max)) = spec.range {
        if actual >= min && actual <= max {
            AssertionResult::pass(assertion, format!("Status {} in range {}-{}", actual, min, max))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Status out of range"),
                Some(format!("{}-{}", min, max)),
                Some(actual.to_string()),
            )
        }
    } else {
        AssertionResult::pass(assertion, "No status constraint")
    }
}

fn evaluate_header(
    assertion: Assertion,
    spec: &HeaderAssertion,
    headers: &BTreeMap<String, String>,
) -> AssertionResult {
    let key = spec.name.to_lowercase();
    let value = headers.iter().find(|(k, _)| k.to_lowercase() == key).map(|(_, v)| v);

    if let Some(exists) = spec.exists {
        if value.is_some() == exists {
            AssertionResult::pass(
                assertion,
                format!("Header '{}' exists: {}", spec.name, exists),
            )
        } else {
            AssertionResult::fail(
                assertion,
                format!("Header '{}' existence mismatch", spec.name),
                Some(exists.to_string()),
                Some(value.is_some().to_string()),
            )
        }
    } else if let Some(expected) = &spec.equals {
        if value == Some(expected) {
            AssertionResult::pass(assertion, format!("Header '{}' matches", spec.name))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Header '{}' mismatch", spec.name),
                Some(expected.clone()),
                value.map(String::clone),
            )
        }
    } else if let Some(substr) = &spec.contains {
        if value.map(|v| v.contains(substr)).unwrap_or(false) {
            AssertionResult::pass(
                assertion,
                format!("Header '{}' contains '{}'", spec.name, substr),
            )
        } else {
            AssertionResult::fail(
                assertion,
                format!("Header '{}' doesn't contain '{}'", spec.name, substr),
                Some(substr.clone()),
                value.map(String::clone),
            )
        }
    } else {
        AssertionResult::pass(assertion, "No header constraint")
    }
}

fn evaluate_body(assertion: Assertion, spec: &BodyAssertion, body: &Value) -> AssertionResult {
    let value = json_path_query(&spec.path, body);

    if let Some(exists) = spec.exists {
        if value.is_some() == exists {
            AssertionResult::pass(
                assertion,
                format!("Path '{}' exists: {}", spec.path, exists),
            )
        } else {
            AssertionResult::fail(
                assertion,
                format!("Path '{}' existence mismatch", spec.path),
                Some(exists.to_string()),
                Some(value.is_some().to_string()),
            )
        }
    } else if let Some(expected) = &spec.equals {
        if value.as_ref() == Some(expected) {
            AssertionResult::pass(assertion, format!("Path '{}' matches", spec.path))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Path '{}' mismatch", spec.path),
                Some(serde_json::to_string(expected).unwrap_or_default()),
                value.map(|v| serde_json::to_string(&v).unwrap_or_default()),
            )
        }
    } else if let Some(substr) = &spec.contains {
        let str_val = value.and_then(|v| v.as_str().map(String::from));
        if str_val.as_ref().map(|s| s.contains(substr)).unwrap_or(false) {
            AssertionResult::pass(
                assertion,
                format!("Path '{}' contains '{}'", spec.path, substr),
            )
        } else {
            AssertionResult::fail(
                assertion,
                format!("Path '{}' doesn't contain '{}'", spec.path, substr),
                Some(substr.clone()),
                str_val,
            )
        }
    } else if let Some(expected_type) = &spec.r#type {
        let actual_type = value.as_ref().map(json_type);
        if actual_type.as_ref() == Some(expected_type) {
            AssertionResult::pass(assertion, format!("Path '{}' is type {}", spec.path, expected_type))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Path '{}' type mismatch", spec.path),
                Some(expected_type.clone()),
                actual_type,
            )
        }
    } else {
        AssertionResult::pass(assertion, "No body constraint")
    }
}

fn evaluate_duration(
    assertion: Assertion,
    spec: &DurationAssertion,
    actual_ms: u64,
) -> AssertionResult {
    if let Some(max) = spec.less_than_ms {
        if actual_ms < max {
            AssertionResult::pass(assertion, format!("Duration {}ms < {}ms", actual_ms, max))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Duration too long"),
                Some(format!("< {}ms", max)),
                Some(format!("{}ms", actual_ms)),
            )
        }
    } else if let Some(min) = spec.greater_than_ms {
        if actual_ms > min {
            AssertionResult::pass(assertion, format!("Duration {}ms > {}ms", actual_ms, min))
        } else {
            AssertionResult::fail(
                assertion,
                format!("Duration too short"),
                Some(format!("> {}ms", min)),
                Some(format!("{}ms", actual_ms)),
            )
        }
    } else {
        AssertionResult::pass(assertion, "No duration constraint")
    }
}

fn json_path_query(path: &str, value: &Value) -> Option<Value> {
    // Simple JSONPath implementation for $.field.nested
    let path = path.trim_start_matches("$.");
    let mut current = value.clone();
    
    for segment in path.split('.') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        
        // Handle array index: field[0]
        if let Some(idx_start) = segment.find('[') {
            let field = &segment[..idx_start];
            let idx_str = &segment[idx_start + 1..segment.len() - 1];
            
            if !field.is_empty() {
                current = current.get(field)?.clone();
            }
            
            if let Ok(idx) = idx_str.parse::<usize>() {
                current = current.get(idx)?.clone();
            }
        } else {
            current = current.get(segment)?.clone();
        }
    }
    
    Some(current)
}

fn json_type(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(_) => "boolean".into(),
        Value::Number(_) => "number".into(),
        Value::String(_) => "string".into(),
        Value::Array(_) => "array".into(),
        Value::Object(_) => "object".into(),
    }
}

/// Variable extraction from response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableExtraction {
    pub name: String,
    pub from: String,
}

pub fn extract_variables(
    extractions: &[VariableExtraction],
    response: &ResponseData,
) -> BTreeMap<String, Value> {
    let mut vars = BTreeMap::new();
    for ext in extractions {
        if let Some(value) = json_path_query(&ext.from, &response.body) {
            vars.insert(ext.name.clone(), value);
        }
    }
    vars
}

/// Test case definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: TestId,
    pub name: String,
    pub request_id: String,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub extract: Vec<VariableExtraction>,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for TestCase {
    fn default() -> Self {
        Self {
            id: format!("test_{}", Uuid::new_v4().simple()),
            name: String::new(),
            request_id: String::new(),
            assertions: Vec::new(),
            extract: Vec::new(),
            enabled: true,
        }
    }
}

/// Test suite definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub id: SuiteId,
    pub name: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub tests: Vec<TestCase>,
    #[serde(default)]
    pub stop_on_failure: bool,
    #[serde(default)]
    pub enabled: bool,
}

impl Default for TestSuite {
    fn default() -> Self {
        Self {
            id: format!("suite_{}", Uuid::new_v4().simple()),
            name: String::new(),
            environment: None,
            tests: Vec::new(),
            stop_on_failure: false,
            enabled: true,
        }
    }
}

/// Test result for a single test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_id: TestId,
    pub test_name: String,
    pub passed: bool,
    pub assertion_results: Vec<AssertionResult>,
    pub extracted_variables: BTreeMap<String, Value>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub executed_at: DateTime<Utc>,
}

impl TestResult {
    pub fn success(
        test: &TestCase,
        assertion_results: Vec<AssertionResult>,
        extracted_variables: BTreeMap<String, Value>,
        duration_ms: u64,
    ) -> Self {
        let passed = assertion_results.iter().all(|r| r.passed);
        Self {
            test_id: test.id.clone(),
            test_name: test.name.clone(),
            passed,
            assertion_results,
            extracted_variables,
            duration_ms,
            error: None,
            executed_at: Utc::now(),
        }
    }

    pub fn failure(test: &TestCase, error: String) -> Self {
        Self {
            test_id: test.id.clone(),
            test_name: test.name.clone(),
            passed: false,
            assertion_results: Vec::new(),
            extracted_variables: BTreeMap::new(),
            duration_ms: 0,
            error: Some(error),
            executed_at: Utc::now(),
        }
    }
}

/// Suite result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    pub suite_id: SuiteId,
    pub suite_name: String,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total_duration_ms: u64,
    pub test_results: Vec<TestResult>,
    pub executed_at: DateTime<Utc>,
}

impl SuiteResult {
    pub fn new(suite: &TestSuite, test_results: Vec<TestResult>) -> Self {
        let passed = test_results.iter().filter(|r| r.passed).count();
        let failed = test_results.iter().filter(|r| !r.passed).count();
        let total_duration_ms = test_results.iter().map(|r| r.duration_ms).sum();

        Self {
            suite_id: suite.id.clone(),
            suite_name: suite.name.clone(),
            passed,
            failed,
            skipped: 0,
            total_duration_ms,
            test_results,
            executed_at: Utc::now(),
        }
    }

    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// Test suite file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuiteFile {
    pub name: String,
    #[serde(default)]
    pub environment: Option<String>,
    pub tests: Vec<TestCaseRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseRef {
    pub request: String,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub extract: Option<BTreeMap<String, String>>,
}

/// Load test suite from YAML file
pub fn load_test_suite(path: &Path) -> anyhow::Result<TestSuite> {
    let content = std::fs::read_to_string(path)?;
    let file: TestSuiteFile = serde_yaml::from_str(&content)?;
    
    let tests = file
        .tests
        .into_iter()
        .map(|t| TestCase {
            id: format!("test_{}", Uuid::new_v4().simple()),
            name: t.request.clone(),
            request_id: t.request,
            assertions: t.assertions,
            extract: t
                .extract
                .unwrap_or_default()
                .into_iter()
                .map(|(name, from)| VariableExtraction { name, from })
                .collect(),
            enabled: true,
        })
        .collect();

    Ok(TestSuite {
        id: format!("suite_{}", Uuid::new_v4().simple()),
        name: file.name,
        environment: file.environment,
        tests,
        stop_on_failure: false,
        enabled: true,
    })
}

/// Generate JUnit XML report
pub fn generate_junit_report(results: &[SuiteResult]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    
    let total_tests: usize = results.iter().map(|r| r.test_results.len()).sum();
    let total_failures: usize = results.iter().map(|r| r.failed).sum();
    let total_time: f64 = results.iter().map(|r| r.total_duration_ms as f64 / 1000.0).sum();
    
    xml.push_str(&format!(
        r#"<testsuites tests="{}" failures="{}" time="{:.3}">"#,
        total_tests, total_failures, total_time
    ));
    xml.push('\n');

    for suite in results {
        xml.push_str(&format!(
            r#"  <testsuite name="{}" tests="{}" failures="{}" time="{:.3}">"#,
            escape_xml(&suite.suite_name),
            suite.test_results.len(),
            suite.failed,
            suite.total_duration_ms as f64 / 1000.0
        ));
        xml.push('\n');

        for test in &suite.test_results {
            xml.push_str(&format!(
                r#"    <testcase name="{}" time="{:.3}">"#,
                escape_xml(&test.test_name),
                test.duration_ms as f64 / 1000.0
            ));
            
            if !test.passed {
                if let Some(err) = &test.error {
                    xml.push_str(&format!(
                        r#"<failure message="{}"/>"#,
                        escape_xml(err)
                    ));
                } else {
                    let failed_assertions: Vec<_> = test
                        .assertion_results
                        .iter()
                        .filter(|r| !r.passed)
                        .collect();
                    for assertion in failed_assertions {
                        xml.push_str(&format!(
                            r#"<failure message="{}"/>"#,
                            escape_xml(&assertion.message)
                        ));
                    }
                }
            }
            
            xml.push_str("</testcase>\n");
        }

        xml.push_str("  </testsuite>\n");
    }

    xml.push_str("</testsuites>\n");
    xml
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Format test results for display
pub fn format_test_results(results: &SuiteResult) -> String {
    let mut output = String::new();
    output.push_str(&format!("{}\n", results.suite_name));
    
    for test in &results.test_results {
        let status = if test.passed { "PASS" } else { "FAIL" };
        output.push_str(&format!("{}  {}\n", status, test.test_name));
        
        if !test.passed {
            for assertion in &test.assertion_results {
                if !assertion.passed {
                    output.push_str(&format!("      {}\n", assertion.message));
                    if let Some(expected) = &assertion.expected {
                        output.push_str(&format!("      Expected: {}\n", expected));
                    }
                    if let Some(actual) = &assertion.actual {
                        output.push_str(&format!("      Actual: {}\n", actual));
                    }
                }
            }
        }
    }
    
    output.push_str(&format!(
        "{} passed\n{} failed\nDuration:\n  {} ms\n",
        results.passed, results.failed, results.total_duration_ms
    ));
    
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_assertion_pass() {
        let assertion = Assertion::Status(StatusAssertion {
            equals: Some(200),
            range: None,
        });
        let response = ResponseData {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({}),
            duration_ms: 50,
        };
        let result = evaluate_assertion(&assertion, &response);
        assert!(result.passed);
    }

    #[test]
    fn status_assertion_fail() {
        let assertion = Assertion::Status(StatusAssertion {
            equals: Some(201),
            range: None,
        });
        let response = ResponseData {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({}),
            duration_ms: 50,
        };
        let result = evaluate_assertion(&assertion, &response);
        assert!(!result.passed);
    }

    #[test]
    fn body_path_query() {
        let body = json!({"user": {"id": "123", "email": "test@example.com"}});
        let result = json_path_query("$.user.id", &body);
        assert_eq!(result, Some(json!("123")));
    }

    #[test]
    fn body_assertion_exists() {
        let assertion = Assertion::Body(BodyAssertion {
            path: "$.id".into(),
            exists: Some(true),
            equals: None,
            contains: None,
            r#type: None,
        });
        let response = ResponseData {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({"id": "123"}),
            duration_ms: 50,
        };
        let result = evaluate_assertion(&assertion, &response);
        assert!(result.passed);
    }

    #[test]
    fn duration_assertion() {
        let assertion = Assertion::Duration(DurationAssertion {
            less_than_ms: Some(100),
            greater_than_ms: None,
        });
        let response = ResponseData {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({}),
            duration_ms: 50,
        };
        let result = evaluate_assertion(&assertion, &response);
        assert!(result.passed);
    }

    #[test]
    fn variable_extraction() {
        let extractions = vec![VariableExtraction {
            name: "userId".into(),
            from: "$.id".into(),
        }];
        let response = ResponseData {
            status: 200,
            headers: BTreeMap::new(),
            body: json!({"id": "usr_123"}),
            duration_ms: 50,
        };
        let vars = extract_variables(&extractions, &response);
        assert_eq!(vars.get("userId"), Some(&json!("usr_123")));
    }

    #[test]
    fn junit_report_generation() {
        let result = SuiteResult {
            suite_id: "s1".into(),
            suite_name: "Test Suite".into(),
            passed: 1,
            failed: 0,
            skipped: 0,
            total_duration_ms: 100,
            test_results: vec![TestResult {
                test_id: "t1".into(),
                test_name: "Test 1".into(),
                passed: true,
                assertion_results: vec![],
                extracted_variables: BTreeMap::new(),
                duration_ms: 100,
                error: None,
                executed_at: Utc::now(),
            }],
            executed_at: Utc::now(),
        };
        let xml = generate_junit_report(&[result]);
        assert!(xml.contains("testsuite"));
        assert!(xml.contains("Test Suite"));
    }
}
