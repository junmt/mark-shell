//! WSL 統合。`\\wsl.localhost\<ディストリビューション>\...` のフォルダを開いたときに使う。
//!
//! Windows から `\\wsl.localhost` 経由では、フォルダ全体をたどる処理が遅く、変更の通知も届かない。
//! そこで WSL の中でエージェント（crates/agent）を動かし、探索・検索・置換・リンクの書き換え・
//! 変更の監視・ごみ箱への移動を任せる。1 つのファイルの読み書きは `\\wsl.localhost` 経由で直接行う。
//!
//! エージェントはアプリに埋め込んであり（build.rs）、初回に `~/.mark-shell/` に置いて `wsl.exe` で起動する。
//! 標準入出力で 1 行 1 つの JSON をやり取りし、エージェントからの通知はそのまま画面側のイベントとして送る。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::paths::normalize_rel;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(has_wsl_agent)]
const AGENT_BIN: &[u8] = include_bytes!(env!("MARKSHELL_AGENT_BIN"));
#[cfg(not(has_wsl_agent))]
const AGENT_BIN: &[u8] = &[];

/// エージェントを埋め込んでビルドしたか（`npm run agent` を先に実行していない開発ビルドでは false）
pub fn agent_available() -> bool {
    !AGENT_BIN.is_empty()
}

/// `\\wsl.localhost\Ubuntu\home\jun\notes`（または `\\wsl$\...`）→ ("Ubuntu", "/home/jun/notes")
pub fn parse_unc(path: &str) -> Option<(String, String)> {
    let p = path.replace('/', "\\");
    let rest = p.strip_prefix(r"\\?\UNC\").or_else(|| p.strip_prefix(r"\\"))?;
    let mut parts = rest.split('\\').filter(|s| !s.is_empty());
    let host = parts.next()?;
    if !host.eq_ignore_ascii_case("wsl.localhost") && !host.eq_ignore_ascii_case("wsl$") {
        return None;
    }
    let distro = parts.next()?.to_string();
    let linux = format!("/{}", parts.collect::<Vec<_>>().join("/"));
    Some((distro, linux))
}

pub fn wsl_exe() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    PathBuf::from(root).join(r"System32\wsl.exe")
}

fn wsl_command(distro: &str) -> Command {
    let mut c = Command::new(wsl_exe());
    c.args(["-d", distro, "--exec"]).creation_flags(CREATE_NO_WINDOW);
    c
}

/// 中身が変わったら別の名前になるよう、FNV-1a のハッシュを名前に入れる
fn agent_file_name() -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in AGENT_BIN {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("agent-{h:016x}")
}

/// `~/.mark-shell/agent-<hash>` がなければ置く（古い版は消す）
fn deploy(distro: &str, name: &str) -> Result<(), String> {
    let installed = wsl_command(distro)
        .args(["sh", "-c", &format!("test -x \"$HOME/.mark-shell/{name}\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("wsl.exe を起動できません: {e}"))?
        .success();
    if installed {
        return Ok(());
    }
    let script = format!(
        "set -e; d=\"$HOME/.mark-shell\"; mkdir -p \"$d\"; rm -f \"$d\"/agent-*; \
         cat > \"$d/{name}.tmp\"; chmod +x \"$d/{name}.tmp\"; mv \"$d/{name}.tmp\" \"$d/{name}\""
    );
    let mut child = wsl_command(distro)
        .args(["sh", "-c", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("wsl.exe を起動できません: {e}"))?;
    child.stdin.take().unwrap().write_all(AGENT_BIN).map_err(|e| format!("WSL にエージェントを置けません: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("WSL にエージェントを置けません: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

type Pending = Mutex<HashMap<u64, mpsc::Sender<Result<Value, String>>>>;

pub struct Agent {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Arc<Pending>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
}

impl Agent {
    fn start(app: &AppHandle, distro: &str) -> Result<Arc<Agent>, String> {
        if !agent_available() {
            return Err("この版には WSL 用のエージェントが含まれていません".into());
        }
        let name = agent_file_name();
        deploy(distro, &name)?;
        let mut child = wsl_command(distro)
            .args(["sh", "-c", &format!("exec \"$HOME/.mark-shell/{name}\"")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("WSL のエージェントを起動できません: {e}"))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let pending: Arc<Pending> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));

        // 応答は待っている要求に返し、通知は画面側のイベントとして送る
        let (app2, pending2, alive2) = (app.clone(), pending.clone(), alive.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(event) = msg.get("event").and_then(Value::as_str) {
                    let _ = app2.emit(event, msg.get("payload").cloned().unwrap_or(Value::Null));
                } else if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = pending2.lock().unwrap().remove(&id) {
                        let r = match msg.get("err").and_then(Value::as_str) {
                            Some(e) => Err(e.to_string()),
                            None => Ok(msg.get("ok").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(r);
                    }
                }
            }
            // 終了した。待っている要求には失敗を返す（送り手を捨てると受け手が Err になる）
            alive2.store(false, Ordering::SeqCst);
            pending2.lock().unwrap().clear();
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("[wsl agent] {line}");
            }
        });

        let agent = Arc::new(Agent { stdin: Mutex::new(stdin), child: Mutex::new(child), pending, next_id: AtomicU64::new(1), alive });
        agent.request("ping", json!({}))?;
        Ok(agent)
    }

    /// 要求を送り、応答を待つ
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("WSL のエージェントが終了しています。フォルダを開き直してください".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let line = serde_json::to_string(&json!({ "id": id, "method": method, "params": params })).map_err(|e| e.to_string())?;
        {
            let mut stdin = self.stdin.lock().unwrap();
            if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() {
                self.pending.lock().unwrap().remove(&id);
                return Err("WSL のエージェントに送れません".into());
            }
        }
        rx.recv().map_err(|_| "WSL のエージェントが終了しました".to_string())?
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

/// ディストリビューションごとに 1 つのエージェントを使い回す（終了していたら起動し直す）
fn agent_for(app: &AppHandle, distro: &str) -> Result<Arc<Agent>, String> {
    static AGENTS: OnceLock<Mutex<HashMap<String, Arc<Agent>>>> = OnceLock::new();
    let mut agents = AGENTS.get_or_init(Default::default).lock().unwrap();
    if let Some(a) = agents.get(distro).filter(|a| a.alive.load(Ordering::SeqCst)) {
        return Ok(a.clone());
    }
    let a = Agent::start(app, distro)?;
    agents.insert(distro.to_string(), a.clone());
    Ok(a)
}

/// 開いている WSL のフォルダ
pub struct WslRoot {
    pub distro: String,
    /// Linux 側のパス（/home/jun/notes）
    pub linux_root: String,
    pub agent: Arc<Agent>,
    /// Windows のパス → Linux のパス（wslpath の結果）
    linux_paths: Mutex<HashMap<PathBuf, String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WslInfo {
    pub distro: String,
    pub linux_root: String,
}

impl WslRoot {
    /// エージェントでフォルダを開く（監視も始まる）
    pub fn open(app: &AppHandle, distro: &str, linux_root: &str, config: &Value) -> Result<Arc<WslRoot>, String> {
        let agent = agent_for(app, distro)?;
        agent.request("open", json!({ "root": linux_root, "config": config }))?;
        Ok(Arc::new(WslRoot { distro: distro.into(), linux_root: linux_root.into(), agent, linux_paths: Mutex::default() }))
    }

    pub fn info(&self) -> WslInfo {
        WslInfo { distro: self.distro.clone(), linux_root: self.linux_root.clone() }
    }

    /// ルートからの相対パス → Linux のパス
    pub fn linux_path(&self, rel: &str) -> String {
        match normalize_rel(rel).filter(|r| !r.is_empty()) {
            Some(r) if self.linux_root == "/" => format!("/{r}"),
            Some(r) => format!("{}/{r}", self.linux_root),
            None => self.linux_root.clone(),
        }
    }

    /// Windows のパス（C:\...）→ WSL から見たパス（/mnt/c/...）
    pub fn to_linux(&self, win: &std::path::Path) -> Result<String, String> {
        if let Some(p) = self.linux_paths.lock().unwrap().get(win) {
            return Ok(p.clone());
        }
        let p = self.agent.request("wslpath", json!({ "path": win.to_string_lossy() }))?;
        let p = p.as_str().ok_or("パスを変換できません")?.to_string();
        self.linux_paths.lock().unwrap().insert(win.to_path_buf(), p.clone());
        Ok(p)
    }

    /// ログインシェルの PATH でコマンドを探す
    pub fn which(&self, command: &str) -> Option<String> {
        self.agent.request("which", json!({ "command": command })).ok()?.as_str().map(str::to_string)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unc() {
        assert_eq!(parse_unc(r"\\wsl.localhost\Ubuntu\home\jun\notes"), Some(("Ubuntu".into(), "/home/jun/notes".into())));
        assert_eq!(parse_unc(r"\\wsl$\Ubuntu-22.04\home"), Some(("Ubuntu-22.04".into(), "/home".into())));
        assert_eq!(parse_unc(r"\\?\UNC\wsl.localhost\Ubuntu"), Some(("Ubuntu".into(), "/".into())));
        assert_eq!(parse_unc(r"\\server\share\x"), None);
        assert_eq!(parse_unc(r"C:\Users"), None);
    }
}
