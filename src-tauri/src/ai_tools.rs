//! インストール済み AI ツールの検出と起動コマンドの組み立て。
//! ツール定義は %APPDATA%\mark-shell\ai-tools.json に置く（ルートディレクトリには置かない）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::config_dir;

const DEFAULT_TOOLS: &str = r#"{
  "tools": [
    {
      "id": "claude-code",
      "name": "Claude Code",
      "kind": "cli",
      "detect": { "command": "claude", "paths": ["%USERPROFILE%\\.local\\bin\\claude.exe"] },
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
      "detect": { "command": "gemini" },
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
      "detect": { "command": "codex" },
      "launch": { "shell": "default", "args": [], "cwd": "${root}" },
      "installUrl": "https://github.com/openai/codex",
      "actions": [
        { "id": "summarize", "label": "このファイルを要約する", "args": ["${noteRelativePath} を読んで、要点をまとめてください"] }
      ]
    },
    {
      "id": "antigravity",
      "name": "Antigravity",
      "kind": "ide",
      "detect": {
        "command": "antigravity",
        "paths": ["%LOCALAPPDATA%\\Programs\\Antigravity\\Antigravity.exe", "%ProgramFiles%\\Antigravity\\Antigravity.exe"]
      },
      "launch": { "args": ["${root}"], "detached": true },
      "installUrl": "https://antigravity.google/",
      "actions": [
        { "id": "open-note", "label": "このファイルを開く", "args": ["${root}", "${note}"] }
      ]
    }
  ]
}
"#;

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
    actions: Vec<ActionInfo>,
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
    serde_json::from_str::<ToolsFile>(&text)
        .or_else(|_| serde_json::from_str::<ToolsFile>(DEFAULT_TOOLS))
        .map(|f| f.tools)
        .unwrap_or_default()
}

/// `%NAME%` 形式の環境変数を展開する
fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
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

fn detect(tool: &ToolDef) -> Option<PathBuf> {
    if let Some(cmd) = &tool.detect.command {
        if let Ok(p) = which::which(cmd) {
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
    load().into_iter().find(|t| t.id == id).ok_or_else(|| format!("AI ツール {id} が定義されていません"))
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
        Some(c) => which::which(c).ok().or_else(|| Some(PathBuf::from(expand_env(c)))).filter(|p| p.exists()),
        None => detect(&tool),
    }
    .ok_or_else(|| format!("{} が見つかりません。インストールされているか確認してください", tool.name))?;
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
        Some(a) => &tool.actions.iter().find(|x| x.id == a).ok_or_else(|| format!("操作 {a} が定義されていません"))?.args,
        None => &tool.launch.args,
    };
    Ok(raw.iter().map(|a| vars.expand(a)).collect())
}

/// PowerShell の単一引用符文字列（中身はそのまま渡る）
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[tauri::command]
pub async fn list_ai_tools(app: tauri::AppHandle) -> Vec<ToolInfo> {
    use tauri::Manager;
    let wsl = app.state::<crate::state::AppState>().wsl();
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
                    actions: t.actions.iter().map(|a| ActionInfo { id: a.id.clone(), label: a.label.clone() }).collect(),
                    id: t.id,
                    name: t.name,
                    kind: t.kind,
                    install_url: t.install_url,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// IDE 型のツール（Antigravity など）を別ウィンドウで起動する
#[tauri::command]
pub fn launch_ide_tool(
    state: tauri::State<'_, crate::state::AppState>,
    tool_id: String,
    action_id: Option<String>,
    note: Option<String>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let root = state.root()?;
    let (tool, path) = find(&tool_id)?;
    let vars = Vars::new(&root, note.as_deref(), None);
    let args = args_for(&tool, action_id.as_deref(), &vars)?;
    std::process::Command::new(&path)
        .args(&args)
        .current_dir(&root)
        .creation_flags(0x0800_0000 | 0x0000_0008) // CREATE_NO_WINDOW | DETACHED_PROCESS
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} を起動できません: {e}", tool.name))
}

#[tauri::command]
pub fn ai_tools_file_path() -> String {
    let _ = load();
    tools_path().to_string_lossy().into_owned()
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
}
