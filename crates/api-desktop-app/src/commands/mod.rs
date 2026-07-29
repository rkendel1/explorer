//! Typed Tauri commands for the desktop application.
//!
//! These commands provide a typed bridge between the React frontend and Rust backend.
//! All business logic remains in the existing Rust crates.

pub mod change;
pub mod contract;
pub mod environment;
pub mod journey;
pub mod project;
pub mod request;
pub mod runtime;
pub mod test;
pub mod vault;
pub mod workflow;

#[cfg(feature = "tauri")]
pub type AppState<'a> = tauri::State<'a, std::sync::Arc<crate::state::DesktopStateManager>>;
#[cfg(not(feature = "tauri"))]
pub type AppState<'a> = std::sync::Arc<crate::state::DesktopStateManager>;

use serde::{Deserialize, Serialize};

/// Common command result wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum CommandResult<T> {
    #[serde(rename = "ok")]
    Ok(T),
    #[serde(rename = "error")]
    Error(CommandError),
}

impl<T> CommandResult<T> {
    pub fn ok(value: T) -> Self {
        CommandResult::Ok(value)
    }

    pub fn error(message: impl Into<String>) -> Self {
        CommandResult::Error(CommandError {
            code: "error".to_string(),
            message: message.into(),
        })
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        CommandResult::Error(CommandError {
            code: "not_found".to_string(),
            message: message.into(),
        })
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        CommandResult::Error(CommandError {
            code: "unauthorized".to_string(),
            message: message.into(),
        })
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        CommandResult::Error(CommandError {
            code: "validation_error".to_string(),
            message: message.into(),
        })
    }
}

impl<T> From<anyhow::Error> for CommandResult<T> {
    fn from(err: anyhow::Error) -> Self {
        CommandResult::error(err.to_string())
    }
}

/// Command error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}
