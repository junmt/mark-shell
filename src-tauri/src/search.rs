//! 全文検索と一括置換。処理は markshell_core にあり、WSL のフォルダではエージェントに任せる

use std::sync::atomic::Ordering;

use markshell_core::search::{self, Hit, ReplaceResult, SearchOptions};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

#[derive(Serialize, Clone)]
struct SearchBatch {
    id: u64,
    hits: Vec<Hit>,
}

#[derive(Serialize, Clone)]
struct SearchDone {
    id: u64,
    total: usize,
    truncated: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn search_start(app: AppHandle, state: State<'_, AppState>, options: SearchOptions) -> Result<u64, String> {
    let root = state.root()?;
    let filter = state.filter();
    let id = state.search_gen.fetch_add(1, Ordering::SeqCst) + 1;
    if options.query.is_empty() {
        return Ok(id);
    }
    // 結果は search:batch / search:done で届く
    if let Some(wsl) = state.wsl() {
        wsl.agent.request("search", json!({ "id": id, "options": options }))?;
        return Ok(id);
    }
    let gen = state.search_gen.clone();
    std::thread::spawn(move || {
        let cancelled = || gen.load(Ordering::Relaxed) != id;
        let r = search::search(&root, &filter, &options, cancelled, |hits| {
            let _ = app.emit("search:batch", SearchBatch { id, hits });
        });
        if cancelled() {
            return;
        }
        let done = match r {
            Ok(s) => SearchDone { id, total: s.total, truncated: s.truncated, error: None },
            Err(e) => SearchDone { id, total: 0, truncated: false, error: Some(e) },
        };
        let _ = app.emit("search:done", done);
    });
    Ok(id)
}

#[tauri::command]
pub fn search_cancel(state: State<'_, AppState>) {
    state.search_gen.fetch_add(1, Ordering::SeqCst);
    if let Some(wsl) = state.wsl() {
        let _ = wsl.agent.request("searchCancel", json!({}));
    }
}

/// 検索結果のファイルに一括置換をかける（文字コード・改行コードは保つ）
#[tauri::command]
pub async fn replace_in_files(app: AppHandle, options: SearchOptions, replacement: String, paths: Vec<String>) -> Result<ReplaceResult, String> {
    let state = app.state::<AppState>();
    let root = state.root()?;
    let wsl = state.wsl();
    tauri::async_runtime::spawn_blocking(move || match wsl {
        Some(wsl) => {
            let r = wsl.agent.request("replace", json!({ "options": options, "replacement": replacement, "paths": paths }))?;
            serde_json::from_value(r).map_err(|e| e.to_string())
        }
        None => search::replace(&root, &options, &replacement, &paths),
    })
    .await
    .map_err(|e| e.to_string())?
}
