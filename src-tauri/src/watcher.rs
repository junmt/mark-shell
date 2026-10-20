//! ルートディレクトリ以下の変更を監視し、UI に差分を送る

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::paths::{mtime_ms, to_rel, FilterConfig};
use crate::workspace::{scan, Entry};

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Change {
    Create(Entry),
    Modify(Entry),
    Remove { path: String },
}

#[derive(Serialize, Clone)]
struct ChangeBatch {
    changes: Vec<Change>,
}

/// 変更をまとめて送るまでの待ち時間
const COALESCE: Duration = Duration::from_millis(120);

pub fn start(app: AppHandle, root: PathBuf, filter: Arc<FilterConfig>) -> notify::Result<notify::RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        let gitignore = load_gitignore(&root, &filter);
        loop {
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
                    Ok(ev) => {
                        if ev.need_rescan() {
                            eprintln!("[watcher] 再走査が必要: {ev:?}");
                            rescan = true;
                            continue;
                        }
                        collect(&root, &filter, &gitignore, &ev, &mut changes);
                    }
                    Err(e) => {
                        eprintln!("[watcher] エラー: {e:?}");
                        rescan = true;
                    }
                }
            }
            if rescan {
                let _ = app.emit("fs:rescan", ());
            }
            if !changes.is_empty() {
                let _ = app.emit("fs:change", ChangeBatch { changes });
            }
        }
    });
    Ok(watcher)
}

fn load_gitignore(root: &Path, filter: &FilterConfig) -> Option<Gitignore> {
    if !filter.respect_gitignore {
        return None;
    }
    let mut b = GitignoreBuilder::new(root);
    b.add(root.join(".gitignore"));
    b.build().ok()
}

fn ignored(root: &Path, filter: &FilterConfig, gi: &Option<Gitignore>, path: &Path, rel: &str, is_dir: bool) -> bool {
    if rel.is_empty() || filter.is_rel_excluded(rel) {
        return true;
    }
    match gi {
        Some(gi) if path.starts_with(root) => gi.matched_path_or_any_parents(path, is_dir).is_ignore(),
        _ => false,
    }
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
fn present(root: &Path, filter: &Arc<FilterConfig>, gi: &Option<Gitignore>, path: &Path, out: &mut Vec<Change>) {
    let Some((entry, is_dir)) = entry_for(root, path) else { return };
    if ignored(root, filter, gi, path, &entry.path, is_dir) {
        return;
    }
    if is_dir {
        out.push(Change::Create(entry));
        scan(root, path, filter, || false, |batch| {
            out.extend(batch.into_iter().map(Change::Create));
        });
    } else if filter.is_listed(path) {
        out.push(Change::Create(entry));
    }
}

fn removed(root: &Path, filter: &FilterConfig, path: &Path, out: &mut Vec<Change>) {
    if let Some(rel) = to_rel(root, path) {
        if !rel.is_empty() && !filter.is_rel_excluded(&rel) {
            out.push(Change::Remove { path: rel });
        }
    }
}

fn collect(root: &Path, filter: &Arc<FilterConfig>, gi: &Option<Gitignore>, ev: &notify::Event, out: &mut Vec<Change>) {
    match ev.kind {
        EventKind::Create(_) => {
            for p in &ev.paths {
                present(root, filter, gi, p, out);
            }
        }
        EventKind::Remove(_) => {
            for p in &ev.paths {
                removed(root, filter, p, out);
            }
        }
        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            RenameMode::From => ev.paths.iter().for_each(|p| removed(root, filter, p, out)),
            RenameMode::To => ev.paths.iter().for_each(|p| present(root, filter, gi, p, out)),
            RenameMode::Both if ev.paths.len() == 2 => {
                removed(root, filter, &ev.paths[0], out);
                present(root, filter, gi, &ev.paths[1], out);
            }
            _ => {
                for p in &ev.paths {
                    if p.exists() {
                        present(root, filter, gi, p, out);
                    } else {
                        removed(root, filter, p, out);
                    }
                }
            }
        },
        EventKind::Modify(_) => {
            for p in &ev.paths {
                if !filter.is_listed(p) {
                    continue;
                }
                if let Some((entry, false)) = entry_for(root, p) {
                    if !ignored(root, filter, gi, p, &entry.path, false) {
                        out.push(Change::Modify(entry));
                    }
                }
            }
        }
        _ => {}
    }
}
