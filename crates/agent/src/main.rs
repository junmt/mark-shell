//! WSL の中で動くエージェント。mark-shell（Windows）から `wsl.exe` 経由で起動され、
//! 標準入出力で 1 行 1 つの JSON をやり取りする。
//!
//! - 要求: `{"id": 1, "method": "scan", "params": {...}}`
//! - 応答: `{"id": 1, "ok": ...}` または `{"id": 1, "err": "..."}`
//! - 通知: `{"event": "scan:batch", "payload": {...}}`。名前と中身は画面側が受け取るイベントと同じ
//!
//! Windows から `\\wsl.localhost` 経由では、フォルダ全体をたどる処理が遅く、変更の通知も届かない。
//! そこで探索・検索・置換・リンクの書き換え・変更の監視・ごみ箱への移動をここで行う。
//! 1 つのファイルの読み書きは Windows 側から直接行う。標準入力が閉じたら（アプリが終了したら）終わる。

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use markshell_core::paths::{normalize_rel, resolve, FilterConfig};
use markshell_core::search::SearchOptions;
use markshell_core::watch::WatchEvent;
use markshell_core::{git, i18n, links, scan, search, tr, watch};
use serde::Deserialize;
use serde_json::{json, Value};

fn send(v: &Value) {
    let line = serde_json::to_string(v).unwrap_or_default();
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn emit(event: &str, payload: Value) {
    send(&json!({ "event": event, "payload": payload }));
}

struct Agent {
    root: RwLock<Option<PathBuf>>,
    filter: RwLock<Arc<FilterConfig>>,
    watcher: Mutex<Option<watch::Watcher>>,
    scan_gen: Arc<AtomicU64>,
    search_id: Arc<AtomicU64>,
}

impl Agent {
    fn new() -> Self {
        Self {
            root: RwLock::new(None),
            filter: RwLock::new(Arc::new(FilterConfig::from_config(&Value::Null))),
            watcher: Mutex::new(None),
            scan_gen: Arc::new(AtomicU64::new(0)),
            search_id: Arc::new(AtomicU64::new(0)),
        }
    }

    fn root(&self) -> Result<PathBuf, String> {
        self.root.read().unwrap().clone().ok_or_else(|| tr!("フォルダが開かれていません", "No folder is open", "未打开文件夹"))
    }

    fn filter(&self) -> Arc<FilterConfig> {
        self.filter.read().unwrap().clone()
    }
}

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

fn param<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(params.get(key).cloned().unwrap_or(Value::Null)).map_err(|e| format!("{key}: {e}"))
}

fn handle(agent: &Arc<Agent>, method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "ping" => Ok(json!({ "version": env!("CARGO_PKG_VERSION") })),

        // 表示言語（エラーメッセージの言語）
        "locale" => {
            i18n::set_locale(i18n::parse(&param::<String>(params, "locale")?));
            Ok(json!({}))
        }

        // フォルダを開く。設定を受け取り、監視を張り直す
        "open" => {
            if let Some(l) = params.get("locale").and_then(Value::as_str) {
                i18n::set_locale(i18n::parse(l));
            }
            let root = PathBuf::from(param::<String>(params, "root")?);
            if !root.is_dir() {
                let root = root.display();
                return Err(tr!("フォルダが見つかりません: {root}", "Folder not found: {root}", "找不到文件夹：{root}"));
            }
            let filter = Arc::new(FilterConfig::from_config(params.get("config").unwrap_or(&Value::Null)));
            *agent.root.write().unwrap() = Some(root.clone());
            *agent.filter.write().unwrap() = filter.clone();
            // 古い監視を先に止める
            *agent.watcher.lock().unwrap() = None;
            let w = watch::start(root, filter, |ev| match ev {
                WatchEvent::Changes(changes) => emit("fs:change", json!({ "changes": changes })),
                WatchEvent::Rescan => emit("fs:rescan", Value::Null),
            })
            .map_err(|e| tr!("変更の監視を始められません: {e}", "Cannot start watching for changes: {e}", "无法开始监视更改：{e}"))?;
            *agent.watcher.lock().unwrap() = Some(w);
            Ok(json!({}))
        }

        "config" => {
            *agent.filter.write().unwrap() = Arc::new(FilterConfig::from_config(params.get("config").unwrap_or(&Value::Null)));
            Ok(json!({}))
        }

        // 探索。結果は scan:batch / scan:done で送る。新しい世代が来たら古い探索は止まる
        "scan" => {
            let gen: u64 = param(params, "gen")?;
            let root = agent.root()?;
            let filter = agent.filter();
            agent.scan_gen.store(gen, Ordering::SeqCst);
            let current = agent.scan_gen.clone();
            std::thread::spawn(move || {
                let s = scan::scan(&root, &root, &filter, || current.load(Ordering::Relaxed) != gen, |entries| {
                    emit("scan:batch", json!({ "gen": gen, "entries": entries }));
                });
                emit("scan:done", json!({ "gen": gen, "files": s.files, "dirs": s.dirs, "truncated": s.truncated }));
            });
            Ok(json!({}))
        }

        // フォルダの直下を読む（探索が上限で止まったときに画面側が補う）
        "listDir" => {
            let root = agent.root()?;
            let dir = resolve(&root, &param::<String>(params, "path")?)?;
            serde_json::to_value(scan::list_dir(&root, &dir, &agent.filter())).map_err(|e| e.to_string())
        }

        // 全文検索。結果は search:batch / search:done で送る
        "search" => {
            let id: u64 = param(params, "id")?;
            let options: SearchOptions = param(params, "options")?;
            let root = agent.root()?;
            let filter = agent.filter();
            agent.search_id.store(id, Ordering::SeqCst);
            let current = agent.search_id.clone();
            std::thread::spawn(move || {
                let cancelled = || current.load(Ordering::Relaxed) != id;
                let r = search::search(&root, &filter, &options, cancelled, |hits| {
                    emit("search:batch", json!({ "id": id, "hits": hits }));
                });
                if cancelled() {
                    return;
                }
                match r {
                    Ok(s) => emit("search:done", json!({ "id": id, "total": s.total, "truncated": s.truncated, "error": null })),
                    Err(e) => emit("search:done", json!({ "id": id, "total": 0, "truncated": false, "error": e })),
                }
            });
            Ok(json!({}))
        }

        "searchCancel" => {
            agent.search_id.fetch_add(1, Ordering::SeqCst);
            Ok(json!({}))
        }

        "replace" => {
            let options: SearchOptions = param(params, "options")?;
            let replacement: String = param(params, "replacement")?;
            let paths: Vec<String> = param(params, "paths")?;
            let r = search::replace(&agent.root()?, &options, &replacement, &paths)?;
            serde_json::to_value(r).map_err(|e| e.to_string())
        }

        // 名前変更・移動に合わせたリンクの書き換え（apply が false なら数えるだけ）
        "linkPlan" => {
            let from: String = param(params, "from")?;
            let to: String = param(params, "to")?;
            let apply: bool = param(params, "apply")?;
            let r = links::plan(&agent.root()?, &agent.filter(), &from, &to, apply)?;
            serde_json::to_value(r).map_err(|e| e.to_string())
        }

        // Windows のごみ箱は \\wsl.localhost のファイルを扱えないので、Linux のごみ箱に移す
        "trash" => {
            let rel = normalize_rel(&param::<String>(params, "path")?).ok_or_else(markshell_core::paths::invalid_path)?;
            if rel.is_empty() {
                return Err(tr!("ルートディレクトリは削除できません", "The root directory cannot be deleted", "无法删除根目录"));
            }
            let p = resolve(&agent.root()?, &rel)?;
            trash::delete(&p).map_err(|e| tr!("ごみ箱へ移動できません: {e}", "Cannot move to the trash: {e}", "无法移到回收站：{e}"))?;
            Ok(json!({}))
        }

        // WSL の中の git を実行する（Git 連携）。進捗は git:progress で送る
        "git" => {
            let cwd: String = param(params, "cwd")?;
            let args: Vec<String> = param(params, "args")?;
            let stdin: Option<String> = param(params, "stdin")?;
            let op: Option<u64> = param(params, "op")?;
            let stdin = stdin.map(|s| git::from_b64(&s)).transpose()?;
            let progress = |line: &str| emit("git:progress", json!({ "op": op, "line": line }));
            let out = git::run(std::path::Path::new("git"), std::path::Path::new(&cwd), &args, stdin.as_deref(), op, &progress)?;
            serde_json::to_value(out).map_err(|e| e.to_string())
        }

        "gitCancel" => {
            git::cancel(param(params, "op")?);
            Ok(json!({}))
        }

        // 未追跡のファイルの破棄（Git 連携）。ルートの外にあることもあるので絶対パスで受け取る
        "trashAbs" => {
            let p = PathBuf::from(param::<String>(params, "path")?);
            trash::delete(&p).map_err(|e| tr!("ごみ箱へ移動できません: {e}", "Cannot move to the trash: {e}", "无法移到回收站：{e}"))?;
            Ok(json!({}))
        }

        // ログインシェルの PATH でコマンドを探す（~/.local/bin などは .profile で足されることが多い）
        "which" => {
            let command: String = param(params, "command")?;
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            let out = std::process::Command::new(shell)
                .args(["-lc", "command -v \"$1\"", "sh", &command])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .map_err(|e| e.to_string())?;
            let path = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").trim().to_string();
            Ok(if out.status.success() && !path.is_empty() { json!(path) } else { Value::Null })
        }

        // Windows のパスを Linux のパスに変える（/mnt/c/... など）
        "wslpath" => {
            let path: String = param(params, "path")?;
            let out = std::process::Command::new("wslpath")
                .args(["-u", &path])
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| tr!("wslpath を実行できません: {e}", "Cannot run wslpath: {e}", "无法运行 wslpath：{e}"))?;
            if !out.status.success() {
                return Err(tr!("パスを変換できません: {path}", "Cannot convert the path: {path}", "无法转换路径：{path}"));
            }
            Ok(json!(String::from_utf8_lossy(&out.stdout).trim()))
        }

        _ => Err(tr!("不明な要求です: {method}", "Unknown request: {method}", "未知的请求：{method}")),
    }
}

fn main() {
    let agent = Arc::new(Agent::new());
    let stdin = std::io::stdin();
    let mut running: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for line in stdin.lock().lines() {
        running.retain(|h| !h.is_finished());
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[agent] 要求を読めません: {e}");
                continue;
            }
        };
        // 時間のかかる要求（リンクの書き換えなど）で他を待たせないよう、要求ごとにスレッドを分ける
        let agent = agent.clone();
        running.push(std::thread::spawn(move || {
            let reply = match handle(&agent, &req.method, &req.params) {
                Ok(v) => json!({ "id": req.id, "ok": v }),
                Err(e) => json!({ "id": req.id, "err": e }),
            };
            send(&reply);
        }));
    }
    // アプリが終了して標準入力が閉じたら、受け付けた要求に答えてから終わる
    // （探索・検索のスレッドや監視は待たない）
    for h in running {
        let _ = h.join();
    }
    std::process::exit(0);
}
