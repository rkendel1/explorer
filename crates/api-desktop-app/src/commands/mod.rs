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

/// Get an owned `Arc` handle to the state manager, for passing into service
/// functions that take `&Arc<DesktopStateManager>` regardless of whether
/// `AppState` is a `tauri::State` wrapper or a bare `Arc`.
#[cfg(feature = "tauri")]
pub(crate) fn state_handle(
    state: &AppState<'_>,
) -> std::sync::Arc<crate::state::DesktopStateManager> {
    state.inner().clone()
}
#[cfg(not(feature = "tauri"))]
pub(crate) fn state_handle(
    state: &AppState<'_>,
) -> std::sync::Arc<crate::state::DesktopStateManager> {
    state.clone()
}

use serde::{Deserialize, Serialize};

/// What every Tauri command returns.
///
/// Tauri requires async commands that take a reference input (our `AppState<'_>`
/// is one) to return an actual `Result`, so this is a plain type alias rather
/// than a custom envelope: `Ok(T)` resolves the frontend's `invoke()` promise
/// with `T`, `Err(CommandError)` rejects it. Every frontend `catch` block is
/// built to handle a rejection, so this is also the shape they expect.
pub type CommandResult<T> = std::result::Result<T, CommandError>;

/// Command error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn error(message: impl Into<String>) -> Self {
        CommandError {
            code: "error".to_string(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        CommandError {
            code: "not_found".to_string(),
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        CommandError {
            code: "unauthorized".to_string(),
            message: message.into(),
        }
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        CommandError {
            code: "validation_error".to_string(),
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(err: anyhow::Error) -> Self {
        CommandError::error(err.to_string())
    }
}

impl From<crate::services::ServiceError> for CommandError {
    fn from(err: crate::services::ServiceError) -> Self {
        use crate::services::ServiceErrorCode;
        match err.code {
            ServiceErrorCode::NotFound => CommandError::not_found(err.message),
            ServiceErrorCode::VaultLocked | ServiceErrorCode::VaultUnlockFailed => {
                CommandError::unauthorized(err.message)
            }
            ServiceErrorCode::ValidationError => CommandError::validation_error(err.message),
            _ => CommandError::error(err.message),
        }
    }
}

/// Convert a service `Result` into a `CommandResult` in one step.
pub(crate) fn from_service<T>(result: crate::services::ServiceResult<T>) -> CommandResult<T> {
    result.map_err(CommandError::from)
}
