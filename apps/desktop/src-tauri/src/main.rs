//! Repo API Desktop Application
//!
//! This is the entry point for the Tauri desktop application.
//! It integrates with api-desktop-app to expose typed commands to the frontend.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use api_desktop_app::commands::*;
use api_desktop_app::state::DesktopStateManager;
use std::sync::Arc;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("repo-api-desktop"));
            let _ = std::fs::create_dir_all(&app_data_dir);
            app.manage(Arc::new(DesktopStateManager::new(app_data_dir)));
            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            // Project commands
            project::project_list,
            project::project_open,
            project::project_create,
            project::project_close,
            // Contract commands
            contract::contract_get,
            contract::endpoint_list,
            contract::endpoint_get,
            contract::schema_get,
            // Environment commands
            environment::environment_list,
            environment::environment_select,
            environment::environment_update,
            // Request commands
            request::request_execute,
            request::request_save,
            request::request_history,
            request::request_delete,
            // Vault commands
            vault::vault_list,
            vault::vault_create,
            vault::vault_update,
            vault::vault_delete,
            vault::vault_unlock,
            vault::vault_lock,
            vault::vault_reveal,
            // Workflow commands
            workflow::workflow_list,
            workflow::workflow_get,
            workflow::workflow_start,
            workflow::workflow_resume,
            workflow::workflow_emit_event,
            // Runtime commands
            runtime::runtime_status,
            runtime::runtime_start,
            runtime::runtime_stop,
            runtime::runtime_restart,
            runtime::runtime_reset,
            // Test commands
            test::test_list,
            test::test_run,
            test::test_result,
            // Change commands
            change::change_list,
            change::change_review,
            change::change_accept,
            change::change_reject,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
