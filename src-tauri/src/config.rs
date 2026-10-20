//! 設定と状態の保存。ルートディレクトリには何も書かず、%APPDATA% / %LOCALAPPDATA% に置く

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::paths::FilterConfig;
use crate::state::AppState;
use crate::textio::write_atomic;

const MAX_RECENT: usize = 15;

/// %APPDATA%\mark-shell
pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("mark-shell")
}

/// %LOCALAPPDATA%\mark-shell
pub fn local_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("mark-shell")
}

fn read_json(path: &Path) -> Option<Value> {
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    write_atomic(path, s.as_bytes()).map_err(|e| e.to_string())
}

/// パスごとに安定したキー（FNV-1a 64bit。大文字小文字は区別しない）
pub fn path_key(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.to_lowercase().bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn read_config() -> Value {
    read_json(&config_dir().join("config.json")).unwrap_or_else(|| json!({}))
}

#[tauri::command]
pub fn get_config() -> Value {
    read_config()
}

#[tauri::command]
pub fn set_config(state: State<'_, AppState>, config: Value) -> Result<(), String> {
    write_json(&config_dir().join("config.json"), &config)?;
    *state.filter.write().unwrap() = Arc::new(FilterConfig::from_config(&config));
    if let Some(wsl) = state.wsl() {
        wsl.agent.request("config", serde_json::json!({ "config": config }))?;
    }
    Ok(())
}

fn workspace_state_path(root: &str) -> PathBuf {
    local_dir().join("workspaces").join(format!("{}.json", path_key(root)))
}

#[tauri::command]
pub fn load_workspace_state(root: String) -> Value {
    read_json(&workspace_state_path(&root)).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn save_workspace_state(root: String, value: Value) -> Result<(), String> {
    write_json(&workspace_state_path(&root), &value)
}

fn recent_path() -> PathBuf {
    local_dir().join("recent.json")
}

fn read_recent() -> Vec<String> {
    read_json(&recent_path())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

pub fn push_recent(root: &str) {
    let mut list = read_recent();
    list.retain(|p| !p.eq_ignore_ascii_case(root));
    list.insert(0, root.to_string());
    list.truncate(MAX_RECENT);
    let _ = write_json(&recent_path(), &json!(list));
}

#[tauri::command]
pub fn get_recent() -> Vec<String> {
    read_recent()
        .into_iter()
        .filter(|p| Path::new(p).is_dir())
        .collect()
}

#[tauri::command]
pub fn remove_recent(path: String) -> Result<(), String> {
    let mut list = read_recent();
    list.retain(|p| !p.eq_ignore_ascii_case(&path));
    write_json(&recent_path(), &json!(list))
}

// ---- 異常終了に備えた編集内容の退避 ----

fn recovery_dir(root: &Path) -> PathBuf {
    local_dir()
        .join("recovery")
        .join(path_key(&root.to_string_lossy()))
}

#[tauri::command]
pub fn write_recovery(state: State<'_, AppState>, path: String, content: String) -> Result<(), String> {
    let root = state.root()?;
    let file = recovery_dir(&root).join(format!("{}.json", path_key(&path)));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    write_json(&file, &json!({ "path": path, "content": content, "time": now }))
}

#[tauri::command]
pub fn delete_recovery(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = state.root()?;
    let file = recovery_dir(&root).join(format!("{}.json", path_key(&path)));
    if file.exists() {
        fs::remove_file(file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Serialize)]
pub struct RecoveryEntry {
    path: String,
    content: String,
    time: u64,
}

#[tauri::command]
pub fn list_recovery(state: State<'_, AppState>) -> Result<Vec<RecoveryEntry>, String> {
    let root = state.root()?;
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(recovery_dir(&root)) else {
        return Ok(out);
    };
    for e in rd.flatten() {
        if let Some(v) = read_json(&e.path()) {
            if let (Some(path), Some(content)) = (v["path"].as_str(), v["content"].as_str()) {
                out.push(RecoveryEntry {
                    path: path.to_string(),
                    content: content.to_string(),
                    time: v["time"].as_u64().unwrap_or(0),
                });
            }
        }
    }
    Ok(out)
}

// ---- エクスプローラーの右クリックメニュー（HKCU なので管理者権限は不要） ----

const MENU_KEYS: &[&str] = &[
    r"HKCU\Software\Classes\Directory\shell\mark-shell",
    r"HKCU\Software\Classes\Directory\Background\shell\mark-shell",
];

fn reg(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = std::process::Command::new("reg.exe")
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("reg.exe が失敗しました: {args:?}"))
    }
}

#[tauri::command]
pub fn register_explorer_menu(enable: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy();
    for (i, key) in MENU_KEYS.iter().enumerate() {
        if enable {
            let arg = if i == 0 { "%1" } else { "%V" };
            reg(&["add", key, "/ve", "/d", "mark-shell で開く", "/f"])?;
            reg(&["add", key, "/v", "Icon", "/d", &exe, "/f"])?;
            let cmd_key = format!(r"{key}\command");
            let cmd = format!("\"{exe}\" \"{arg}\"");
            reg(&["add", &cmd_key, "/ve", "/d", &cmd, "/f"])?;
        } else {
            let _ = reg(&["delete", key, "/f"]);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn is_explorer_menu_registered() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("reg.exe")
        .args(["query", MENU_KEYS[0]])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
