//! Secret redaction service for preventing accidental exposure of sensitive data.
//!
//! This module provides a centralized redaction service that ensures secrets
//! are never accidentally exposed in:
//! - Request/response history
//! - Runtime events
//! - Logs and error messages
//! - Notifications
//! - Test reports
//! - Exported diagnostics
//! - Frontend command responses

use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Pattern for identifying common secret headers
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "api-key",
    "apikey",
    "x-auth-token",
    "x-access-token",
    "bearer",
    "cookie",
    "set-cookie",
    "x-csrf-token",
    "x-xsrf-token",
];

/// Pattern for identifying secret-like field names in JSON
const SECRET_FIELDS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "accesstoken",
    "refresh_token",
    "refreshtoken",
    "private_key",
    "privatekey",
    "credential",
    "auth",
    "bearer",
    "jwt",
    "session",
    "cookie",
];

/// Redacted placeholder text
pub const REDACTED: &str = "[REDACTED]";

/// Secret redaction service that tracks known secrets for redaction
#[derive(Clone, Default)]
pub struct RedactionService {
    /// Known secret values that should be redacted
    known_secrets: Arc<RwLock<HashSet<String>>>,
}

impl RedactionService {
    /// Create a new redaction service
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a secret value to be redacted
    pub fn register_secret(&self, secret: &str) {
        if !secret.is_empty() {
            let mut secrets = self.known_secrets.write().unwrap();
            secrets.insert(secret.to_string());
        }
    }

    /// Unregister a secret value
    pub fn unregister_secret(&self, secret: &str) {
        let mut secrets = self.known_secrets.write().unwrap();
        secrets.remove(secret);
    }

    /// Clear all registered secrets (e.g., on vault lock)
    pub fn clear_secrets(&self) {
        let mut secrets = self.known_secrets.write().unwrap();
        secrets.clear();
    }

    /// Redact known secrets from a string
    pub fn redact_string(&self, input: &str) -> String {
        let secrets = self.known_secrets.read().unwrap();
        let mut result = input.to_string();
        for secret in secrets.iter() {
            if !secret.is_empty() && result.contains(secret) {
                result = result.replace(secret, REDACTED);
            }
        }
        result
    }

    /// Redact secrets from a JSON value
    pub fn redact_json(&self, value: &Value) -> Value {
        self.redact_json_internal(value, false)
    }

    fn redact_json_internal(&self, value: &Value, in_secret_context: bool) -> Value {
        match value {
            Value::String(s) => {
                if in_secret_context {
                    Value::String(REDACTED.to_string())
                } else {
                    Value::String(self.redact_string(s))
                }
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|v| self.redact_json_internal(v, in_secret_context))
                    .collect(),
            ),
            Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (key, val) in map {
                    let is_secret_field = is_secret_field_name(key);
                    new_map.insert(key.clone(), self.redact_json_internal(val, is_secret_field));
                }
                Value::Object(new_map)
            }
            _ => value.clone(),
        }
    }

    /// Redact headers, ensuring authorization and other sensitive headers are redacted
    pub fn redact_headers(&self, headers: &[(String, String)]) -> Vec<(String, String)> {
        headers
            .iter()
            .map(|(name, value)| {
                let lower_name = name.to_lowercase();
                if SECRET_HEADERS.iter().any(|h| lower_name.contains(h)) {
                    (name.clone(), REDACTED.to_string())
                } else {
                    (name.clone(), self.redact_string(value))
                }
            })
            .collect()
    }
}

/// Check if a field name looks like it contains a secret
fn is_secret_field_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    SECRET_FIELDS.iter().any(|s| lower.contains(s))
}

/// Redact a secret value showing only last few characters
pub fn redact_preview(secret: &str) -> String {
    if secret.is_empty() {
        return String::new();
    }
    let visible = secret.chars().count().min(4);
    let suffix: String = secret
        .chars()
        .rev()
        .take(visible)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••{}", suffix)
}

/// Redact authorization header value
pub fn redact_auth_header(value: &str) -> String {
    if value.to_lowercase().starts_with("bearer ") {
        "******".to_string()
    } else if value.to_lowercase().starts_with("basic ") {
        format!("Basic {}", REDACTED)
    } else {
        REDACTED.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_known_secret_from_string() {
        let service = RedactionService::new();
        service.register_secret("my-secret-token");

        let input = "Authorization: my-secret-token";
        let output = service.redact_string(input);

        assert!(!output.contains("my-secret-token"));
        assert!(output.contains(REDACTED));
    }

    #[test]
    fn redact_secret_fields_in_json() {
        let service = RedactionService::new();

        let input = json!({
            "username": "testuser",
            "password": "super-secret",
            "api_key": "key-12345"
        });

        let output = service.redact_json(&input);

        assert_eq!(output["username"], "testuser");
        assert_eq!(output["password"], REDACTED);
        assert_eq!(output["api_key"], REDACTED);
    }

    #[test]
    fn redact_headers() {
        let service = RedactionService::new();
        service.register_secret("secret-value");

        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), "******".to_string()),
            ("X-API-Key".to_string(), "another-secret".to_string()),
            (
                "X-Custom".to_string(),
                "contains secret-value here".to_string(),
            ),
        ];

        let redacted = service.redact_headers(&headers);

        assert_eq!(redacted[0].1, "application/json");
        assert_eq!(redacted[1].1, REDACTED);
        assert_eq!(redacted[2].1, REDACTED);
        assert!(redacted[3].1.contains(REDACTED));
    }

    #[test]
    fn redact_preview_shows_suffix() {
        assert_eq!(redact_preview("secret-token-12345"), "••••2345");
        assert_eq!(redact_preview("ab"), "••••ab");
        assert_eq!(redact_preview(""), "");
    }

    #[test]
    fn redact_auth_header_formats() {
        // ****** are fully redacted to asterisks
        assert_eq!(redact_auth_header("Bearer xyz"), "******");
        // Basic auth shows scheme but redacts credentials
        assert_eq!(redact_auth_header("Basic dXNlcjpwYXNz"), "Basic [REDACTED]");
        // Other auth types are fully redacted
        assert_eq!(redact_auth_header("other-auth"), "[REDACTED]");
    }
}
