//! ルートディレクトリの再帰探索とファイル操作。データベースは使わず、毎回ファイルシステムを読む。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use base64::Engine;
use ignore::{WalkBuilder, WalkState};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::paths::{self, mtime_ms, normalize_rel, resolve, to_rel, FilterConfig};
use crate::state::AppState;
use crate::{config, links, textio, watcher};

/// これを超えたら探索を止める
const MAX_FILES: usize = 50_000;
const BATCH: usize = 500;

#[derive(Serialize, Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub dir: bool,
    pub mtime: u64,
    pub size: u64,
}

#[derive(Serialize, Clone)]
struct ScanBatch {
    gen: u64,
    entries: Vec<Entry>,
}

#[derive(Serialize, Clone)]
struct ScanDone {
    gen: u64,
    files: usize,
    dirs: usize,
    truncated: bool,
}

pub fn walker(root: &Path, start: &Path, filter: &Arc<FilterConfig>) -> WalkBuilder {
    let mut b = WalkBuilder::new(start);
    b.hidden(true)
        .git_ignore(filter.respect_gitignore)
        .git_exclude(filter.respect_gitignore)
        .git_global(false)
        .ignore(filter.respect_gitignore)
        .parents(true)
        .require_git(false)
        .follow_links(filter.follow_symlinks);
    let f = filter.clone();
    let root = root.to_path_buf();
    b.filter_entry(move |e| {
        if e.depth() == 0 {
            return true;
        }
        let name = e.file_name().to_string_lossy();
        let rel = to_rel(&root, e.path()).unwrap_or_default();
        !f.is_excluded(&name, &rel)
    });
    b
}

/// `start` 以下を並列に探索し、見つかったエントリを `on_batch` に渡す
pub fn scan<F>(root: &Path, start: &Path, filter: &Arc<FilterConfig>, cancel: impl Fn() -> bool + Sync, mut on_batch: F) -> (usize, usize, bool)
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
        let walker = walker(root, start, filter).build_parallel();
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
                    if !is_dir && !filter.is_markdown(e.path()) {
                        return WalkState::Continue;
                    }
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
    (file_count.min(MAX_FILES), dirs, truncated.load(Ordering::Relaxed))
}

fn start_scan(app: &AppHandle, state: &AppState, root: PathBuf) -> u64 {
    let gen = state.scan_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let gen_ref = state.scan_gen.clone();
    let filter = state.filter();
    let app = app.clone();
    std::thread::spawn(move || {
        let cancel = || gen_ref.load(Ordering::Relaxed) != gen;
        let (files, dirs, truncated) = scan(&root, &root, &filter, cancel, |entries| {
            let _ = app.emit("scan:batch", ScanBatch { gen, entries });
        });
        let _ = app.emit("scan:done", ScanDone { gen, files, dirs, truncated });
    });
    gen
}

#[derive(Serialize)]
pub struct OpenResult {
    root: String,
    name: String,
    gen: u64,
}

/// 起動時のコマンドライン引数で指定されたフォルダ
#[tauri::command]
pub fn get_launch_root() -> Option<String> {
    let arg = std::env::args().nth(1)?;
    let p = std::path::absolute(PathBuf::from(arg.trim_matches('"'))).ok()?;
    p.is_dir().then(|| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_root(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<OpenResult, String> {
    let p = std::path::absolute(PathBuf::from(&path)).map_err(|e| e.to_string())?;
    if !p.is_dir() {
        return Err(format!("フォルダが見つかりません: {path}"));
    }
    *state.root.write().unwrap() = Some(p.clone());
    let gen = start_scan(&app, &state, p.clone());

    // 監視を張り直す（古い監視は drop で止まる）
    let w = watcher::start(app.clone(), p.clone(), state.filter()).map_err(|e| e.to_string());
    *state.watcher.lock().unwrap() = w.ok();

    let root_str = p.to_string_lossy().into_owned();
    config::push_recent(&root_str);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root_str.clone());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&format!("{name} - mark-shell"));
    }
    Ok(OpenResult { root: root_str, name, gen })
}

#[tauri::command]
pub fn rescan(app: AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let root = state.root()?;
    Ok(start_scan(&app, &state, root))
}

#[derive(Serialize)]
pub struct ReadResult {
    content: String,
    encoding: String,
    eol: String,
    readonly: bool,
    mtime: u64,
}

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> Result<ReadResult, String> {
    let root = state.root()?;
    let p = resolve(&root, &path)?;
    let meta = fs::metadata(&p).map_err(|e| format!("{path} を開けません: {e}"))?;
    let bytes = fs::read(&p).map_err(|e| format!("{path} を読めません: {e}"))?;
    let d = textio::decode(&bytes);
    Ok(ReadResult {
        content: d.text,
        encoding: d.encoding,
        eol: d.eol.to_string(),
        readonly: meta.permissions().readonly(),
        mtime: mtime_ms(&meta),
    })
}

/// 保存。戻り値は保存後の更新日時
#[tauri::command]
pub fn write_file(state: State<'_, AppState>, path: String, content: String, encoding: String, eol: String) -> Result<u64, String> {
    let root = state.root()?;
    let p = resolve(&root, &path)?;
    if let Ok(meta) = fs::metadata(&p) {
        if meta.permissions().readonly() {
            return Err("読み取り専用のファイルです".into());
        }
    }
    let bytes = textio::encode(&content, &encoding, &eol)?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    textio::write_atomic(&p, &bytes).map_err(|e| format!("{path} を保存できません: {e}"))?;
    Ok(fs::metadata(&p).map(|m| mtime_ms(&m)).unwrap_or(0))
}

#[tauri::command]
pub fn create_file(state: State<'_, AppState>, path: String, content: Option<String>) -> Result<String, String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or("不正なパスです")?;
    let p = resolve(&root, &rel)?;
    if p.exists() {
        return Err(format!("{rel} は既にあります"));
    }
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&p, content.unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(rel)
}

#[tauri::command]
pub fn create_dir(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or("不正なパスです")?;
    let p = resolve(&root, &rel)?;
    if p.exists() {
        return Err(format!("{rel} は既にあります"));
    }
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// 名前変更・移動で書き換わるリンクの数
#[tauri::command]
pub async fn count_link_updates(app: AppHandle, from: String, to: String) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let root = state.root()?;
    let filter = state.filter();
    tauri::async_runtime::spawn_blocking(move || links::plan(&root, &filter, &from, &to, false).map(|r| r.links))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct RenameResult {
    path: String,
    links: usize,
    files: Vec<String>,
}

#[tauri::command]
pub async fn rename_path(app: AppHandle, from: String, to: String, rewrite_links: bool) -> Result<RenameResult, String> {
    let state = app.state::<AppState>();
    let root = state.root()?;
    let filter = state.filter();
    let from = normalize_rel(&from).ok_or("不正なパスです")?;
    let to = normalize_rel(&to).ok_or("不正なパスです")?;
    if to.is_empty() || from.is_empty() {
        return Err("不正なパスです".into());
    }
    if to.to_lowercase().starts_with(&(from.to_lowercase() + "/")) {
        return Err("フォルダをその中に移動することはできません".into());
    }
    let src = resolve(&root, &from)?;
    let dst = resolve(&root, &to)?;
    let case_only = from.eq_ignore_ascii_case(&to);
    if dst.exists() && !case_only {
        return Err(format!("{to} は既にあります"));
    }
    if let Some(dir) = dst.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::rename(&src, &dst).map_err(|e| format!("移動できません: {e}"))?;

    let (links, files) = if rewrite_links {
        let (f, t) = (from.clone(), to.clone());
        let r = tauri::async_runtime::spawn_blocking(move || links::plan(&root, &filter, &f, &t, true))
            .await
            .map_err(|e| e.to_string())??;
        (r.links, r.files)
    } else {
        (0, vec![])
    };
    Ok(RenameResult { path: to, links, files })
}

#[tauri::command]
pub fn delete_path(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or("不正なパスです")?;
    if rel.is_empty() {
        return Err("ルートディレクトリは削除できません".into());
    }
    let p = resolve(&root, &rel)?;
    trash::delete(&p).map_err(|e| format!("ごみ箱へ移動できません: {e}"))
}

/// 空いている名前を探す: `name (2).md` のように番号を付ける
fn unique_path(root: &Path, rel: &str, suffix: &str) -> Result<String, String> {
    let dir = paths::parent_rel(rel);
    let name = paths::file_name(rel);
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..10_000 {
        let candidate = if n == 1 && suffix.is_empty() {
            name.to_string()
        } else if n == 1 {
            format!("{stem}{suffix}{ext}")
        } else {
            format!("{stem}{suffix} ({n}){ext}")
        };
        let c = paths::join_rel(dir, &candidate);
        if !resolve(root, &c)?.exists() {
            return Ok(c);
        }
    }
    Err("空いているファイル名が見つかりません".into())
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for e in fs::read_dir(src)? {
            let e = e?;
            copy_recursive(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

#[tauri::command]
pub fn duplicate_path(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or("不正なパスです")?;
    let target = unique_path(&root, &rel, " のコピー")?;
    copy_recursive(&resolve(&root, &rel)?, &resolve(&root, &target)?).map_err(|e| e.to_string())?;
    Ok(target)
}

/// 画像の保存先: `<ノートのフォルダ>/<folder>/<ノート名>-<連番>.<ext>`
fn next_asset_path(root: &Path, note: &str, folder: &str, ext: &str) -> Result<String, String> {
    let dir = paths::join_rel(paths::parent_rel(note), folder.trim_matches('/'));
    let stem = paths::file_stem(note);
    for n in 1..100_000 {
        let rel = paths::join_rel(&dir, &format!("{stem}-{n}.{ext}"));
        if !resolve(root, &rel)?.exists() {
            return Ok(rel);
        }
    }
    Err("空いているファイル名が見つかりません".into())
}

fn sanitize_ext(ext: &str) -> String {
    let e: String = ext.trim_start_matches('.').chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if e.is_empty() { "png".into() } else { e.to_lowercase() }
}

/// 貼り付けた画像を保存する。戻り値はルートからの相対パス
#[tauri::command]
pub fn save_asset(state: State<'_, AppState>, note: String, data: String, ext: String, folder: String) -> Result<String, String> {
    let root = state.root()?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| e.to_string())?;
    let rel = next_asset_path(&root, &note, &folder, &sanitize_ext(&ext))?;
    let p = resolve(&root, &rel)?;
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&p, bytes).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// エクスプローラーからドロップした画像をコピーする
#[tauri::command]
pub fn copy_asset(state: State<'_, AppState>, note: String, source: String, folder: String) -> Result<String, String> {
    let root = state.root()?;
    let src = PathBuf::from(&source);
    // ルート内のファイルならコピーせずにそのまま参照する
    if let Some(rel) = to_rel(&root, &src) {
        if src.starts_with(&root) {
            return Ok(rel);
        }
    }
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let rel = next_asset_path(&root, &note, &folder, &sanitize_ext(ext))?;
    let p = resolve(&root, &rel)?;
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(&src, &p).map_err(|e| e.to_string())?;
    Ok(rel)
}

#[tauri::command]
pub fn path_exists(state: State<'_, AppState>, path: String) -> Result<bool, String> {
    let root = state.root()?;
    Ok(resolve(&root, &path)?.exists())
}

/// ルートからの相対パスを絶対パスにする
#[tauri::command]
pub fn absolute_path(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = state.root()?;
    Ok(resolve(&root, &path)?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn reveal_in_explorer(state: State<'_, AppState>, path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let root = state.root()?;
    let p = resolve(&root, &path)?;
    let mut cmd = std::process::Command::new("explorer.exe");
    if path.is_empty() {
        cmd.arg(&p);
    } else {
        // explorer.exe は /select,"パス" を 1 つの引数として受け取る
        cmd.raw_arg(format!("/select,\"{}\"", p.display()));
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// 別のフォルダを新しいウィンドウ（別プロセス）で開く
#[tauri::command]
pub fn open_new_window(path: Option<String>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    if let Some(p) = path {
        cmd.arg(p);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}
