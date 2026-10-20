//! フォルダの再帰探索。隠しファイルや .gitignore の対象も含め、すべてたどる

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use ignore::{WalkBuilder, WalkState};
use serde::{Deserialize, Serialize};

use crate::paths::{mtime_ms, to_rel, FilterConfig};

/// これを超えたら探索を止める
pub const MAX_FILES: usize = 50_000;
const BATCH: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
    pub mtime: u64,
    pub size: u64,
}

/// 探索の結果（ファイル数・フォルダ数・上限で止めたか）
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ScanSummary {
    pub files: usize,
    pub dirs: usize,
    pub truncated: bool,
}

/// 隠しファイル、.gitignore の対象、.git や node_modules も含めてすべてたどる
pub fn walker(start: &Path, filter: &FilterConfig) -> WalkBuilder {
    let mut b = WalkBuilder::new(start);
    b.standard_filters(false).follow_links(filter.follow_symlinks);
    b
}

/// `start` 以下を並列に探索し、見つかったエントリを `on_batch` に渡す
pub fn scan<F>(root: &Path, start: &Path, filter: &Arc<FilterConfig>, cancel: impl Fn() -> bool + Sync, mut on_batch: F) -> ScanSummary
where
    F: FnMut(Vec<Entry>),
{
    let (tx, rx) = mpsc::channel::<Entry>();
    let stop = Arc::new(AtomicBool::new(false));
    let files = AtomicU64::new(0);
    let truncated = AtomicBool::new(false);
    let mut dirs = 0usize;
    let mut file_count = 0usize;

    std::thread::scope(|s| {
        let walker = walker(start, filter).build_parallel();
        let stop2 = stop.clone();
        let files = &files;
        let truncated = &truncated;
        let cancel = &cancel;
        s.spawn(move || {
            walker.run(|| {
                let tx = tx.clone();
                let stop = stop2.clone();
                Box::new(move |res| {
                    if stop.load(Ordering::Relaxed) || cancel() {
                        return WalkState::Quit;
                    }
                    let Ok(e) = res else { return WalkState::Continue };
                    if e.depth() == 0 {
                        return WalkState::Continue;
                    }
                    let Some(ft) = e.file_type() else { return WalkState::Continue };
                    let is_dir = ft.is_dir();
                    if !is_dir && files.fetch_add(1, Ordering::Relaxed) as usize >= MAX_FILES {
                        truncated.store(true, Ordering::Relaxed);
                        stop.store(true, Ordering::Relaxed);
                        return WalkState::Quit;
                    }
                    let Some(rel) = to_rel(root, e.path()) else { return WalkState::Continue };
                    let meta = e.metadata().ok();
                    let entry = Entry {
                        path: rel,
                        dir: is_dir,
                        mtime: meta.as_ref().map(mtime_ms).unwrap_or(0),
                        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    };
                    if tx.send(entry).is_err() {
                        return WalkState::Quit;
                    }
                    WalkState::Continue
                })
            });
        });

        let mut buf = Vec::with_capacity(BATCH);
        for e in rx {
            if e.dir {
                dirs += 1;
            } else {
                file_count += 1;
            }
            buf.push(e);
            if buf.len() >= BATCH {
                on_batch(std::mem::take(&mut buf));
            }
        }
        if !buf.is_empty() {
            on_batch(buf);
        }
    });
    ScanSummary { files: file_count.min(MAX_FILES), dirs, truncated: truncated.load(Ordering::Relaxed) }
}
