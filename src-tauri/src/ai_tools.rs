//! インストール済み AI ツールの検出と起動コマンドの組み立て。
//! ツール定義は %APPDATA%\mark-shell\ai-tools.json に置く（ルートディレクトリには置かない）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use markshell_core::i18n::{self, Locale};
use markshell_core::tr;
use serde::{Deserialize, Serialize};

use crate::config::config_dir;

const DEFAULT_TOOLS: &str = r#"{
  "tools": [
    {
      "id": "claude-code",
      "name": "Claude Code",
      "kind": "cli",
      "detect": {
        "command": "claude",
        "paths": [
          "%USERPROFILE%\\.local\\bin\\claude.exe",
          "~/.local/bin/claude",
          "/opt/homebrew/bin/claude",
          "/usr/local/bin/claude"
        ]
      },
      "launch": { "shell": "default", "args": [], "cwd": "${root}" },
      "installUrl": "https://docs.claude.com/en/docs/claude-code/setup",
      "actions": [
        { "id": "summarize", "label": "このファイルを要約する", "args": ["${noteRelativePath} を読んで、要点をまとめてください"] },
        { "id": "proofread", "label": "このファイルを推敲する", "args": ["${noteRelativePath} の誤字脱字や分かりにくい表現を直してください"] },
        { "id": "selection", "label": "選択範囲について聞く", "args": ["${selectionFile} に、${noteRelativePath} から選択した部分を書き出しました。この部分について説明してください"] }
      ]
    },
    {
      "id": "gemini-cli",
      "name": "Gemini CLI",
      "kind": "cli",
      "detect": { "command": "gemini", "paths": ["/opt/homebrew/bin/gemini", "/usr/local/bin/gemini", "~/.local/bin/gemini"] },
      "launch": { "shell": "default", "args": [], "cwd": "${root}" },
      "installUrl": "https://github.com/google-gemini/gemini-cli",
      "actions": [
        { "id": "summarize", "label": "このファイルを要約する", "args": ["-i", "${noteRelativePath} を読んで、要点をまとめてください"] }
      ]
    },
    {
      "id": "codex-cli",
      "name": "Codex CLI",
      "kind": "cli",
      "detect": { "command": "codex", "paths": ["/opt/homebrew/bin/codex", "/usr/local/bin/codex", "~/.local/bin/codex"] },
      "launch": { "shell": "default", "args": [], "cwd": "${root}" },
      "installUrl": "https://github.com/openai/codex",
      "actions": [
        { "id": "summarize", "label": "このファイルを要約する", "args": ["${noteRelativePath} を読んで、要点をまとめてください"] }
      ]
    },
    {
      "id": "antigravity",
      "name": "Antigravity CLI",
      "kind": "cli",
      "detect": {
        "command": "agy",
        "paths": [
          "%USERPROFILE%\\.local\\bin\\agy.exe",
          "~/.local/bin/agy",
          "/opt/homebrew/bin/agy",
          "/usr/local/bin/agy"
        ]
      },
      "launch": { "shell": "default", "args": [], "cwd": "${root}" },
      "installUrl": "https://antigravity.google/",
      "actions": [
        { "id": "summarize", "label": "このファイルを要約する", "args": ["-i", "${noteRelativePath} を読んで、要点をまとめてください"] },
        { "id": "proofread", "label": "このファイルを推敲する", "args": ["-i", "${noteRelativePath} の誤字脱字や分かりにくい表現を直してください"] },
        { "id": "selection", "label": "選択範囲について聞く", "args": ["-i", "${selectionFile} に、${noteRelativePath} から選択した部分を書き出しました。この部分について説明してください"] }
      ]
    }
  ]
}
"#;

/// DEFAULT_TOOLS の日本語の文言と、その英語・中国語訳。
/// ai-tools.json は一度書き出すと日本語のまま残るので、使うときに既定の文言と同じなら今の言語に置き換える
/// （ユーザーが書き換えた文言はそのまま）
const BUILTIN_TEXTS: &[(&str, &str, &str)] = &[
    ("このファイルを要約する", "Summarize this file", "总结此文件"),
    ("このファイルを推敲する", "Polish this file", "润色此文件"),
    ("選択範囲について聞く", "Ask about the selection", "询问所选内容"),
    (
        "${noteRelativePath} を読んで、要点をまとめてください",
        "Read ${noteRelativePath} and summarize the key points",
        "请阅读 ${noteRelativePath} 并总结要点",
    ),
    (
        "${noteRelativePath} の誤字脱字や分かりにくい表現を直してください",
        "Fix typos and unclear wording in ${noteRelativePath}",
        "请修正 ${noteRelativePath} 中的错别字和表达不清的地方",
    ),
    (
        "${selectionFile} に、${noteRelativePath} から選択した部分を書き出しました。この部分について説明してください",
        "I saved the part selected from ${noteRelativePath} to ${selectionFile}. Please explain this part",
        "已将从 ${noteRelativePath} 中选中的部分写入 ${selectionFile}。请解释这部分内容",
    ),
];

/// 既定の文言なら指定の言語に訳す。それ以外はそのまま
fn localize_builtin(text: &str, l: Locale) -> String {
    match BUILTIN_TEXTS.iter().find(|(ja, _, _)| *ja == text) {
        Some((ja, en, zh)) => match l {
            Locale::Ja => ja,
            Locale::En => en,
            Locale::Zh => zh,
        }
        .to_string(),
        None => text.to_string(),
    }
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ToolsFile {
    tools: Vec<ToolDef>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Detect {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    /// "default"（PowerShell 経由）/ "none"（直接起動）
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub detached: bool,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default)]
    pub launch: Launch,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub install_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_command: Option<String>,
}

fn default_kind() -> String {
    "cli".into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    id: String,
    name: String,
    kind: String,
    found: bool,
    path: Option<String>,
    install_url: Option<String>,
    usage_command: Option<String>,
    actions: Vec<ActionInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResult {
    pub id: String,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct ActionInfo {
    id: String,
    label: String,
}

fn tools_path() -> PathBuf {
    config_dir().join("ai-tools.json")
}

fn load() -> Vec<ToolDef> {
    let path = tools_path();
    if !path.exists() {
        let _ = std::fs::create_dir_all(config_dir());
        let _ = std::fs::write(&path, DEFAULT_TOOLS);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_TOOLS.into());
    let mut file = match serde_json::from_str::<ToolsFile>(&text) {
        Ok(f) => f,
        Err(_) => return serde_json::from_str::<ToolsFile>(DEFAULT_TOOLS).map(|f| f.tools).unwrap_or_default(),
    };
    if migrate(&mut file) {
        if let Ok(s) = serde_json::to_string_pretty(&file) {
            let _ = std::fs::write(&path, s);
        }
    }
    file.tools
}

/// 以前の既定の定義を今の既定に置き換える。置き換えたら true
/// - Antigravity: IDE（`antigravity` で別ウィンドウを開く）から Antigravity CLI（`agy` をターミナルで実行）へ
fn migrate(file: &mut ToolsFile) -> bool {
    let Ok(defaults) = serde_json::from_str::<ToolsFile>(DEFAULT_TOOLS) else { return false };
    let mut changed = false;
    for t in file.tools.iter_mut() {
        let old_antigravity = t.id == "antigravity" && t.kind == "ide" && t.detect.command.as_deref() == Some("antigravity");
        if old_antigravity {
            if let Some(d) = defaults.tools.iter().find(|d| d.id == t.id) {
                *t = d.clone();
                changed = true;
            }
        }
    }
    changed
}

/// `%NAME%` 形式の環境変数と先頭の `~` を展開する
fn expand_env(s: &str) -> String {
    let expanded_tilde = if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix(r"~\")) {
        if let Some(home) = dirs::home_dir() {
            home.join(rest).to_string_lossy().into_owned()
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };
    let mut out = String::new();
    let mut rest = expanded_tilde.as_str();
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) => {
                let name = &after[..j];
                out.push_str(&std::env::var(name).unwrap_or_default());
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// コマンド名から実行ファイルを探す。Mac で Finder / Dock から起動したときは PATH が最小限なので、よく使われる場所も見る
fn which_command(cmd: &str) -> Option<PathBuf> {
    if let Ok(p) = which::which(cmd) {
        return Some(p);
    }
    #[cfg(not(windows))]
    {
        let mut dirs = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")];
        if let Some(home) = dirs::home_dir() {
            dirs.insert(0, home.join(".local/bin"));
            dirs.push(home.join(".cargo/bin"));
        }
        for d in dirs {
            let p = d.join(cmd);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn detect(tool: &ToolDef) -> Option<PathBuf> {
    if let Some(cmd) = &tool.detect.command {
        if let Some(p) = which_command(cmd) {
            return Some(p);
        }
    }
    tool.detect
        .paths
        .iter()
        .map(|p| PathBuf::from(expand_env(p)))
        .find(|p| p.exists())
}

pub fn get(id: &str) -> Result<ToolDef, String> {
    load().into_iter().find(|t| t.id == id).ok_or_else(|| tr!("AI ツール {id} が定義されていません", "AI tool {id} is not defined", "未定义 AI 工具 {id}"))
}

impl ToolDef {
    /// 実行するコマンド名（WSL の中で探すときに使う）
    pub fn command_name(&self) -> Option<&str> {
        self.launch.command.as_deref().or(self.detect.command.as_deref())
    }
}

/// WSL のフォルダを開いているとき、CLI のツールは WSL の中で探す（ログインシェルの PATH で）
pub fn find_in_wsl(tool: &ToolDef, wsl: &crate::wsl::WslRoot) -> Option<String> {
    (tool.kind == "cli").then(|| tool.command_name().and_then(|c| wsl.which(c))).flatten()
}

pub fn find(id: &str) -> Result<(ToolDef, PathBuf), String> {
    let tool = get(id)?;
    let path = match &tool.launch.command {
        Some(c) => which_command(c).or_else(|| Some(PathBuf::from(expand_env(c))).filter(|p| p.exists())),
        None => detect(&tool),
    }
    .ok_or_else(|| {
        let name = &tool.name;
        tr!(
            "{name} が見つかりません。インストールされているか確認してください",
            "{name} was not found. Check that it is installed",
            "找不到 {name}。请确认是否已安装"
        )
    })?;
    Ok((tool, path))
}

/// `${root}` などの置換変数
pub struct Vars {
    pub map: HashMap<&'static str, String>,
}

impl Vars {
    pub fn new(root: &Path, note: Option<&str>, selection_file: Option<&Path>) -> Self {
        let mut map = HashMap::new();
        map.insert("root", root.to_string_lossy().into_owned());
        if let Some(n) = note {
            let abs = crate::paths::resolve(root, n).unwrap_or_else(|_| root.to_path_buf());
            map.insert("note", abs.to_string_lossy().into_owned());
            map.insert("noteRelativePath", n.to_string());
            map.insert("noteDir", abs.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
        }
        if let Some(s) = selection_file {
            map.insert("selectionFile", s.to_string_lossy().into_owned());
        }
        Self { map }
    }

    pub fn expand(&self, s: &str) -> String {
        let mut out = s.to_string();
        // 開いているファイルを Claude Code に伝えるフック定義（ai_context.rs）。使うときだけ書き出す
        // WSL の中で起動するときは、呼び出し側が WSL から見たパスを map に入れておく
        if out.contains("${claudeSettings}") && !self.map.contains_key("claudeSettings") {
            if let Ok(p) = crate::ai_context::claude_settings_file() {
                out = out.replace("${claudeSettings}", &p.to_string_lossy());
            }
        }
        for (k, v) in &self.map {
            out = out.replace(&format!("${{{k}}}"), v);
        }
        // 値のない変数は空にする
        regex::Regex::new(r"\$\{[A-Za-z]+\}").unwrap().replace_all(&out, "").into_owned()
    }
}

/// 選んだ操作の引数（操作が未指定ならツール既定の引数）
pub fn args_for(tool: &ToolDef, action: Option<&str>, vars: &Vars) -> Result<Vec<String>, String> {
    let raw = match action {
        Some(a) => {
            &tool
                .actions
                .iter()
                .find(|x| x.id == a)
                .ok_or_else(|| tr!("操作 {a} が定義されていません", "Action {a} is not defined", "未定义操作 {a}"))?
                .args
        }
        None => &tool.launch.args,
    };
    // 既定の文言（日本語）は今の言語に訳してから変数を展開する
    let l = i18n::locale();
    Ok(raw.iter().map(|a| vars.expand(&localize_builtin(a, l))).collect())
}

/// PowerShell の単一引用符文字列（中身はそのまま渡る）
#[cfg_attr(not(windows), allow(dead_code))]
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[tauri::command]
pub async fn list_ai_tools(app: tauri::AppHandle) -> Vec<ToolInfo> {
    use tauri::Manager;
    let wsl = app.state::<crate::state::AppState>().wsl();
    let l = i18n::locale();
    tauri::async_runtime::spawn_blocking(move || {
        load()
            .into_iter()
            .map(|t| {
                // WSL のフォルダでは CLI のツールを WSL の中で探す
                let path = match &wsl {
                    Some(w) if t.kind == "cli" => find_in_wsl(&t, w).map(PathBuf::from),
                    _ => detect(&t),
                };
                ToolInfo {
                    found: path.is_some(),
                    path: path.map(|p| p.to_string_lossy().into_owned()),
                    actions: t.actions.iter().map(|a| ActionInfo { id: a.id.clone(), label: localize_builtin(&a.label, l) }).collect(),
                    id: t.id,
                    name: t.name,
                    kind: t.kind,
                    install_url: t.install_url,
                    usage_command: t.usage_command,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// IDE 型のツール（kind: "ide"）を別ウィンドウで起動する
#[tauri::command]
pub fn launch_ide_tool(
    state: tauri::State<'_, crate::state::AppState>,
    tool_id: String,
    action_id: Option<String>,
    note: Option<String>,
) -> Result<(), String> {
    let root = state.root()?;
    let (tool, path) = find(&tool_id)?;
    let vars = Vars::new(&root, note.as_deref(), None);
    let args = args_for(&tool, action_id.as_deref(), &vars)?;
    let mut cmd = std::process::Command::new(&path);
    cmd.args(&args).current_dir(&root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0008); // CREATE_NO_WINDOW | DETACHED_PROCESS
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| {
            let name = &tool.name;
            tr!("{name} を起動できません: {e}", "Cannot start {name}: {e}", "无法启动 {name}：{e}")
        })
}

#[tauri::command]
pub fn ai_tools_file_path() -> String {
    let _ = load();
    tools_path().to_string_lossy().into_owned()
}

/// 標準出力に含まれる ANSI エスケープシーケンスを取り除く
fn strip_ansi(s: &str) -> String {
    regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
        .unwrap()
        .replace_all(s, "")
        .into_owned()
}

/// 子プロセスを上限時間つきで実行し、(成功したか, 標準出力, 標準エラー) を返す
fn run_command_timeout(mut cmd: std::process::Command, timeout: std::time::Duration) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>), String> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(64 * 1024).read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(64 * 1024).read_to_end(&mut buf);
        buf
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(tr!("コマンドがタイムアウトしました", "Command timed out", "命令超时"));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    };
    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    Ok((status, out, err))
}

/// 各 AI ツールの Usage 取得コマンドを実行して標準出力を返す
#[tauri::command]
pub async fn run_ai_usage(
    app: tauri::AppHandle,
    tool_id: String,
    command: Option<String>,
    note: Option<String>,
) -> UsageResult {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    let root = state.root().ok();
    let wsl = state.wsl();
    tauri::async_runtime::spawn_blocking(move || {
        let tool = load().into_iter().find(|t| t.id == tool_id);
        let raw_cmd = command
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| tool.as_ref().and_then(|t| t.usage_command.as_ref()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        let Some(raw_cmd) = raw_cmd else {
            return UsageResult { id: tool_id, output: String::new(), error: None };
        };
        let is_cli = tool.as_ref().map(|t| t.kind == "cli").unwrap_or(true);
        let cmd = match (&wsl, is_cli) {
            (Some(w), true) => {
                let mut map = HashMap::new();
                map.insert("root", w.linux_root.clone());
                if let Some(n) = note.as_deref() {
                    map.insert("note", w.linux_path(n));
                    map.insert("noteRelativePath", n.to_string());
                    map.insert("noteDir", w.linux_path(crate::paths::parent_rel(n)));
                }
                let vars = Vars { map };
                let line = vars.expand(&raw_cmd);
                let mut c = std::process::Command::new(crate::wsl::wsl_exe());
                c.args(["-d", &w.distro, "--cd", &w.linux_root, "--exec", "sh", "-lc", &line]);
                c.env("NO_COLOR", "1");
                c.env("MARKSHELL_ROOT", &w.linux_root);
                c.env("WSLENV", "NO_COLOR:MARKSHELL_ROOT");
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                }
                c
            }
            _ => {
                let vars = match &root {
                    Some(r) => Vars::new(r, note.as_deref(), None),
                    None => Vars { map: HashMap::new() },
                };
                let line = vars.expand(&raw_cmd);
                let cwd = root
                    .clone()
                    .filter(|p| p.is_dir())
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(std::env::temp_dir);
                #[cfg(windows)]
                let mut c = {
                    use std::os::windows::process::CommandExt;
                    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into());
                    let mut c = std::process::Command::new(comspec);
                    c.args(["/d", "/c", &format!("chcp 65001 >nul & {line}")]);
                    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                    c
                };
                #[cfg(not(windows))]
                let mut c = {
                    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
                    let mut c = std::process::Command::new(shell);
                    c.args(["-c", &line]);
                    c
                };
                c.current_dir(&cwd);
                c.env("NO_COLOR", "1");
                if let Some(r) = &root {
                    c.env("MARKSHELL_ROOT", r.to_string_lossy().as_ref());
                }
                c
            }
        };
        match run_command_timeout(cmd, std::time::Duration::from_secs(15)) {
            Ok((status, stdout, stderr)) => {
                let out = strip_ansi(&String::from_utf8_lossy(&stdout)).trim().to_string();
                let err = strip_ansi(&String::from_utf8_lossy(&stderr)).trim().to_string();
                if status.success() {
                    UsageResult {
                        id: tool_id,
                        output: if out.is_empty() { err } else { out },
                        error: None,
                    }
                } else {
                    let msg = if !err.is_empty() {
                        err
                    } else if !out.is_empty() {
                        out
                    } else {
                        format!("exit {}", status.code().unwrap_or(-1))
                    };
                    UsageResult { id: tool_id, output: String::new(), error: Some(msg) }
                }
            }
            Err(e) => UsageResult { id: tool_id, output: String::new(), error: Some(e) },
        }
    })
    .await
    .unwrap_or_else(|e| UsageResult { id: String::new(), output: String::new(), error: Some(e.to_string()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tools_parse() {
        let f: ToolsFile = serde_json::from_str(DEFAULT_TOOLS).unwrap();
        assert_eq!(f.tools.len(), 4);
    }

    #[test]
    fn vars() {
        let v = Vars::new(Path::new(r"C:\Notes"), Some("a/b.md"), None);
        assert_eq!(v.expand("${noteRelativePath} x ${selectionFile}"), "a/b.md x ");
        assert_eq!(ps_quote("it's"), "'it''s'");
    }

    #[test]
    fn builtin_texts_localized() {
        let f: ToolsFile = serde_json::from_str(DEFAULT_TOOLS).unwrap();
        // 既定の文言にはすべて訳がある
        for a in f.tools.iter().flat_map(|t| &t.actions) {
            assert_ne!(localize_builtin(&a.label, Locale::En), a.label);
            for arg in a.args.iter().filter(|x| !x.is_ascii()) {
                assert_ne!(localize_builtin(arg, Locale::En), *arg);
                assert_ne!(localize_builtin(arg, Locale::Zh), *arg);
            }
        }
        assert_eq!(localize_builtin("このファイルを要約する", Locale::Ja), "このファイルを要約する");
        assert_eq!(localize_builtin("このファイルを要約する", Locale::Zh), "总结此文件");
        assert_eq!(
            localize_builtin("${noteRelativePath} を読んで、要点をまとめてください", Locale::En),
            "Read ${noteRelativePath} and summarize the key points"
        );
        // ユーザーが書き換えた文言はそのまま
        assert_eq!(localize_builtin("自分の操作", Locale::En), "自分の操作");
    }

    #[test]
    fn migrate_old_antigravity_ide() {
        let old = r#"{"tools":[
          {"id":"antigravity","name":"Antigravity","kind":"ide","detect":{"command":"antigravity"},"launch":{"args":["${root}"],"detached":true}},
          {"id":"my-tool","name":"Mine","kind":"ide","detect":{"command":"antigravity"}}
        ]}"#;
        let mut f: ToolsFile = serde_json::from_str(old).unwrap();
        assert!(migrate(&mut f));
        assert_eq!(f.tools[0].kind, "cli");
        assert_eq!(f.tools[0].detect.command.as_deref(), Some("agy"));
        // id が違う定義には触れない
        assert_eq!(f.tools[1].kind, "ide");
        // 移行済みなら何もしない
        assert!(!migrate(&mut f));
    }

    #[test]
    fn strip_ansi_and_usage_command() {
        assert_eq!(strip_ansi("\x1b[32m50%\x1b[0m ($2.10)"), "50% ($2.10)");
        let json = r#"{"tools":[{"id":"claude-code","name":"Claude Code","usageCommand":"ccusage"}]}"#;
        let f: ToolsFile = serde_json::from_str(json).unwrap();
        assert_eq!(f.tools[0].usage_command.as_deref(), Some("ccusage"));
    }
}
