//! Linux（WSL のエージェント）での変更の監視。inotify を直接使う。
//!
//! notify 8 は inotify の監視に OPEN を含める。すると大きなフォルダを探索しただけで、フォルダを開いたイベントが
//! カーネルのキュー（既定で 16,384 件）から溢れ、「取りこぼしたので探索し直す」が出る。その探索でまた溢れるので、
//! 探索のやり直しが止まらなくなる。ここでは変更に関わるイベントだけを監視する。
//!
//! inotify はフォルダ単位なので、ルート以下のすべてのフォルダに監視を付け、新しくできたフォルダにも付け足す。

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use inotify::{EventMask, Inotify, WatchDescriptor, WatchMask};

use super::{modified, present, removed, Change, WatchEvent, COALESCE};
use crate::paths::FilterConfig;
use crate::scan::walker;

/// イベントがないときに読み直すまでの間隔
const POLL: Duration = Duration::from_millis(30);
/// 変更が続いていても、この時間が経ったら一度送る
const MAX_DELAY: Duration = Duration::from_secs(1);
/// ENOSPC（監視の数の上限 fs.inotify.max_user_watches に達した）
const ENOSPC: i32 = 28;

/// 監視のハンドル。drop すると監視が止まる
pub struct Watcher {
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// 監視を始める。戻り値を drop すると止まる
pub fn start(root: PathBuf, filter: Arc<FilterConfig>, on_event: impl Fn(WatchEvent) + Send + 'static) -> io::Result<Watcher> {
    let mut tree = Tree::new(root.clone(), filter)?;
    tree.add_tree(&root)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    std::thread::spawn(move || tree.run(&stop2, on_event));
    Ok(Watcher { stop })
}

struct Tree {
    inotify: Inotify,
    mask: WatchMask,
    root: PathBuf,
    filter: Arc<FilterConfig>,
    /// 監視の番号 → (監視, フォルダ)
    dirs: HashMap<i32, (WatchDescriptor, PathBuf)>,
    /// フォルダ → 監視の番号
    by_path: HashMap<PathBuf, i32>,
}

impl Tree {
    fn new(root: PathBuf, filter: Arc<FilterConfig>) -> io::Result<Self> {
        let mut mask = WatchMask::CREATE
            | WatchMask::DELETE
            | WatchMask::MODIFY
            | WatchMask::CLOSE_WRITE
            | WatchMask::ATTRIB
            | WatchMask::MOVED_FROM
            | WatchMask::MOVED_TO
            | WatchMask::ONLYDIR;
        if !filter.follow_symlinks {
            mask |= WatchMask::DONT_FOLLOW;
        }
        Ok(Self { inotify: Inotify::init()?, mask, root, filter, dirs: HashMap::new(), by_path: HashMap::new() })
    }

    /// dir とその下のすべてのフォルダに監視を付ける。上限に達したときだけ Err（読めないフォルダなどは飛ばす）
    fn add_tree(&mut self, dir: &Path) -> io::Result<()> {
        for e in walker(dir, &self.filter).build().flatten() {
            if !e.file_type().is_some_and(|t| t.is_dir()) {
                continue;
            }
            match self.inotify.watches().add(e.path(), self.mask) {
                Ok(wd) => {
                    let id = wd.get_watch_descriptor_id();
                    self.by_path.insert(e.path().to_path_buf(), id);
                    self.dirs.insert(id, (wd, e.path().to_path_buf()));
                }
                Err(err) if err.raw_os_error() == Some(ENOSPC) => return Err(err),
                Err(_) => {}
            }
        }
        Ok(())
    }

    /// dir とその下のフォルダの監視を外す（フォルダが監視の外へ移動したとき）
    fn remove_tree(&mut self, dir: &Path) {
        let gone: Vec<PathBuf> = self.by_path.keys().filter(|p| p.starts_with(dir)).cloned().collect();
        for p in gone {
            if let Some(id) = self.by_path.remove(&p) {
                if let Some((wd, _)) = self.dirs.remove(&id) {
                    let _ = self.inotify.watches().remove(wd);
                }
            }
        }
    }

    /// 監視が外れた（フォルダが消えたなど）
    fn forget(&mut self, id: i32) {
        if let Some((_, path)) = self.dirs.remove(&id) {
            if self.by_path.get(&path) == Some(&id) {
                self.by_path.remove(&path);
            }
        }
    }

    fn run(mut self, stop: &AtomicBool, on_event: impl Fn(WatchEvent)) {
        let mut buf = vec![0u8; 64 * 1024];
        let mut batch = Batch::default();
        // まとめている変更の、最初と最後のイベントの時刻
        let mut span: Option<(Instant, Instant)> = None;
        while !stop.load(Ordering::Relaxed) {
            let events: Vec<_> = match self.inotify.read_events(&mut buf) {
                Ok(events) => events.map(|e| e.to_owned()).collect(),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Vec::new(),
                Err(_) => return,
            };
            if events.is_empty() {
                if let Some((first, last)) = span {
                    if last.elapsed() >= COALESCE || first.elapsed() >= MAX_DELAY {
                        batch.flush(&on_event);
                        span = None;
                    }
                }
                std::thread::sleep(POLL);
                continue;
            }
            for e in events {
                self.handle(e.wd.get_watch_descriptor_id(), e.mask, e.name.as_deref().map(Path::new), &mut batch);
            }
            let now = Instant::now();
            span = Some((span.map_or(now, |(first, _)| first), now));
            if span.is_some_and(|(first, _)| first.elapsed() >= MAX_DELAY) {
                batch.flush(&on_event);
                span = None;
            }
        }
    }

    fn handle(&mut self, id: i32, mask: EventMask, name: Option<&Path>, batch: &mut Batch) {
        if mask.contains(EventMask::Q_OVERFLOW) {
            batch.rescan = true;
            return;
        }
        if mask.contains(EventMask::IGNORED) {
            self.forget(id);
            return;
        }
        // name がないのは監視しているフォルダ自身のイベント。中身の変化は親フォルダのイベントで分かる
        let (Some(name), Some((_, dir))) = (name, self.dirs.get(&id)) else { return };
        let path = dir.join(name);
        let is_dir = mask.contains(EventMask::ISDIR);
        if mask.intersects(EventMask::CREATE | EventMask::MOVED_TO) {
            // 先に監視を付けてから中身を読む（その間にできたものを取りこぼさない）
            if is_dir {
                if let Err(e) = self.add_tree(&path) {
                    eprintln!("[watch] {}: {e}", path.display());
                }
            }
            present(&self.root, &self.filter, &path, &mut batch.changes);
        } else if mask.intersects(EventMask::DELETE | EventMask::MOVED_FROM) {
            // 消えたフォルダの監視はカーネルが外す（IGNORED が届く）。移動したフォルダの監視は残るので外す
            if is_dir && mask.contains(EventMask::MOVED_FROM) {
                self.remove_tree(&path);
            }
            removed(&self.root, &path, &mut batch.changes);
        } else if !is_dir && mask.intersects(EventMask::MODIFY | EventMask::CLOSE_WRITE | EventMask::ATTRIB) {
            // 書き込みのたびに MODIFY が来るので、同じファイルは 1 回にまとめる
            if batch.modified.insert(path.clone()) {
                if let Some(c) = modified(&self.root, &path) {
                    batch.changes.push(c);
                }
            }
        }
    }
}

#[derive(Default)]
struct Batch {
    changes: Vec<Change>,
    modified: HashSet<PathBuf>,
    rescan: bool,
}

impl Batch {
    fn flush(&mut self, on_event: &impl Fn(WatchEvent)) {
        if std::mem::take(&mut self.rescan) {
            on_event(WatchEvent::Rescan);
        }
        self.modified.clear();
        let changes = std::mem::take(&mut self.changes);
        if !changes.is_empty() {
            on_event(WatchEvent::Changes(changes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;

    /// 監視を始め、届いた変更を (種類, パス) にして返す受け口
    fn watch(root: &Path) -> (Watcher, mpsc::Receiver<Vec<(String, String)>>) {
        let (tx, rx) = mpsc::channel();
        let filter = Arc::new(FilterConfig::from_config(&serde_json::Value::Null));
        let w = start(root.to_path_buf(), filter, move |ev| {
            let v = match ev {
                WatchEvent::Rescan => vec![("rescan".to_string(), String::new())],
                WatchEvent::Changes(cs) => cs
                    .into_iter()
                    .map(|c| match c {
                        Change::Create(e) => ("create".into(), e.path),
                        Change::Modify(e) => ("modify".into(), e.path),
                        Change::Remove { path } => ("remove".into(), path),
                    })
                    .collect(),
            };
            let _ = tx.send(v);
        })
        .unwrap();
        (w, rx)
    }

    /// 少し待って、届いた変更をすべて集める
    fn drain(rx: &mpsc::Receiver<Vec<(String, String)>>) -> Vec<(String, String)> {
        let mut out = Vec::new();
        while let Ok(v) = rx.recv_timeout(Duration::from_millis(800)) {
            out.extend(v);
        }
        out
    }

    fn has(got: &[(String, String)], kind: &str, path: &str) -> bool {
        got.iter().any(|(k, p)| k == kind && p == path)
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("markshell-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn reading_many_dirs_does_not_overflow() {
        // キューの上限（16,384）を超える数のフォルダを読んでも、探索し直しにならない
        let root = temp_root("read");
        for i in 0..200 {
            for j in 0..100 {
                fs::create_dir_all(root.join(format!("d{i}/e{j}"))).unwrap();
            }
        }
        let (_w, rx) = watch(&root);
        for _ in 0..2 {
            for e in walker(&root, &FilterConfig::from_config(&serde_json::Value::Null)).build().flatten() {
                if e.file_type().is_some_and(|t| t.is_dir()) {
                    let _ = fs::read_dir(e.path()).map(|d| d.count());
                }
            }
        }
        let got = drain(&rx);
        assert!(got.is_empty(), "{got:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_changes_in_new_and_moved_dirs() {
        let root = temp_root("changes");
        fs::create_dir_all(root.join("a")).unwrap();
        let (_w, rx) = watch(&root);

        fs::write(root.join("a/x.md"), "1").unwrap();
        fs::create_dir_all(root.join("b/c")).unwrap();
        fs::write(root.join("b/c/y.md"), "1").unwrap();
        let got = drain(&rx);
        assert!(has(&got, "create", "a/x.md"), "{got:?}");
        assert!(has(&got, "create", "b/c"), "{got:?}");
        assert!(has(&got, "create", "b/c/y.md"), "{got:?}");

        // 新しくできたフォルダの中の変更も届く
        fs::write(root.join("b/c/y.md"), "22").unwrap();
        let got = drain(&rx);
        assert!(has(&got, "modify", "b/c/y.md"), "{got:?}");
        assert_eq!(got.iter().filter(|(k, _)| k == "modify").count(), 1, "{got:?}");

        // 名前を変えたフォルダは、新しい名前で監視される
        fs::rename(root.join("b"), root.join("b2")).unwrap();
        let got = drain(&rx);
        assert!(has(&got, "remove", "b"), "{got:?}");
        assert!(has(&got, "create", "b2/c/y.md"), "{got:?}");
        fs::write(root.join("b2/c/z.md"), "1").unwrap();
        let got = drain(&rx);
        assert!(has(&got, "create", "b2/c/z.md"), "{got:?}");

        fs::remove_dir_all(root.join("b2")).unwrap();
        fs::remove_file(root.join("a/x.md")).unwrap();
        let got = drain(&rx);
        assert!(has(&got, "remove", "b2"), "{got:?}");
        assert!(has(&got, "remove", "a/x.md"), "{got:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stops_when_dropped() {
        let root = temp_root("drop");
        let (w, rx) = watch(&root);
        drop(w);
        std::thread::sleep(POLL * 4);
        fs::write(root.join("x.md"), "1").unwrap();
        assert!(drain(&rx).is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
