mod ai_context;
mod ai_tools;
mod cli;
mod config;
mod git;
mod memory;
mod protocol;
mod pty;
mod rate_limits;
mod search;
mod session;
mod shells;
mod state;
mod titlebar;
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

/// `mark-shell.exe --ai-usage <id>` の処理
pub fn print_ai_usage(tool_id: &str) {
    ai_tools::print_for_usage(tool_id);
}

/// `mark-shell.exe --statusline`（Claude Code のステータスライン）の処理
pub fn print_statusline() {
    rate_limits::print_for_statusline();
}

/// `mark-shell.exe --unregister-cli`（アンインストーラー）の処理
pub fn unregister_cli() {
    cli::unregister();
}

/// Mac で Finder / Dock から起動したときは PATH が最小限（/usr/bin:/bin など）なので、
/// ログインシェルと標準の場所から PATH を補う（git・認証ヘルパー・フック・AI ツールが見つかるようにする）
#[cfg(not(windows))]
fn enrich_path() {
    let mut dirs: Vec<String> = std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    if let Ok(out) = std::process::Command::new(&shell)
        .args(["-ilc", "printf '%s' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    {
        if out.status.success() {
            for p in String::from_utf8_lossy(&out.stdout).split(':').filter(|s| !s.is_empty()) {
                if !dirs.iter().any(|d| d == p) {
                    dirs.push(p.to_string());
                }
            }
        }
    }
    let mut extra = vec!["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string()];
    if let Some(home) = dirs::home_dir() {
        extra.insert(0, home.join(".local/bin").to_string_lossy().into_owned());
        extra.push(home.join(".cargo/bin").to_string_lossy().into_owned());
    }
    for p in extra {
        if !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    }
    std::env::set_var("PATH", dirs.join(":"));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(not(windows))]
    enrich_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::new())
        .on_window_event(memory::on_window_event)
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
            cli::register_cli,
            cli::is_cli_registered,
            workspace::get_launch_target,
            workspace::open_root,
            workspace::rescan,
            workspace::list_dir,
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
            titlebar::set_title_bar_colors,
            workspace::is_installed,
            workspace::open_with_default_app,
            session::take_restore_windows,
            session::mark_update_restart,
            session::cancel_update_restart,
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
            ai_tools::run_ai_usage,
            ai_context::set_ai_context,
            git::git_repos,
            git::git_status,
            git::git_log,
            git::git_commit_detail,
            git::git_compare,
            git::git_ls_tree,
            git::git_cherry,
            git::git_show_file,
            git::git_refs,
            git::git_stashes,
            git::git_stash_files,
            git::git_config_get,
            git::git_op,
            git::git_cancel,
            git::git_watch,
            git::git_load_state,
            git::git_save_state,
        ])
        .build(tauri::generate_context!())
        .expect("mark-shell の起動に失敗しました")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                session::mark_closed();
                ai_context::remove_context_file();
            }
        });
}
