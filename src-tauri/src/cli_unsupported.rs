//! コマンド `mark-shell` の登録（Windows 以外）。今は Windows だけで使える

use markshell_core::tr;

#[tauri::command]
pub async fn register_cli(enable: bool) -> Result<(), String> {
    if enable {
        Err(tr!("この機能は Windows だけで使えます", "This feature is only available on Windows", "此功能仅适用于 Windows"))
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn is_cli_registered() -> bool {
    false
}

pub fn unregister() {}
