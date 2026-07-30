//! Repo API Desktop Application
//!
//! This is the entry point for the Tauri desktop application.
//! It integrates with api-desktop-app to expose typed commands to the frontend.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::Arc;

use tauri::Manager;

// Glob imports (not `{name, ...}` lists) are required here: `#[tauri::command]`
// re-exports a hidden `__cmd__<name>` helper macro alongside each function at
// the same visibility, and `generate_handler!` needs that macro in scope too.
// A named import only brings in the function, not its sibling macro.
use api_desktop_app::commands::change::*;
use api_desktop_app::commands::contract::*;
use api_desktop_app::commands::environment::*;
use api_desktop_app::commands::journey::*;
use api_desktop_app::commands::project::*;
use api_desktop_app::commands::request::*;
use api_desktop_app::commands::runtime::*;
use api_desktop_app::commands::test::*;
use api_desktop_app::commands::vault::*;
use api_desktop_app::commands::workflow::*;
use api_desktop_app::services::ProjectService;
use api_desktop_app::state::DesktopStateManager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir)?;

            let state = Arc::new(DesktopStateManager::new(app_data_dir));
            app.manage(state.clone());

            // Restore the previously open project (if any) without blocking startup.
            tauri::async_runtime::spawn(async move {
                if let Err(err) = ProjectService::restore_project(&state).await {
                    eprintln!("failed to restore project state: {err}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            change_list,
            change_review,
            change_accept,
            change_reject,
            change_accept_all,
            change_keep_current,
            endpoint_list,
            endpoint_get,
            schema_list,
            schema_get,
            contract_get,
            contract_rescan,
            environment_list,
            environment_select,
            environment_update,
            environment_create,
            environment_delete,
            journey_get,
            journey_state,
            journey_select_goal,
            journey_complete_outcome,
            journey_defer_action,
            journey_progress,
            project_list,
            project_open,
            project_create,
            project_close,
            project_remove_recent,
            request_execute,
            request_save,
            request_saved_list,
            request_history,
            request_delete,
            request_history_clear,
            runtime_status,
            runtime_start,
            runtime_stop,
            runtime_restart,
            runtime_reset,
            runtime_events,
            runtime_metrics,
            runtime_export_state,
            runtime_import_state,
            test_list,
            test_run,
            test_result,
            test_export,
            test_prepare_onboarding,
            vault_list,
            vault_create,
            vault_delete,
            vault_unlock,
            vault_lock,
            vault_state,
            workflow_list,
            workflow_get,
            workflow_start,
            workflow_resume,
            workflow_handle_event,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
