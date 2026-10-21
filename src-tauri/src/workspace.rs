//! ルートディレクトリの探索とファイル操作。データベースは使わず、毎回ファイルシステムを読む。
//! WSL のフォルダでは、全体をたどる処理（探索・リンクの書き換え）とごみ箱への移動を WSL のエージェントに任せる

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use base64::Engine;
use markshell_core::links::Plan;
use markshell_core::scan::{self, Entry};
use markshell_core::tr;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::paths::{self, mtime_ms, normalize_rel, resolve, to_rel, FilterConfig};
use crate::state::AppState;
use crate::wsl::{self, WslInfo, WslRoot};
use crate::{config, links, textio, watcher};

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

/// 探索を始める。結果は scan:batch / scan:done で届く
fn start_scan(app: &AppHandle, state: &AppState, root: PathBuf) -> Result<u64, String> {
    let gen = state.scan_gen.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(wsl) = state.wsl() {
        wsl.agent.request("scan", json!({ "gen": gen }))?;
        return Ok(gen);
    }
    let gen_ref = state.scan_gen.clone();
    let filter = state.filter();
    let app = app.clone();
    std::thread::spawn(move || {
        let cancel = || gen_ref.load(Ordering::Relaxed) != gen;
        let s = scan::scan(&root, &root, &filter, cancel, |entries| {
            let _ = app.emit("scan:batch", ScanBatch { gen, entries });
        });
        let _ = app.emit("scan:done", ScanDone { gen, files: s.files, dirs: s.dirs, truncated: s.truncated });
    });
    Ok(gen)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    root: String,
    name: String,
    gen: u64,
    /// WSL のフォルダなら、そのディストリビューションと Linux 側のパス
    wsl: Option<WslInfo>,
    /// WSL のフォルダだがエージェントを使えなかったときの理由（\wsl.localhost 経由だけで扱う）
    wsl_error: Option<String>,
}

/// 起動時のコマンドライン引数で指定されたフォルダかファイル（エクスプローラーの右クリックメニューから）
#[tauri::command]
pub fn get_launch_target() -> Option<LaunchTarget> {
    let arg = std::env::args().nth(1)?;
    let p = std::path::absolute(PathBuf::from(arg.trim_matches('"'))).ok()?;
    if p.is_dir() {
        return Some(LaunchTarget { root: p.to_string_lossy().into_owned(), file: None });
    }
    if !p.is_file() {
        return None;
    }
    // ファイルなら、それを含む最近開いたフォルダ（新しい順）を開く。なければファイルのあるフォルダ
    let root = config::read_recent()
        .into_iter()
        .map(PathBuf::from)
        .find(|r| p.starts_with(r) && r.is_dir())
        .or_else(|| p.parent().map(Path::to_path_buf))?;
    let file = to_rel(&root, &p)?;
    Some(LaunchTarget { root: root.to_string_lossy().into_owned(), file: Some(file) })
}

/// 起動時に開くフォルダと、ファイルを指定されたときはそのファイル（ルートからの相対パス）
#[derive(Serialize)]
pub struct LaunchTarget {
    root: String,
    file: Option<String>,
}

#[tauri::command]
pub async fn open_root(app: AppHandle, path: String) -> Result<OpenResult, String> {
    // WSL のエージェントの起動には数秒かかることがあるので、画面を止めないよう別スレッドで行う
    tauri::async_runtime::spawn_blocking(move || open_root_blocking(&app, &path))
        .await
        .map_err(|e| e.to_string())?
}

fn open_root_blocking(app: &AppHandle, path: &str) -> Result<OpenResult, String> {
    let state = app.state::<AppState>();
    let p = std::path::absolute(PathBuf::from(path)).map_err(|e| e.to_string())?;
    if !p.is_dir() {
        return Err(tr!("フォルダが見つかりません: {path}", "Folder not found: {path}", "找不到文件夹：{path}"));
    }
    // WSL のフォルダなら、探索・検索・監視を WSL のエージェントに任せる
    let (wsl_root, wsl_error) = match wsl::parse_unc(&p.to_string_lossy()) {
        Some((distro, linux)) => match WslRoot::open(app, &distro, &linux, &config::read_config()) {
            Ok(w) => (Some(w), None),
            Err(e) => (None, Some(e)),
        },
        None => (None, None),
    };
    *state.wsl.write().unwrap() = wsl_root.clone();
    *state.root.write().unwrap() = Some(p.clone());

    // 監視を張り直す（古い監視は drop で止まる）。WSL ではエージェントが監視する
    *state.watcher.lock().unwrap() = None;
    if wsl_root.is_none() {
        *state.watcher.lock().unwrap() = watcher::start(app.clone(), p.clone(), state.filter()).ok();
    }
    let gen = start_scan(app, &state, p.clone())?;

    let root_str = p.to_string_lossy().into_owned();
    config::push_recent(&root_str);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .or_else(|| wsl_root.as_ref().map(|w| w.linux_root.rsplit('/').next().unwrap_or("/").to_string()))
        .unwrap_or_else(|| root_str.clone());
    if let Some(w) = app.get_webview_window("main") {
        let title = match &wsl_root {
            Some(w) => format!("{name} [WSL: {}] - mark-shell", w.distro),
            None => format!("{name} - mark-shell"),
        };
        let _ = w.set_title(&title);
    }
    Ok(OpenResult { root: root_str, name, gen, wsl: wsl_root.map(|w| w.info()), wsl_error })
}

#[tauri::command]
pub fn rescan(app: AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let root = state.root()?;
    start_scan(&app, &state, root)
}

#[derive(Serialize)]
pub struct ReadResult {
    content: String,
    encoding: String,
    eol: String,
    readonly: bool,
    mtime: u64,
}

/// エディタで開けるファイルの大きさの上限。大きなログでも画面が固まらない程度にする
const MAX_OPEN_BYTES: u64 = 50 * 1024 * 1024;
/// エディタで開けないファイル（バイナリ・大きすぎる）のエラーに付ける印（画面側の api.ts と同じ）
const NOT_TEXT_PREFIX: &str = "not-text:";

#[tauri::command]
pub fn read_file(state: State<'_, AppState>, path: String) -> Result<ReadResult, String> {
    let root = state.root()?;
    let p = resolve(&root, &path)?;
    let meta = fs::metadata(&p).map_err(|e| tr!("{path} を開けません: {e}", "Cannot open {path}: {e}", "无法打开 {path}：{e}"))?;
    // 先頭の NOT_TEXT_PREFIX を見て、画面側は既定のアプリで開くか尋ねる
    if meta.len() > MAX_OPEN_BYTES {
        let (size, max) = (meta.len() / 1024 / 1024, MAX_OPEN_BYTES / 1024 / 1024);
        let msg = tr!(
            "{path} は大きすぎるためエディタでは開けません（{size} MB。開けるのは {max} MB まで）",
            "{path} is too large to open in the editor ({size} MB; the limit is {max} MB)",
            "{path} 太大，无法在编辑器中打开（{size} MB，上限为 {max} MB）"
        );
        return Err(format!("{NOT_TEXT_PREFIX}{msg}"));
    }
    let bytes = fs::read(&p).map_err(|e| tr!("{path} を読めません: {e}", "Cannot read {path}: {e}", "无法读取 {path}：{e}"))?;
    if textio::looks_binary(&bytes) {
        let msg = tr!(
            "{path} はテキストファイルではないため、エディタでは開けません",
            "{path} is not a text file, so it cannot be opened in the editor",
            "{path} 不是文本文件，无法在编辑器中打开"
        );
        return Err(format!("{NOT_TEXT_PREFIX}{msg}"));
    }
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
            return Err(tr!("読み取り専用のファイルです", "The file is read-only", "该文件为只读文件"));
        }
    }
    let bytes = textio::encode(&content, &encoding, &eol)?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    textio::write_atomic(&p, &bytes).map_err(|e| tr!("{path} を保存できません: {e}", "Cannot save {path}: {e}", "无法保存 {path}：{e}"))?;
    Ok(fs::metadata(&p).map(|m| mtime_ms(&m)).unwrap_or(0))
}

#[tauri::command]
pub fn create_file(state: State<'_, AppState>, path: String, content: Option<String>) -> Result<String, String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or_else(paths::invalid_path)?;
    let p = resolve(&root, &rel)?;
    if p.exists() {
        return Err(tr!("{rel} は既にあります", "{rel} already exists", "{rel} 已存在"));
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
    let rel = normalize_rel(&path).ok_or_else(paths::invalid_path)?;
    let p = resolve(&root, &rel)?;
    if p.exists() {
        return Err(tr!("{rel} は既にあります", "{rel} already exists", "{rel} 已存在"));
    }
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(rel)
}

/// リンクの書き換え（apply が false なら数えるだけ）。WSL のフォルダではエージェントが行う
async fn link_plan(app: &AppHandle, from: String, to: String, apply: bool) -> Result<Plan, String> {
    let state = app.state::<AppState>();
    let root = state.root()?;
    let filter: Arc<FilterConfig> = state.filter();
    let wsl = state.wsl();
    tauri::async_runtime::spawn_blocking(move || match wsl {
        Some(wsl) => {
            let r = wsl.agent.request("linkPlan", json!({ "from": from, "to": to, "apply": apply }))?;
            serde_json::from_value(r).map_err(|e| e.to_string())
        }
        None => links::plan(&root, &filter, &from, &to, apply),
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 名前変更・移動で書き換わるリンクの数
#[tauri::command]
pub async fn count_link_updates(app: AppHandle, from: String, to: String) -> Result<usize, String> {
    link_plan(&app, from, to, false).await.map(|r| r.links)
}

#[derive(Serialize)]
pub struct RenameResult {
    path: String,
    links: usize,
    files: Vec<String>,
}

#[tauri::command]
pub async fn rename_path(app: AppHandle, from: String, to: String, rewrite_links: bool) -> Result<RenameResult, String> {
    let root = app.state::<AppState>().root()?;
    let from = normalize_rel(&from).ok_or_else(paths::invalid_path)?;
    let to = normalize_rel(&to).ok_or_else(paths::invalid_path)?;
    if to.is_empty() || from.is_empty() {
        return Err(paths::invalid_path());
    }
    if to.to_lowercase().starts_with(&(from.to_lowercase() + "/")) {
        return Err(tr!(
            "フォルダをその中に移動することはできません",
            "A folder cannot be moved into itself",
            "无法将文件夹移动到其自身内部"
        ));
    }
    let src = resolve(&root, &from)?;
    let dst = resolve(&root, &to)?;
    let case_only = from.eq_ignore_ascii_case(&to);
    if dst.exists() && !case_only {
        return Err(tr!("{to} は既にあります", "{to} already exists", "{to} 已存在"));
    }
    if let Some(dir) = dst.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::rename(&src, &dst).map_err(|e| tr!("移動できません: {e}", "Cannot move: {e}", "无法移动：{e}"))?;

    let (links, files) = if rewrite_links {
        let r = link_plan(&app, from.clone(), to.clone(), true).await?;
        (r.links, r.files)
    } else {
        (0, vec![])
    };
    Ok(RenameResult { path: to, links, files })
}

#[tauri::command]
pub fn delete_path(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or_else(paths::invalid_path)?;
    if rel.is_empty() {
        return Err(tr!("ルートディレクトリは削除できません", "The root directory cannot be deleted", "无法删除根目录"));
    }
    // Windows のごみ箱は \\wsl.localhost のファイルを扱えないので、WSL では Linux のごみ箱に移す
    if let Some(wsl) = state.wsl() {
        return wsl.agent.request("trash", json!({ "path": rel })).map(|_| ());
    }
    let p = resolve(&root, &rel)?;
    trash::delete(&p).map_err(|e| tr!("ごみ箱へ移動できません: {e}", "Cannot move to the Recycle Bin: {e}", "无法移到回收站：{e}"))
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
    Err(tr!("空いているファイル名が見つかりません", "No available file name was found", "找不到可用的文件名"))
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

/// エクスプローラーなどからファイルツリーにドロップしたファイル・フォルダを、ルート内のフォルダ（dest_dir）にコピーする。
/// 同じ名前があれば `名前 (2).md` のように番号を付ける。戻り値はコピー先（ルートからの相対パス）
#[tauri::command]
pub async fn import_paths(app: AppHandle, sources: Vec<String>, dest_dir: String) -> Result<Vec<String>, String> {
    let root = app.state::<AppState>().root()?;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = normalize_rel(&dest_dir).ok_or_else(paths::invalid_path)?;
        let dir_abs = resolve(&root, &dir)?;
        let mut copied = Vec::new();
        for s in sources {
            let src = PathBuf::from(&s);
            let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            if dir_abs.starts_with(&src) {
                return Err(tr!("{name} をその中にコピーすることはできません", "Cannot copy {name} into itself", "无法将 {name} 复制到其自身内部"));
            }
            let target = unique_path(&root, &paths::join_rel(&dir, &name), "")?;
            copy_recursive(&src, &resolve(&root, &target)?).map_err(|e| tr!("{name} をコピーできません: {e}", "Cannot copy {name}: {e}", "无法复制 {name}：{e}"))?;
            copied.push(target);
        }
        Ok(copied)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn duplicate_path(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let root = state.root()?;
    let rel = normalize_rel(&path).ok_or_else(paths::invalid_path)?;
    let suffix = tr!(" のコピー", " copy", " 副本");
    let target = unique_path(&root, &rel, &suffix)?;
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
    Err(tr!("空いているファイル名が見つかりません", "No available file name was found", "找不到可用的文件名"))
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

/// 画像や Office のファイルなど、エディタで開けないファイルを Windows の既定のアプリで開く
#[tauri::command]
pub fn open_with_default_app(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = state.root()?;
    let p = resolve(&root, &path)?;
    tauri_plugin_opener::open_path(&p, None::<&str>).map_err(|e| tr!("{path} を開けません: {e}", "Cannot open {path}: {e}", "无法打开 {path}：{e}"))
}

/// インストーラーで入れたものか（隣にアンインストーラーがある）。
/// 単体実行版（*-portable.exe）は自動アップデートできないので、画面側でリリースのページに案内する
#[tauri::command]
pub fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("uninstall.exe").exists()))
        .unwrap_or(false)
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
