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
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
