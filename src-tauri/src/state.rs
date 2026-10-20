use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use crate::paths::FilterConfig;
use crate::pty::PtySession;

pub struct AppState {
    /// 開いているルートディレクトリ（ウィンドウ 1 つにつき 1 つ）
    pub root: RwLock<Option<PathBuf>>,
    pub filter: RwLock<Arc<FilterConfig>>,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// 走査の世代番号。新しい走査を始めると古い走査は止まる
    pub scan_gen: Arc<AtomicU64>,
    pub search_gen: Arc<AtomicU64>,
    pub ptys: Mutex<HashMap<u32, PtySession>>,
    pub next_pty: AtomicU32,
}

impl AppState {
    pub fn new() -> Self {
        let filter = FilterConfig::from_config(&crate::config::read_config());
        Self {
            root: RwLock::new(None),
            filter: RwLock::new(Arc::new(filter)),
            watcher: Mutex::new(None),
            scan_gen: Arc::new(AtomicU64::new(0)),
            search_gen: Arc::new(AtomicU64::new(0)),
            ptys: Mutex::new(HashMap::new()),
            next_pty: AtomicU32::new(1),
        }
    }

    pub fn root(&self) -> Result<PathBuf, String> {
        self.root
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| "フォルダが開かれていません".to_string())
    }

    pub fn filter(&self) -> Arc<FilterConfig> {
        self.filter.read().unwrap().clone()
    }
}
