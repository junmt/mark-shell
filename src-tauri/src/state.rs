use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};

use markshell_core::watch::Watcher;

use crate::paths::FilterConfig;
use crate::pty::PtySession;
use crate::wsl::WslRoot;

pub struct AppState {
    /// 開いているルートディレクトリ（ウィンドウ 1 つにつき 1 つ）。WSL なら \wsl.localhost\... のパス
    pub root: RwLock<Option<PathBuf>>,
    /// WSL のフォルダを開いているとき。探索・検索・監視などは WSL のエージェントが行う
    pub wsl: RwLock<Option<Arc<WslRoot>>>,
    pub filter: RwLock<Arc<FilterConfig>>,
    pub watcher: Mutex<Option<Watcher>>,
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
            wsl: RwLock::new(None),
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

    pub fn wsl(&self) -> Option<Arc<WslRoot>> {
        self.wsl.read().unwrap().clone()
    }

    pub fn filter(&self) -> Arc<FilterConfig> {
        self.filter.read().unwrap().clone()
    }
}
