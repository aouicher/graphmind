mod commands;
mod state;
mod tray;
mod types;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Mutex::new(AppState::default()))
        .setup(|app| {
            tray::create_tray(app)?;

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auto_update(handle).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::projects::list_projects,
            commands::projects::add_project,
            commands::projects::remove_project,
            commands::projects::get_project_status,
            commands::indexing::build_project,
            commands::indexing::build_all_projects,
            commands::indexing::embed_projects,
            commands::watcher::start_watching,
            commands::watcher::stop_watching,
            commands::watcher::get_watch_status,
            commands::integrations::detect_clients,
            commands::integrations::install_mcp_for_client,
            commands::integrations::uninstall_mcp_for_client,
            commands::setup::check_cli_installed,
            commands::setup::install_cli,
            commands::setup::ensure_cli_in_path,
            commands::setup::check_cli_update,
            commands::setup::update_cli,
            commands::setup::get_cli_path,
            commands::graph::get_graph_data,
            commands::settings::get_excludes,
            commands::settings::set_global_excludes,
            commands::settings::set_project_excludes,
            commands::settings::get_app_version,
            commands::settings::get_hook_status,
            commands::settings::install_claude_hook,
            commands::settings::uninstall_claude_hook,
            commands::settings::get_git_hook_status,
            commands::settings::install_git_hook,
            commands::settings::uninstall_git_hook,
            commands::settings::install_skill,
            commands::settings::get_skill_status,
            commands::settings::get_claude_md_status,
            commands::settings::get_embedding_settings,
            commands::settings::set_embedding_settings,
            commands::updater::check_app_update,
            commands::updater::install_app_update,
            commands::notices::check_setup_status,
            commands::notices::run_setup,
            commands::notices::check_announcements,
            commands::notices::dismiss_announcement,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn auto_update(app: tauri::AppHandle) {
    // Small delay to let the app finish loading
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let updater = match app.updater() {
        Ok(u) => u,
        Err(_) => return,
    };

    let update = match updater.check().await {
        Ok(Some(u)) => u,
        _ => return,
    };

    let new_version = update.version.clone();

    // Auto-install the app update
    if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
        // Also update CLI
        commands::updater::update_cli_after_app(&new_version);

        // Notify frontend
        if let Some(window) = app.get_webview_window("main") {
            use tauri::Emitter;
            window.emit("app-updated", &new_version).ok();
        }
    }
}
