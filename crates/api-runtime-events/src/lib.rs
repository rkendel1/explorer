//! Runtime event models and subscriptions for API mock runtime observability.
//!
//! This crate owns:
//! - Runtime event models
//! - Event subscriptions
//! - Event filtering
//! - Event streams
//! - Event persistence adapters

use api_core::{EndpointId, HttpMethod};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

pub type RuntimeId = String;
pub type RequestId = String;
pub type EventId = String;

/// Runtime event types for mock server observability
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    RequestReceived(RequestReceivedEvent),
    RequestValidated(RequestValidatedEvent),
    ValidationFailed(ValidationFailedEvent),
    ScenarioMatched(ScenarioMatchedEvent),
    ResponseGenerated(ResponseGeneratedEvent),
    StateChanged(StateChangedEvent),
    ResponseSent(ResponseSentEvent),
    RuntimeDiagnostic(RuntimeDiagnosticEvent),
}

impl RuntimeEvent {
    pub fn event_id(&self) -> &str {
        match self {
            Self::RequestReceived(e) => &e.event_id,
            Self::RequestValidated(e) => &e.event_id,
            Self::ValidationFailed(e) => &e.event_id,
            Self::ScenarioMatched(e) => &e.event_id,
            Self::ResponseGenerated(e) => &e.event_id,
            Self::StateChanged(e) => &e.event_id,
            Self::ResponseSent(e) => &e.event_id,
            Self::RuntimeDiagnostic(e) => &e.event_id,
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::RequestReceived(e) => e.timestamp,
            Self::RequestValidated(e) => e.timestamp,
            Self::ValidationFailed(e) => e.timestamp,
            Self::ScenarioMatched(e) => e.timestamp,
            Self::ResponseGenerated(e) => e.timestamp,
            Self::StateChanged(e) => e.timestamp,
            Self::ResponseSent(e) => e.timestamp,
            Self::RuntimeDiagnostic(e) => e.timestamp,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::RequestReceived(e) => Some(&e.request_id),
            Self::RequestValidated(e) => Some(&e.request_id),
            Self::ValidationFailed(e) => Some(&e.request_id),
            Self::ScenarioMatched(e) => Some(&e.request_id),
            Self::ResponseGenerated(e) => Some(&e.request_id),
            Self::StateChanged(e) => Some(&e.request_id),
            Self::ResponseSent(e) => Some(&e.request_id),
            Self::RuntimeDiagnostic(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestReceivedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub method: HttpMethod,
    pub path: String,
    pub headers: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestValidatedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub endpoint_id: EndpointId,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFailedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub endpoint_id: Option<EndpointId>,
    pub violations: Vec<ValidationViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationViolation {
    pub location: String,
    pub rule: String,
    pub expected: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMatchedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub scenario_id: String,
    pub scenario_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseGeneratedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub source: ResponseSource,
    pub schema_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseSource {
    Scenario,
    ContractExample,
    SchemaGenerated,
    StatefulResource,
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChangedEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub resource_type: String,
    pub resource_id: String,
    pub operation: StateOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateOperation {
    Created,
    Updated,
    Replaced,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSentEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub request_id: RequestId,
    pub status: u16,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDiagnosticEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub runtime_id: RuntimeId,
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

/// Event filter for subscribing to specific event types
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub event_types: Option<Vec<String>>,
    pub endpoint_ids: Option<Vec<EndpointId>>,
    pub include_body: bool,
}

impl EventFilter {
    pub fn matches(&self, event: &RuntimeEvent) -> bool {
        if let Some(types) = &self.event_types {
            let event_type = match event {
                RuntimeEvent::RequestReceived(_) => "request_received",
                RuntimeEvent::RequestValidated(_) => "request_validated",
                RuntimeEvent::ValidationFailed(_) => "validation_failed",
                RuntimeEvent::ScenarioMatched(_) => "scenario_matched",
                RuntimeEvent::ResponseGenerated(_) => "response_generated",
                RuntimeEvent::StateChanged(_) => "state_changed",
                RuntimeEvent::ResponseSent(_) => "response_sent",
                RuntimeEvent::RuntimeDiagnostic(_) => "runtime_diagnostic",
            };
            if !types.iter().any(|t| t == event_type) {
                return false;
            }
        }
        true
    }
}

/// Event emitter for publishing runtime events
#[derive(Clone)]
pub struct EventEmitter {
    sender: broadcast::Sender<RuntimeEvent>,
    runtime_id: RuntimeId,
}

impl EventEmitter {
    pub fn new(runtime_id: RuntimeId, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender, runtime_id }
    }

    pub fn emit(&self, event: RuntimeEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            filter: EventFilter::default(),
        }
    }

    pub fn subscribe_filtered(&self, filter: EventFilter) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            filter,
        }
    }

    pub fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub fn request_received(
        &self,
        request_id: &str,
        method: HttpMethod,
        path: String,
        headers: Vec<(String, String)>,
        body_preview: Option<String>,
    ) {
        self.emit(RuntimeEvent::RequestReceived(RequestReceivedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            method,
            path,
            headers,
            body_preview,
        }));
    }

    pub fn request_validated(
        &self,
        request_id: &str,
        endpoint_id: &str,
        valid: bool,
    ) {
        self.emit(RuntimeEvent::RequestValidated(RequestValidatedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            valid,
        }));
    }

    pub fn validation_failed(
        &self,
        request_id: &str,
        endpoint_id: Option<&str>,
        violations: Vec<ValidationViolation>,
    ) {
        self.emit(RuntimeEvent::ValidationFailed(ValidationFailedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            endpoint_id: endpoint_id.map(String::from),
            violations,
        }));
    }

    pub fn scenario_matched(
        &self,
        request_id: &str,
        scenario_id: &str,
        scenario_name: &str,
    ) {
        self.emit(RuntimeEvent::ScenarioMatched(ScenarioMatchedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            scenario_id: scenario_id.to_string(),
            scenario_name: scenario_name.to_string(),
        }));
    }

    pub fn response_generated(
        &self,
        request_id: &str,
        source: ResponseSource,
        schema_id: Option<&str>,
    ) {
        self.emit(RuntimeEvent::ResponseGenerated(ResponseGeneratedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            source,
            schema_id: schema_id.map(String::from),
        }));
    }

    pub fn state_changed(
        &self,
        request_id: &str,
        resource_type: &str,
        resource_id: &str,
        operation: StateOperation,
    ) {
        self.emit(RuntimeEvent::StateChanged(StateChangedEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            operation,
        }));
    }

    pub fn response_sent(
        &self,
        request_id: &str,
        status: u16,
        duration_ms: u64,
        body_size: Option<usize>,
    ) {
        self.emit(RuntimeEvent::ResponseSent(ResponseSentEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            request_id: request_id.to_string(),
            status,
            duration_ms,
            body_size,
        }));
    }

    pub fn diagnostic(&self, level: DiagnosticLevel, code: &str, message: &str) {
        self.emit(RuntimeEvent::RuntimeDiagnostic(RuntimeDiagnosticEvent {
            event_id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            runtime_id: self.runtime_id.clone(),
            level,
            code: code.to_string(),
            message: message.to_string(),
        }));
    }
}

/// Event subscription for receiving filtered events
pub struct EventSubscription {
    receiver: broadcast::Receiver<RuntimeEvent>,
    filter: EventFilter,
}

impl EventSubscription {
    pub async fn recv(&mut self) -> Option<RuntimeEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) if self.filter.matches(&event) => return Some(event),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}

/// Runtime metrics aggregated from events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub total_requests: u64,
    pub requests_by_endpoint: std::collections::HashMap<String, u64>,
    pub status_distribution: std::collections::HashMap<u16, u64>,
    pub validation_failures: u64,
    pub scenario_matches: u64,
    pub generated_responses: u64,
    pub state_mutations: u64,
    pub total_duration_ms: u64,
    pub request_count_for_avg: u64,
}

impl RuntimeMetrics {
    pub fn average_duration_ms(&self) -> f64 {
        if self.request_count_for_avg == 0 {
            0.0
        } else {
            self.total_duration_ms as f64 / self.request_count_for_avg as f64
        }
    }

    pub fn update(&mut self, event: &RuntimeEvent) {
        match event {
            RuntimeEvent::RequestReceived(_) => {
                self.total_requests += 1;
            }
            RuntimeEvent::ValidationFailed(_) => {
                self.validation_failures += 1;
            }
            RuntimeEvent::ScenarioMatched(_) => {
                self.scenario_matches += 1;
            }
            RuntimeEvent::ResponseGenerated(e) => {
                if e.source == ResponseSource::SchemaGenerated {
                    self.generated_responses += 1;
                }
            }
            RuntimeEvent::StateChanged(_) => {
                self.state_mutations += 1;
            }
            RuntimeEvent::ResponseSent(e) => {
                *self.status_distribution.entry(e.status).or_insert(0) += 1;
                self.total_duration_ms += e.duration_ms;
                self.request_count_for_avg += 1;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_matches_all_by_default() {
        let filter = EventFilter::default();
        let event = RuntimeEvent::RequestReceived(RequestReceivedEvent {
            event_id: "e1".into(),
            timestamp: Utc::now(),
            runtime_id: "r1".into(),
            request_id: "req1".into(),
            method: HttpMethod::GET,
            path: "/users".into(),
            headers: vec![],
            body_preview: None,
        });
        assert!(filter.matches(&event));
    }

    #[test]
    fn event_filter_by_type() {
        let filter = EventFilter {
            event_types: Some(vec!["response_sent".into()]),
            ..Default::default()
        };
        let event = RuntimeEvent::RequestReceived(RequestReceivedEvent {
            event_id: "e1".into(),
            timestamp: Utc::now(),
            runtime_id: "r1".into(),
            request_id: "req1".into(),
            method: HttpMethod::GET,
            path: "/users".into(),
            headers: vec![],
            body_preview: None,
        });
        assert!(!filter.matches(&event));
    }

    #[test]
    fn metrics_update() {
        let mut metrics = RuntimeMetrics::default();
        let event = RuntimeEvent::ResponseSent(ResponseSentEvent {
            event_id: "e1".into(),
            timestamp: Utc::now(),
            runtime_id: "r1".into(),
            request_id: "req1".into(),
            status: 200,
            duration_ms: 50,
            body_size: Some(100),
        });
        metrics.update(&event);
        assert_eq!(metrics.status_distribution.get(&200), Some(&1));
        assert_eq!(metrics.average_duration_ms(), 50.0);
    }
}
