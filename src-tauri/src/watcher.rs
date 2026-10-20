//! ルートディレクトリ以下の変更を監視し、UI に差分を送る（Windows のフォルダのとき。WSL ではエージェントが行う）

use std::path::PathBuf;
use std::sync::Arc;

use markshell_core::watch::{self, Change, WatchEvent, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::paths::FilterConfig;

#[derive(Serialize, Clone)]
struct ChangeBatch {
    changes: Vec<Change>,
}

pub fn start(app: AppHandle, root: PathBuf, filter: Arc<FilterConfig>) -> Result<Watcher, String> {
    watch::start(root, filter, move |ev| match ev {
        WatchEvent::Changes(changes) => {
            let _ = app.emit("fs:change", ChangeBatch { changes });
        }
        WatchEvent::Rescan => {
            let _ = app.emit("fs:rescan", ());
        }
    })
    .map_err(|e| e.to_string())
}

