//! ルートディレクトリ以下の変更の監視。少しの間に起きた変更をまとめて渡す

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher as _};
use serde::{Deserialize, Serialize};

use crate::paths::{mtime_ms, to_rel, FilterConfig};
use crate::scan::{scan, Entry};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Change {
    Create(Entry),
    Modify(Entry),
    Remove { path: String },
}

/// 監視のハンドル。drop すると監視が止まる
pub type Watcher = notify::RecommendedWatcher;

pub enum WatchEvent {
    Changes(Vec<Change>),
    /// 取りこぼしがあったので全体を探索し直す
    Rescan,
}

/// 変更をまとめて送るまでの待ち時間
const COALESCE: Duration = Duration::from_millis(120);

/// 監視を始める。戻り値を drop すると止まる
pub fn start(
    root: PathBuf,
    filter: Arc<FilterConfig>,
    on_event: impl Fn(WatchEvent) + Send + 'static,
) -> notify::Result<Watcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || loop {
        // 最初のイベントを待ち、少しの間に来たものをまとめる
        let first = match rx.recv() {
            Ok(e) => e,
            Err(_) => return, // 監視が止まった
        };
        let mut events = vec![first];
        loop {
            match rx.recv_timeout(COALESCE) {
                Ok(e) => events.push(e),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let mut changes = Vec::new();
        let mut rescan = false;
        for ev in events {
            match ev {
                Ok(ev) if ev.need_rescan() => rescan = true,
                Ok(ev) => collect(&root, &filter, &ev, &mut changes),
                Err(_) => rescan = true,
            }
        }
        if rescan {
            on_event(WatchEvent::Rescan);
        }
        if !changes.is_empty() {
            on_event(WatchEvent::Changes(changes));
        }
    });
    Ok(watcher)
}

fn entry_for(root: &Path, path: &Path) -> Option<(Entry, bool)> {
    let meta = std::fs::metadata(path).ok()?;
    let rel = to_rel(root, path)?;
    Some((
        Entry { path: rel, dir: meta.is_dir(), mtime: mtime_ms(&meta), size: meta.len() },
        meta.is_dir(),
    ))
}

/// 存在するパスを「追加」として扱う。フォルダなら中身も探索する
fn present(root: &Path, filter: &Arc<FilterConfig>, path: &Path, out: &mut Vec<Change>) {
    let Some((entry, is_dir)) = entry_for(root, path) else { return };
    if entry.path.is_empty() {
        return;
    }
    out.push(Change::Create(entry));
    if is_dir {
        scan(root, path, filter, || false, |batch| {
            out.extend(batch.into_iter().map(Change::Create));
        });
    }
}

fn removed(root: &Path, path: &Path, out: &mut Vec<Change>) {
    if let Some(rel) = to_rel(root, path) {
        if !rel.is_empty() {
            out.push(Change::Remove { path: rel });
        }
    }
}

fn collect(root: &Path, filter: &Arc<FilterConfig>, ev: &notify::Event, out: &mut Vec<Change>) {
    match ev.kind {
        EventKind::Create(_) => {
            for p in &ev.paths {
                present(root, filter, p, out);
            }
        }
        EventKind::Remove(_) => {
            for p in &ev.paths {
                removed(root, p, out);
            }
        }
        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            RenameMode::From => ev.paths.iter().for_each(|p| removed(root, p, out)),
            RenameMode::To => ev.paths.iter().for_each(|p| present(root, filter, p, out)),
            RenameMode::Both if ev.paths.len() == 2 => {
                removed(root, &ev.paths[0], out);
                present(root, filter, &ev.paths[1], out);
            }
            _ => {
                for p in &ev.paths {
                    if p.exists() {
                        present(root, filter, p, out);
                    } else {
                        removed(root, p, out);
                    }
                }
            }
        },
        EventKind::Modify(_) => {
            for p in &ev.paths {
                if let Some((entry, false)) = entry_for(root, p) {
                    if !entry.path.is_empty() {
                        out.push(Change::Modify(entry));
                    }
                }
            }
        }
        _ => {}
    }
}
