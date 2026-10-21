mod ai_context;
mod ai_tools;
mod config;
mod protocol;
mod pty;
mod search;
mod shells;
mod state;
mod watcher;
mod workspace;
mod wsl;

// Windows 版と WSL 用エージェントで共有する処理
use markshell_core::{links, paths, textio};

use state::AppState;

/// `mark-shell.exe --ai-context`（Claude Code のフック）の処理
pub fn print_ai_context() {
    ai_context::print_for_hook();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::new())
        .register_uri_scheme_protocol("mdimg", protocol::handle)
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::set_config,
            config::set_locale,
            config::load_workspace_state,
            config::save_workspace_state,
            config::get_recent,
            config::remove_recent,
            config::write_recovery,
            config::delete_recovery,
            config::list_recovery,
            config::register_explorer_menu,
            config::is_explorer_menu_registered,
            workspace::get_launch_root,
            workspace::open_root,
            workspace::rescan,
            workspace::read_file,
            workspace::write_file,
            workspace::create_file,
            workspace::create_dir,
            workspace::rename_path,
            workspace::count_link_updates,
            workspace::delete_path,
            workspace::duplicate_path,
            workspace::import_paths,
            workspace::save_asset,
            workspace::copy_asset,
            workspace::path_exists,
            workspace::reveal_in_explorer,
            workspace::absolute_path,
            workspace::open_new_window,
            workspace::is_installed,
            workspace::open_with_default_app,
            search::search_start,
            search::search_cancel,
            search::replace_in_files,
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            shells::list_shells,
            ai_tools::list_ai_tools,
            ai_tools::launch_ide_tool,
            ai_tools::ai_tools_file_path,
            ai_context::set_ai_context,
        ])
        .build(tauri::generate_context!())
        .expect("mark-shell の起動に失敗しました")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                ai_context::remove_context_file();
            }
        });
}
