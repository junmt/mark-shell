//! Git 連携（docs/git.md）。PC にインストール済みの git を子プロセスとして実行する。
//! WSL のフォルダでは WSL の中の git をエージェントが実行する。
//!
//! 画面から任意の git コマンドは実行できない。状態の取得は個別のコマンド、変更を伴う操作は `git_op` の決まった種類だけ。
//! リポジトリは git から見たパス（Windows なら `C:/...`、WSL なら `/home/...`）で指定する。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine;
use markshell_core::git::{self, ChangedFile, Commit, Output, Refs, Remote, Stash, Status};
use markshell_core::tr;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::config;
use crate::state::AppState;
use crate::textio;
use crate::wsl::WslRoot;

/// git をどこで実行するか
enum Exec {
    Local(PathBuf),
    Wsl(Arc<WslRoot>),
}

struct Ctx {
    app: AppHandle,
    exec: Exec,
    /// git から見たリポジトリのパス（作業ツリーの最上位、または git init する場所）
    repo: String,
}

/// 設定の `gitPath`、なければ PATH から探す
fn local_git() -> Option<PathBuf> {
    let configured = config::read_config().get("gitPath").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        return p.is_file().then_some(p);
    }
    which::which("git").ok()
}

fn not_found() -> String {
    tr!(
        "git が見つかりません。Git for Windows（https://git-scm.com/）をインストールしてください",
        "git was not found. Please install Git for Windows (https://git-scm.com/)",
        "找不到 git。请安装 Git for Windows（https://git-scm.com/）"
    )
}

fn exec_for(app: &AppHandle) -> Result<Exec, String> {
    let state = app.state::<AppState>();
    match state.wsl() {
        Some(w) => Ok(Exec::Wsl(w)),
        None => local_git().map(Exec::Local).ok_or_else(not_found),
    }
}

impl Ctx {
    fn new(app: &AppHandle, repo: &str) -> Result<Ctx, String> {
        Ok(Ctx { app: app.clone(), exec: exec_for(app)?, repo: repo.to_string() })
    }

    /// git を実行する（終了コードは見ない）
    fn raw(&self, args: &[&str], stdin: Option<&[u8]>, op: Option<u64>) -> Result<Output, String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match &self.exec {
            Exec::Local(gitexe) => {
                let app = self.app.clone();
                let progress = move |line: &str| {
                    let _ = app.emit("git:progress", json!({ "op": op, "line": line }));
                };
                git::run(gitexe, Path::new(&self.repo), &args, stdin, op, &progress)
            }
            Exec::Wsl(w) => {
                let v = w.agent.request(
                    "git",
                    json!({ "cwd": self.repo, "args": args, "stdin": stdin.map(git::to_b64), "op": op }),
                )?;
                serde_json::from_value(v).map_err(|e| e.to_string())
            }
        }
    }

    /// git を実行し、失敗したら git の出力をエラーにする
    fn run(&self, args: &[&str]) -> Result<Output, String> {
        self.run_with(args, None, None)
    }

    fn run_with(&self, args: &[&str], stdin: Option<&[u8]>, op: Option<u64>) -> Result<Output, String> {
        let out = self.raw(args, stdin, op)?;
        if out.ok() {
            return Ok(out);
        }
        let msg = out.stderr.trim();
        let msg = if msg.is_empty() { out.text().trim().to_string() } else { msg.to_string() };
        Err(if msg.is_empty() { format!("git {} (exit {})", args.first().unwrap_or(&""), out.code) } else { msg })
    }

    /// git から見たパスを、Windows から読み書きできるパスにする
    fn win_path(&self, p: &str) -> PathBuf {
        match &self.exec {
            Exec::Local(_) => PathBuf::from(p),
            Exec::Wsl(w) => PathBuf::from(format!(r"\\wsl.localhost\{}{}", w.distro, p.replace('/', "\\"))),
        }
    }

    /// `.git` の場所（Windows から読めるパス）。worktree やサブモジュールでは別の場所にある
    fn git_dir(&self) -> Result<PathBuf, String> {
        static CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(p) = cache.lock().unwrap().get(&self.repo) {
            return Ok(p.clone());
        }
        let out = self.run(&["rev-parse", "--absolute-git-dir"])?;
        let p = self.win_path(out.text().trim());
        cache.lock().unwrap().insert(self.repo.clone(), p.clone());
        Ok(p)
    }
}

/// 同じリポジトリに対する変更の操作は 1 つずつ順に実行する
fn repo_lock(repo: &str) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(Default::default).lock().unwrap().entry(repo.to_lowercase()).or_default().clone()
}

/// 時間のかかる処理を画面を止めずに実行する
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

// ================= リポジトリの検出 =================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// git から見た作業ツリーの最上位
    path: String,
    name: String,
    /// 最上位がルートディレクトリの中にあるとき、ルートからの相対パス（ルートそのものなら ""）
    prefix: Option<String>,
    /// 最上位がルートディレクトリより上にあるとき、最上位から見たルートの相対パス
    root_in_repo: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    /// git が使えるか
    found: bool,
    version: Option<String>,
    error: Option<String>,
    repos: Vec<RepoInfo>,
    /// git から見たルートディレクトリ（git init の場所）
    root: String,
}

fn norm(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_string()
}

/// a が b の中（または同じ）なら、b から見た a の相対パス
fn rel_inside(a: &str, b: &str, case_insensitive: bool) -> Option<String> {
    let (a, b) = (norm(a), norm(b));
    let (la, lb) = if case_insensitive { (a.to_lowercase(), b.to_lowercase()) } else { (a.clone(), b.clone()) };
    if la == lb {
        return Some(String::new());
    }
    let prefix = if lb.ends_with('/') { lb.clone() } else { format!("{lb}/") };
    la.starts_with(&prefix).then(|| a[prefix.len()..].to_string())
}

/// ルートディレクトリを含むリポジトリと、ルートの下にあるリポジトリ（`nested` は `.git` を持つフォルダのルートからの相対パス）
#[tauri::command]
pub async fn git_repos(app: AppHandle, nested: Vec<String>) -> Result<GitInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let root = state.root()?;
        let wsl = state.wsl();
        let root_git = match &wsl {
            Some(w) => w.linux_root.clone(),
            None => norm(&root.to_string_lossy()),
        };
        let ci = wsl.is_none();
        let ctx = match Ctx::new(&app, &root_git) {
            Ok(c) => c,
            Err(e) => return Ok(GitInfo { found: false, version: None, error: Some(e), repos: vec![], root: root_git }),
        };
        let version = match ctx.run(&["--version"]) {
            Ok(o) => o.text().trim().trim_start_matches("git version ").to_string(),
            Err(e) => {
                let error = if wsl.is_some() {
                    tr!("WSL の中で git を実行できません: {e}", "Cannot run git inside WSL: {e}", "无法在 WSL 中运行 git：{e}")
                } else {
                    e
                };
                return Ok(GitInfo { found: false, version: None, error: Some(error), repos: vec![], root: root_git });
            }
        };

        let mut tops: Vec<String> = Vec::new();
        let mut dirs = vec![root_git.clone()];
        for n in nested {
            let n = n.trim_matches('/');
            dirs.push(if n.is_empty() { root_git.clone() } else { format!("{}/{n}", root_git.trim_end_matches('/')) });
        }
        for d in dirs {
            let c = Ctx { app: app.clone(), exec: exec_for(&app)?, repo: d };
            if let Ok(o) = c.run(&["rev-parse", "--show-toplevel"]) {
                let top = norm(o.text().trim());
                if !top.is_empty() && !tops.iter().any(|t| if ci { t.eq_ignore_ascii_case(&top) } else { *t == top }) {
                    tops.push(top);
                }
            }
        }
        let repos = tops
            .into_iter()
            .map(|top| {
                let prefix = rel_inside(&top, &root_git, ci);
                let root_in_repo = if prefix.is_none() { rel_inside(&root_git, &top, ci) } else { None };
                let name = top.rsplit('/').next().unwrap_or(&top).to_string();
                RepoInfo { path: top, name, prefix, root_in_repo }
            })
            .collect();
        Ok(GitInfo { found: true, version: Some(version), error: None, repos, root: root_git })
    })
    .await
}

// ================= 状態 =================

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    /// 進行中の操作: "merge" "cherry-pick" "revert" "rebase"
    kind: Option<String>,
    /// 連続したチェリーピック・revert の残り（今のものを含む）
    remaining: Option<usize>,
    /// .git/MERGE_MSG（マージ・チェリーピックの既定のメッセージ）
    merge_msg: Option<String>,
    /// .git/SQUASH_MSG
    squash_msg: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    status: Status,
    state: RepoState,
    /// 最後にフェッチした日時（ミリ秒）
    last_fetch: Option<u64>,
}

fn read_state(dir: &Path) -> RepoState {
    let mut st = RepoState::default();
    if dir.join("rebase-merge").is_dir() || dir.join("rebase-apply").is_dir() {
        st.kind = Some("rebase".into());
    } else if dir.join("CHERRY_PICK_HEAD").is_file() {
        st.kind = Some("cherry-pick".into());
    } else if dir.join("REVERT_HEAD").is_file() {
        st.kind = Some("revert".into());
    } else if dir.join("MERGE_HEAD").is_file() {
        st.kind = Some("merge".into());
    }
    if st.kind.is_some() {
        if let Ok(todo) = fs::read_to_string(dir.join("sequencer").join("todo")) {
            st.remaining = Some(todo.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count());
        }
        st.merge_msg = fs::read_to_string(dir.join("MERGE_MSG")).ok();
    }
    st.squash_msg = fs::read_to_string(dir.join("SQUASH_MSG")).ok();
    st
}

#[tauri::command]
pub async fn git_status(app: AppHandle, repo: String) -> Result<StatusResult, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let out = ctx.run(&["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"])?;
        let status = git::parse_status(&out.stdout);
        let dir = ctx.git_dir()?;
        let last_fetch = fs::metadata(dir.join("FETCH_HEAD")).ok().map(|m| crate::paths::mtime_ms(&m));
        Ok(StatusResult { status, state: read_state(&dir), last_fetch })
    })
    .await
}

// ================= 履歴 =================

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    /// 表示するブランチ・コミット。空なら HEAD
    #[serde(default)]
    refs: Vec<String>,
    /// すべてのブランチ（ローカル・リモート・タグ）
    #[serde(default)]
    all: bool,
    /// このファイルを変えたコミットだけ（名前変更をたどる）
    path: Option<String>,
    #[serde(default)]
    skip: u32,
    limit: u32,
}

/// まだコミットのないリポジトリでは空にする
fn no_commits(e: &str) -> bool {
    e.contains("does not have any commits") || e.contains("bad default revision") || e.contains("unknown revision")
}

#[tauri::command]
pub async fn git_log(app: AppHandle, repo: String, query: LogQuery) -> Result<Vec<Commit>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let (n, skip) = (format!("-n{}", query.limit), format!("--skip={}", query.skip));
        let mut args: Vec<&str> = vec!["log", git::LOG_FORMAT, "--date-order", &n, &skip];
        if query.all {
            args.extend(["--branches", "--remotes", "--tags", "HEAD"]);
        } else if query.refs.is_empty() {
            args.push("HEAD");
        } else {
            args.extend(query.refs.iter().map(String::as_str));
        }
        if let Some(p) = &query.path {
            args.extend(["--follow", "--", p.as_str()]);
        } else {
            args.push("--");
        }
        match ctx.run(&args) {
            Ok(o) => Ok(git::parse_log(&o.stdout)),
            Err(e) if no_commits(&e) => Ok(vec![]),
            Err(e) => Err(e),
        }
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    commit: Commit,
    message: String,
    files: Vec<ChangedFile>,
}

/// コミットの全文のメッセージと、変更したファイル（マージコミットは 1 つめの親との差分）
#[tauri::command]
pub async fn git_commit_detail(app: AppHandle, repo: String, rev: String) -> Result<CommitDetail, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let commit = git::parse_log(&ctx.run(&["log", "-1", git::LOG_FORMAT, &rev, "--"])?.stdout)
            .into_iter()
            .next()
            .ok_or_else(|| tr!("コミットが見つかりません: {rev}", "Commit not found: {rev}", "找不到提交：{rev}"))?;
        let message = ctx.run(&["log", "-1", "--format=%B", &rev, "--"])?.text().trim_end().to_string();
        let files = match commit.parents.first() {
            Some(p) => ctx.run(&["diff-tree", "-r", "-z", "-M", "--name-status", p, &commit.hash])?,
            None => ctx.run(&["diff-tree", "-r", "-z", "-M", "--name-status", "--root", &commit.hash])?,
        };
        Ok(CommitDetail { commit, message, files: git::parse_name_status(&files.stdout) })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compare {
    /// base にないコミット（取り込むもの）
    incoming: Vec<Commit>,
    /// target にないコミット（送るもの）
    outgoing: Vec<Commit>,
    files: Vec<ChangedFile>,
}

/// 2 つのブランチ・コミットの比較
#[tauri::command]
pub async fn git_compare(app: AppHandle, repo: String, base: String, target: String) -> Result<Compare, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let log = |range: String| -> Result<Vec<Commit>, String> {
            Ok(git::parse_log(&ctx.run(&["log", git::LOG_FORMAT, "--date-order", "-n500", &range, "--"])?.stdout))
        };
        let incoming = log(format!("{base}..{target}"))?;
        let outgoing = log(format!("{target}..{base}"))?;
        let files = git::parse_name_status(&ctx.run(&["diff", "-z", "-M", "--name-status", &base, &target, "--"])?.stdout);
        Ok(Compare { incoming, outgoing, files })
    })
    .await
}

/// 現在のブランチに同じ変更がすでにあるコミット（チェリーピックの前に印を付ける）
#[tauri::command]
pub async fn git_cherry(app: AppHandle, repo: String, revs: Vec<String>) -> Result<Vec<String>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let mut applied = Vec::new();
        for rev in revs {
            // 現在のブランチから到達できる（すでに含まれている）コミット
            if ctx.raw(&["merge-base", "--is-ancestor", &rev, "HEAD"], None, None).map(|o| o.ok()).unwrap_or(false) {
                applied.push(rev);
                continue;
            }
            let parent = format!("{rev}^");
            // 親のないコミットは比べられないので印を付けない
            let Ok(o) = ctx.run(&["cherry", "HEAD", &rev, &parent]) else { continue };
            if o.text().lines().any(|l| l.starts_with('-')) {
                applied.push(rev);
            }
        }
        Ok(applied)
    })
    .await
}

/// コミット時点のファイルの一覧
#[tauri::command]
pub async fn git_ls_tree(app: AppHandle, repo: String, rev: String) -> Result<Vec<String>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let out = ctx.run(&["ls-tree", "-r", "-z", "--name-only", &rev])?;
        Ok(out.text().split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    })
    .await
}

// ================= ファイルの中身 =================

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileSide {
    exists: bool,
    binary: bool,
    size: u64,
    text: Option<String>,
    encoding: String,
    eol: String,
    /// 画像なら data URL
    image: Option<String>,
}

const MAX_TEXT: usize = 50 * 1024 * 1024;
const MAX_IMAGE: usize = 10 * 1024 * 1024;

fn image_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

fn side_from_bytes(path: &str, bytes: &[u8]) -> FileSide {
    let mut s = FileSide { exists: true, size: bytes.len() as u64, ..Default::default() };
    if let Some(mime) = image_mime(path) {
        if bytes.len() <= MAX_IMAGE {
            s.image = Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
        }
    }
    if bytes.len() > MAX_TEXT || textio::looks_binary(bytes) || (s.image.is_some() && !path.to_lowercase().ends_with(".svg")) {
        s.binary = true;
        return s;
    }
    let d = textio::decode(bytes);
    s.text = Some(d.text);
    s.encoding = d.encoding;
    s.eol = d.eol.to_string();
    s
}

/// ファイルの中身。`rev` が None なら作業ツリー、"" ならステージ（index）、":2" などは競合中のステージ、それ以外はコミット
#[tauri::command]
pub async fn git_show_file(app: AppHandle, repo: String, rev: Option<String>, path: String) -> Result<FileSide, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        match rev {
            None => {
                let p = ctx.win_path(&format!("{}/{path}", repo.trim_end_matches('/')));
                match fs::read(&p) {
                    Ok(b) => Ok(side_from_bytes(&path, &b)),
                    Err(_) => Ok(FileSide::default()),
                }
            }
            Some(rev) => {
                let object = if rev.is_empty() { format!(":{path}") } else { format!("{rev}:{path}") };
                // 作業ツリーと比べられるよう、改行コードの変換などのフィルタを通す
                let out = ctx.raw(&["cat-file", "--filters", &object], None, None)?;
                if !out.ok() {
                    return Ok(FileSide::default());
                }
                Ok(side_from_bytes(&path, &out.stdout))
            }
        }
    })
    .await
}

// ================= ブランチ・リモート・スタッシュ =================

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefsResult {
    #[serde(flatten)]
    refs: Refs,
    remotes: Vec<Remote>,
}

#[tauri::command]
pub async fn git_refs(app: AppHandle, repo: String) -> Result<RefsResult, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let remotes = git::parse_remotes(&ctx.run(&["remote", "-v"])?.stdout);
        let names: Vec<String> = remotes.iter().map(|r| r.name.clone()).collect();
        let out = ctx.run(&["for-each-ref", git::REFS_FORMAT, "refs/heads", "refs/remotes", "refs/tags"])?;
        Ok(RefsResult { refs: git::parse_refs(&out.stdout, &names), remotes })
    })
    .await
}

#[tauri::command]
pub async fn git_stashes(app: AppHandle, repo: String) -> Result<Vec<Stash>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        match ctx.run(&["stash", "list", git::STASH_FORMAT]) {
            Ok(o) => Ok(git::parse_stashes(&o.stdout)),
            Err(e) if no_commits(&e) => Ok(vec![]),
            Err(e) => Err(e),
        }
    })
    .await
}

#[tauri::command]
pub async fn git_stash_files(app: AppHandle, repo: String, index: u32) -> Result<Vec<ChangedFile>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let s = format!("stash@{{{index}}}");
        let out = ctx.run(&["stash", "show", "-z", "-M", "--name-status", &s])?;
        Ok(git::parse_name_status(&out.stdout))
    })
    .await
}

#[tauri::command]
pub async fn git_config_get(app: AppHandle, repo: String, key: String) -> Result<Option<String>, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let out = ctx.raw(&["config", "--get", &key], None, None)?;
        Ok(out.ok().then(|| out.text().trim().to_string()).filter(|s| !s.is_empty()))
    })
    .await
}

// ================= 変更を伴う操作 =================

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    Init,
    Stage { paths: Vec<String> },
    StageAll,
    Unstage { paths: Vec<String> },
    UnstageAll,
    /// 作業ツリーの変更をステージ（index）の内容に戻す
    Discard { paths: Vec<String> },
    /// 未追跡のファイルをごみ箱へ移す
    DiscardUntracked { paths: Vec<String> },
    /// ステージの中身を書き換える（ハンク・行ごとのステージ）
    SetIndex { path: String, text: String, encoding: String, eol: String },
    Commit { message: String, amend: bool, signoff: bool },
    Checkout { name: String },
    CheckoutDetached { rev: String },
    /// リモートのブランチから同じ名前のローカルブランチを作って切り替える
    CheckoutTrack { remote_ref: String, local: String },
    CreateBranch { name: String, start: Option<String>, checkout: bool },
    RenameBranch { from: String, to: String },
    DeleteBranch { name: String, force: bool },
    DeleteRemoteBranch { remote: String, name: String },
    /// mode: "ff"（早送りできれば早送り）"no-ff" "squash"
    Merge { rev: String, mode: String },
    CherryPick { revs: Vec<String>, no_commit: bool, record_origin: bool, mainline: Option<u32> },
    Revert { rev: String, mainline: Option<u32> },
    /// kind: "merge" "cherry-pick" "revert"、action: "continue" "skip" "abort"
    Sequencer { kind: String, action: String },
    Fetch { remote: Option<String>, branch: Option<String> },
    /// mode: "ff-only" "merge" "rebase"。None ならユーザーの設定に従う
    Pull { mode: Option<String> },
    Push { remote: Option<String>, branch: Option<String>, set_upstream: bool, force: bool },
    CreateTag { name: String, rev: String, message: Option<String> },
    DeleteTag { name: String },
    PushTag { remote: String, name: String },
    StashPush { message: Option<String>, untracked: bool },
    StashApply { index: u32 },
    StashPop { index: u32 },
    StashDrop { index: u32 },
    AddRemote { name: String, url: String },
    SetRemoteUrl { name: String, url: String },
    RemoveRemote { name: String },
    SetUser { name: String, email: String, global: bool },
    AppendGitignore { line: String },
}

fn with_paths<'a>(mut args: Vec<&'a str>, paths: &'a [String]) -> Vec<&'a str> {
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
}

fn has_head(ctx: &Ctx) -> bool {
    ctx.raw(&["rev-parse", "--verify", "-q", "HEAD"], None, None).map(|o| o.ok()).unwrap_or(false)
}

/// 操作を実行する。戻り値は git の出力（フックの出力などを見せるため）
fn run_op(ctx: &Ctx, op: Op, op_id: Option<u64>) -> Result<String, String> {
    let out = |o: Output| format!("{}{}", o.text(), o.stderr);
    let r = |args: &[&str]| ctx.run_with(args, None, op_id).map(out);
    match op {
        Op::Init => r(&["init"]),
        Op::Stage { paths } => r(&with_paths(vec!["add", "-A"], &paths)),
        Op::StageAll => r(&["add", "-A"]),
        Op::Unstage { paths } => {
            if has_head(ctx) {
                r(&with_paths(vec!["restore", "--staged"], &paths))
            } else {
                r(&with_paths(vec!["rm", "--cached", "-r", "-q"], &paths))
            }
        }
        Op::UnstageAll => {
            if has_head(ctx) {
                r(&["reset", "-q"])
            } else {
                r(&["rm", "--cached", "-r", "-q", "."])
            }
        }
        Op::Discard { paths } => r(&with_paths(vec!["restore", "--worktree"], &paths)),
        Op::DiscardUntracked { paths } => {
            for p in &paths {
                let abs = format!("{}/{p}", ctx.repo.trim_end_matches('/'));
                match &ctx.exec {
                    Exec::Wsl(w) => {
                        w.agent.request("trashAbs", json!({ "path": abs }))?;
                    }
                    Exec::Local(_) => trash::delete(ctx.win_path(&abs))
                        .map_err(|e| tr!("ごみ箱へ移動できません: {e}", "Cannot move to the Recycle Bin: {e}", "无法移到回收站：{e}"))?,
                }
            }
            Ok(String::new())
        }
        Op::SetIndex { path, text, encoding, eol } => {
            let bytes = textio::encode(&text, &encoding, &eol)?;
            let path_arg = format!("--path={path}");
            let sha = ctx.run_with(&["hash-object", "-w", "--stdin", &path_arg], Some(&bytes), None)?.text().trim().to_string();
            // 実行権限などのモードは今のステージのものを使う（新しいファイルなら 100644）
            let ls = ctx.run(&["ls-files", "-s", "--", &path])?.text();
            let mode = ls.split_whitespace().next().filter(|m| m.len() == 6).unwrap_or("100644").to_string();
            let info = format!("{mode},{sha},{path}");
            r(&["update-index", "--add", "--cacheinfo", &info])
        }
        Op::Commit { message, amend, signoff } => {
            let mut args = vec!["commit", "-F", "-"];
            if amend {
                args.push("--amend");
            }
            if signoff {
                args.push("--signoff");
            }
            ctx.run_with(&args, Some(message.as_bytes()), op_id).map(out)
        }
        Op::Checkout { name } => r(&["switch", &name]),
        Op::CheckoutDetached { rev } => r(&["switch", "--detach", &rev]),
        Op::CheckoutTrack { remote_ref, local } => r(&["switch", "-c", &local, "--track", &remote_ref]),
        Op::CreateBranch { name, start, checkout } => {
            let start = start.unwrap_or_else(|| "HEAD".into());
            if checkout {
                r(&["switch", "-c", &name, &start])
            } else {
                r(&["branch", &name, &start])
            }
        }
        Op::RenameBranch { from, to } => r(&["branch", "-m", &from, &to]),
        Op::DeleteBranch { name, force } => r(&["branch", if force { "-D" } else { "-d" }, &name]),
        Op::DeleteRemoteBranch { remote, name } => r(&["push", "--progress", &remote, "--delete", &name]),
        Op::Merge { rev, mode } => {
            let flag = match mode.as_str() {
                "no-ff" => "--no-ff",
                "squash" => "--squash",
                _ => "--ff",
            };
            r(&["merge", "--no-edit", flag, &rev])
        }
        Op::CherryPick { revs, no_commit, record_origin, mainline } => {
            let m = mainline.map(|m| m.to_string());
            let mut args = vec!["cherry-pick"];
            if no_commit {
                args.push("--no-commit");
            }
            if record_origin {
                args.push("-x");
            }
            if let Some(m) = &m {
                args.extend(["-m", m.as_str()]);
            }
            args.extend(revs.iter().map(String::as_str));
            r(&args)
        }
        Op::Revert { rev, mainline } => {
            let m = mainline.map(|m| m.to_string());
            let mut args = vec!["revert", "--no-edit"];
            if let Some(m) = &m {
                args.extend(["-m", m.as_str()]);
            }
            args.push(&rev);
            r(&args)
        }
        Op::Sequencer { kind, action } => {
            let cmd = match kind.as_str() {
                "merge" | "cherry-pick" | "revert" => kind.as_str(),
                _ => return Err(format!("unknown: {kind}")),
            };
            let flag = match action.as_str() {
                "continue" => "--continue",
                "skip" if cmd != "merge" => "--skip",
                "abort" => "--abort",
                _ => return Err(format!("unknown: {action}")),
            };
            r(&[cmd, flag])
        }
        Op::Fetch { remote, branch } => match (remote, branch) {
            (Some(remote), Some(branch)) => r(&["fetch", "--progress", "--prune", &remote, &branch]),
            (Some(remote), None) => r(&["fetch", "--progress", "--prune", "--tags", &remote]),
            _ => r(&["fetch", "--progress", "--prune", "--tags", "--all"]),
        },
        Op::Pull { mode } => match mode.as_deref() {
            Some("ff-only") => r(&["pull", "--progress", "--ff-only"]),
            Some("merge") => r(&["pull", "--progress", "--no-rebase", "--no-edit"]),
            Some("rebase") => r(&["pull", "--progress", "--rebase"]),
            _ => r(&["pull", "--progress", "--no-edit"]),
        },
        Op::Push { remote, branch, set_upstream, force } => {
            let mut args = vec!["push", "--progress"];
            if force {
                args.push("--force-with-lease");
            }
            if set_upstream {
                args.push("-u");
            }
            if let Some(remote) = &remote {
                args.push(remote);
                if let Some(b) = &branch {
                    args.push(b);
                }
            }
            r(&args)
        }
        Op::CreateTag { name, rev, message } => match message.filter(|m| !m.trim().is_empty()) {
            Some(m) => ctx.run_with(&["tag", "-a", &name, "-F", "-", &rev], Some(m.as_bytes()), None).map(out),
            None => r(&["tag", &name, &rev]),
        },
        Op::DeleteTag { name } => r(&["tag", "-d", &name]),
        Op::PushTag { remote, name } => {
            let refspec = format!("refs/tags/{name}");
            r(&["push", "--progress", &remote, &refspec])
        }
        Op::StashPush { message, untracked } => {
            let mut args = vec!["stash", "push"];
            if untracked {
                args.push("--include-untracked");
            }
            if let Some(m) = message.as_deref().filter(|m| !m.trim().is_empty()) {
                args.extend(["-m", m]);
            }
            r(&args)
        }
        Op::StashApply { index } => r(&["stash", "apply", &format!("stash@{{{index}}}")]),
        Op::StashPop { index } => r(&["stash", "pop", &format!("stash@{{{index}}}")]),
        Op::StashDrop { index } => r(&["stash", "drop", &format!("stash@{{{index}}}")]),
        Op::AddRemote { name, url } => r(&["remote", "add", &name, &url]),
        Op::SetRemoteUrl { name, url } => r(&["remote", "set-url", &name, &url]),
        Op::RemoveRemote { name } => r(&["remote", "remove", &name]),
        Op::SetUser { name, email, global } => {
            let scope = if global { "--global" } else { "--local" };
            r(&["config", scope, "user.name", &name])?;
            r(&["config", scope, "user.email", &email])
        }
        Op::AppendGitignore { line } => {
            let p = ctx.win_path(&format!("{}/.gitignore", ctx.repo.trim_end_matches('/')));
            let mut s = fs::read_to_string(&p).unwrap_or_default();
            if s.lines().any(|l| l.trim() == line.trim()) {
                return Ok(String::new());
            }
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(line.trim());
            s.push('\n');
            fs::write(&p, s).map_err(|e| e.to_string())?;
            Ok(String::new())
        }
    }
}

/// 変更を伴う操作。`op_id` を渡すと取り消せるようにし、進捗を git:progress で送る
#[tauri::command]
pub async fn git_op(app: AppHandle, repo: String, op: Op, op_id: Option<u64>) -> Result<String, String> {
    blocking(move || {
        let ctx = Ctx::new(&app, &repo)?;
        let lock = repo_lock(&repo);
        let _guard = lock.lock().unwrap();
        run_op(&ctx, op, op_id)
    })
    .await
}

#[tauri::command]
pub fn git_cancel(app: AppHandle, op_id: u64) {
    match app.state::<AppState>().wsl() {
        Some(w) => {
            let _ = w.agent.request("gitCancel", json!({ "op": op_id }));
        }
        None => git::cancel(op_id),
    }
}

// ================= .git の監視（ルートの外にあるとき） =================

/// リポジトリの .git がルートディレクトリの外にあるとき、その変化を git:changed で知らせる
/// （ルートの中なら、ふつうのファイルの監視 fs:change で分かる）
#[tauri::command]
pub async fn git_watch(app: AppHandle, repos: Vec<String>) -> Result<(), String> {
    static WATCHERS: OnceLock<Mutex<Vec<markshell_core::watch::Watcher>>> = OnceLock::new();
    blocking(move || {
        let state = app.state::<AppState>();
        let root = state.root()?;
        let mut list = WATCHERS.get_or_init(Default::default).lock().unwrap();
        list.clear();
        if state.wsl().is_some() {
            return Ok(());
        }
        for repo in repos {
            let ctx = Ctx::new(&app, &repo)?;
            let dir = ctx.git_dir()?;
            if dir.starts_with(&root) || rel_inside(&dir.to_string_lossy(), &root.to_string_lossy(), true).is_some() {
                continue;
            }
            let filter = Arc::new(crate::paths::FilterConfig::from_config(&Value::Null));
            let app2 = app.clone();
            let repo2 = repo.clone();
            if let Ok(w) = markshell_core::watch::start(dir, filter, move |_| {
                let _ = app2.emit("git:changed", json!({ "repo": repo2 }));
            }) {
                list.push(w);
            }
        }
        Ok(())
    })
    .await
}

// ================= リポジトリごとの状態（コミットメッセージの下書き） =================

fn state_path(repo: &str) -> PathBuf {
    config::local_dir().join("git").join(format!("{}.json", config::path_key(repo)))
}

#[tauri::command]
pub fn git_load_state(repo: String) -> Value {
    fs::read_to_string(state_path(&repo)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

#[tauri::command]
pub fn git_save_state(repo: String, value: Value) -> Result<(), String> {
    let p = state_path(&repo);
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    textio::write_atomic(&p, s.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside() {
        assert_eq!(rel_inside("C:/a/b", "C:/a", true).as_deref(), Some("b"));
        assert_eq!(rel_inside("c:/A", "C:\\a\\", true).as_deref(), Some(""));
        assert_eq!(rel_inside("C:/ab", "C:/a", true), None);
        assert_eq!(rel_inside("/home/x/notes/sub", "/home/x/notes", false).as_deref(), Some("sub"));
        assert_eq!(rel_inside("/", "/", false).as_deref(), Some(""));
    }
}
