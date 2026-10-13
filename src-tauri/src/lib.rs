mod actions;
mod commands;
mod session;
mod settings;
mod shell;

use std::sync::Arc;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let config = app.path().app_config_dir()?;
            app.manage(Arc::new(AppState::new(config.join("settings.json"))));
            if disklens_core::fileops::is_elevated() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_title("DiskLens (Administrator)");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_volumes,
            commands::start_scan,
            commands::cancel_scan,
            commands::stop_scan,
            commands::pause_scan,
            commands::resume_scan,
            commands::get_summary,
            commands::refresh_items,
            commands::list_children,
            commands::node_detail,
            commands::owners,
            commands::ext_stats,
            commands::largest_files,
            commands::search_items,
            commands::render_treemap,
            commands::treemap_hit,
            commands::treemap_rect,
            commands::get_settings,
            commands::set_settings,
            commands::app_info,
            commands::find_duplicates,
            commands::cancel_duplicates,
            commands::get_duplicates,
            actions::delete_items,
            actions::empty_folders,
            actions::move_items,
            actions::compress_items,
            actions::optimize_vhds,
            actions::compute_hashes,
            actions::prune_missing,
            actions::reveal_item,
            actions::open_item,
            actions::show_properties,
            actions::open_console,
            actions::explorer_menu,
            actions::preview_user_cleanup,
            actions::run_user_cleanup,
            actions::system_action,
            actions::save_results,
            actions::save_duplicates,
            actions::load_results,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
