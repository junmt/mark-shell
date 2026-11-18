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
      "usageCommand": "npx --yes ccusage@latest claude daily --json --offline",
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
      "usageCommand": "npx --yes ccusage@latest gemini daily --json --offline",
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
      "usageCommand": "npx --yes ccusage@latest codex daily --json --offline",
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
      "usageCommand": "mark-shell --ai-usage antigravity",
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
    pub detail: String,
    pub error: Option<String>,
    /// あとどれくらい使えるか（レート制限の枠ごとの使用率とリセット時刻）。取れないツールは空
    pub limits: Vec<crate::rate_limits::Limit>,
    /// limits を記録した時刻（UNIX 秒）
    pub limits_updated_at: Option<u64>,
    /// limits が取れないときの理由（画面に出す）
    pub limits_note: Option<String>,
}

impl UsageResult {
    fn new(id: String, output: String, detail: String, error: Option<String>) -> Self {
        UsageResult { id, output, detail, error, limits: Vec::new(), limits_updated_at: None, limits_note: None }
    }
}

/// ツールごとに、あとどれくらい使えるかを足す
fn with_limits(mut r: UsageResult) -> UsageResult {
    match r.id.as_str() {
        "claude-code" => match (claude_limits(), crate::rate_limits::read_claude()) {
            (Ok((limits, at)), _) => {
                r.limits = limits;
                r.limits_updated_at = Some(at);
            }
            // /usage が使えないときは、ステータスラインで記録した値を使う
            (Err(_), Some(rec)) => {
                r.limits = rec.limits;
                r.limits_updated_at = Some(rec.updated_at);
            }
            (Err(_), None) if crate::rate_limits::user_has_statusline() => {
                r.limits_note = Some(tr!(
                    "ステータスラインを自分で設定しているため、残りを取得できません",
                    "Remaining usage is unavailable because you set your own status line",
                    "由于你自行设置了状态栏，无法获取剩余用量"
                ));
            }
            (Err(_), None) => {
                r.limits_note = Some(tr!(
                    "mark-shell のターミナルで Claude Code を使うと、残りを表示します",
                    "Use Claude Code in a mark-shell terminal to show the remaining usage",
                    "在 mark-shell 的终端中使用 Claude Code 后，会显示剩余用量"
                ));
            }
        },
        "codex-cli" => {
            let (limits, at, _, _) = codex_local_stats();
            r.limits = limits;
            r.limits_updated_at = at;
        }
        "antigravity" => match antigravity_limits() {
            Ok((limits, at)) => {
                r.limits = limits;
                r.limits_updated_at = Some(at);
            }
            Err(note) => r.limits_note = Some(note),
        },
        _ => {}
    }
    r
}

/// `<cli> -p /usage` の結果を使い回す時間。CLI の起動に数秒かかるので、Usage の更新ごとには呼ばない
const SLASH_USAGE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// 残り（枠ごとの使用率と回復時刻）と、取得した時刻（UNIX 秒）
type LimitsResult = Result<(Vec<crate::rate_limits::Limit>, u64), String>;
type LimitsCache = std::sync::Mutex<Option<(std::time::Instant, LimitsResult)>>;

/// 取得結果を 5 分間使い回す。取得中はロックを持ったままにして、同時に何度も CLI を起動しない
fn cached_limits(cache: &LimitsCache, fetch: fn() -> LimitsResult) -> LimitsResult {
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, res)) = cache.as_ref() {
        if at.elapsed() < SLASH_USAGE_TTL {
            return res.clone();
        }
    }
    let res = fetch();
    *cache = Some((std::time::Instant::now(), res.clone()));
    res
}

fn antigravity_limits() -> LimitsResult {
    static CACHE: LimitsCache = std::sync::Mutex::new(None);
    cached_limits(&CACHE, fetch_antigravity_limits)
}

fn claude_limits() -> LimitsResult {
    static CACHE: LimitsCache = std::sync::Mutex::new(None);
    cached_limits(&CACHE, fetch_claude_limits)
}

/// AI ツールの CLI を引数つきでウィンドウを出さずに実行し、成功したときの標準出力を返す
fn run_tool_cli(tool_id: &str, args: &[&str]) -> Option<String> {
    let (_, path) = find(tool_id).ok()?;
    let mut cmd = std::process::Command::new(path);
    cmd.args(args);
    cmd.current_dir(dirs::home_dir().unwrap_or_else(std::env::temp_dir));
    cmd.env("NO_COLOR", "1");
    // CLI の自動更新を止める。Antigravity の agy は 15 分おきに自動更新の確認を切り離したプロセス
    // （agy --bg-updater）で行い、その中で実行する `agy --version` に新しいコンソールが作られる。
    // CREATE_NO_WINDOW は切り離されたプロセスの子には届かないので、既定のターミナル（Windows Terminal）の
    // 黒いウィンドウが一瞬表示されていた。Claude Code も念のため同じように止める（更新はユーザーが使うときに行われる）
    cmd.env("AGY_CLI_DISABLE_AUTO_UPDATE", "true");
    cmd.env("DISABLE_AUTOUPDATER", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let (status, out, _) = run_command_timeout(cmd, std::time::Duration::from_secs(30)).ok()?;
    status.success().then(|| strip_ansi(&String::from_utf8_lossy(&out)))
}

/// Claude Code のスラッシュコマンド /usage を印刷モードで実行して残りを読む。
/// `--no-session-persistence` で、実行のたびに会話の記録（~/.claude/projects）が増えないようにする
fn fetch_claude_limits() -> LimitsResult {
    let out = run_tool_cli("claude-code", &["-p", "/usage", "--no-session-persistence"]).unwrap_or_default();
    let now = chrono::Local::now();
    let limits = parse_claude_usage(&out, now);
    if limits.is_empty() {
        return Err(String::new());
    }
    Ok((limits, now.timestamp().max(0) as u64))
}

/// `claude -p /usage` の出力を読む。例:
/// `Current session: 7% used · resets Nov 18, 3pm (Asia/Tokyo)`
/// `Current week (all models): 8% used · resets Nov 23, 8am (Asia/Tokyo)`
/// `Current week (Fable): 0% used · resets Nov 23, 8am (Asia/Tokyo)`
fn parse_claude_usage<Tz: chrono::TimeZone>(text: &str, now: chrono::DateTime<Tz>) -> Vec<crate::rate_limits::Limit> {
    let re = regex::Regex::new(r"^\s*Current\s+(session|week|day)\s*(?:\(([^)]*)\))?\s*:\s*(\d+(?:\.\d+)?)%\s*used(?:.*?\bresets\s+(.+))?$").unwrap();
    text.lines()
        .filter_map(|line| {
            let c = re.captures(line)?;
            let window_minutes = match &c[1] {
                "session" => crate::rate_limits::FIVE_HOURS,
                "week" => crate::rate_limits::SEVEN_DAYS,
                _ => 1440,
            };
            let label = c.get(2).map(|m| m.as_str().trim()).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all models"));
            Some(crate::rate_limits::Limit {
                window_minutes,
                used_percent: c[3].parse().ok()?,
                resets_at: c.get(4).and_then(|m| parse_claude_reset(m.as_str(), &now)),
                label: label.map(str::to_string),
            })
        })
        .collect()
}

/// 「Nov 18, 3pm (Asia/Tokyo)」「3:30pm (Asia/Tokyo)」のような回復時刻を UNIX 秒にする。
/// 年は書かれていないので、いまより前になるなら翌年（日付がないときは翌日）とみなす。
/// タイムゾーンは Claude Code が PC のタイムゾーンで出すので、now と同じとみなす
fn parse_claude_reset<Tz: chrono::TimeZone>(s: &str, now: &chrono::DateTime<Tz>) -> Option<u64> {
    use chrono::{Datelike, Duration, NaiveDate};
    let re = regex::Regex::new(r"(?i)^(?:([A-Z][a-z]{2})[a-z]*\.?\s+(\d{1,2}),?\s*(?:at\s+)?)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)").unwrap();
    let c = re.captures(s.trim())?;
    let mut hour: u32 = c[3].parse().ok()?;
    let minute: u32 = c.get(4).map_or(Some(0), |m| m.as_str().parse().ok())?;
    let pm = c[5].eq_ignore_ascii_case("pm");
    if hour == 12 {
        hour = 0;
    }
    if pm {
        hour += 12;
    }
    let tz = now.timezone();
    let local_now = now.naive_local();
    let at = |date: NaiveDate| {
        let naive = date.and_hms_opt(hour, minute, 0)?;
        tz.from_local_datetime(&naive).earliest()
    };
    let t = match (c.get(1), c.get(2)) {
        (Some(mon), Some(day)) => {
            let month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
                .iter()
                .position(|m| m.eq_ignore_ascii_case(mon.as_str()))? as u32
                + 1;
            let day: u32 = day.as_str().parse().ok()?;
            let this_year = at(NaiveDate::from_ymd_opt(local_now.year(), month, day)?)?;
            // 年末に年明けの日付が出たときは翌年
            if this_year.clone() + Duration::days(31) < *now {
                at(NaiveDate::from_ymd_opt(local_now.year() + 1, month, day)?)?
            } else {
                this_year
            }
        }
        _ => {
            let today = at(local_now.date())?;
            if today < *now {
                at(local_now.date().succ_opt()?)?
            } else {
                today
            }
        }
    };
    u64::try_from(t.timestamp()).ok()
}

/// Antigravity CLI のスラッシュコマンド /usage を印刷モード（`agy -p /usage`）で実行して残りを読む
fn fetch_antigravity_limits() -> LimitsResult {
    let limits = parse_agy_usage(&run_tool_cli("antigravity", &["-p", "/usage"]).unwrap_or_default());
    if limits.is_empty() {
        return Err(tr!(
            "Antigravity CLI（agy）にログインすると、残りを表示します",
            "Sign in to Antigravity CLI (agy) to show the remaining usage",
            "登录 Antigravity CLI（agy）后，会显示剩余用量"
        ));
    }
    Ok((limits, now_secs()))
}

/// `agy -p /usage` の出力（1 行 1 枠のタブ区切り: グループ、枠の名前、残り%、回復時刻）を読む。
/// 例: `Gemini Models\tFive Hour Limit Remaining\t100%\t2026-11-18T07:50:28Z`
fn parse_agy_usage(text: &str) -> Vec<crate::rate_limits::Limit> {
    text.lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [group, name, pct, reset, ..] = cols.as_slice() else { return None };
            let remaining: f64 = pct.strip_suffix('%')?.trim().parse().ok()?;
            let lower = name.to_ascii_lowercase();
            let window_minutes = if lower.contains("five hour") || lower.contains("5 hour") || lower.contains("5-hour") {
                crate::rate_limits::FIVE_HOURS
            } else if lower.contains("week") {
                crate::rate_limits::SEVEN_DAYS
            } else if lower.contains("daily") || lower.contains("day") {
                1440
            } else {
                0
            };
            // 「Gemini Models」→「Gemini」、「Claude and GPT models」→「Claude/GPT」
            let label = group.trim_end_matches(" Models").trim_end_matches(" models").replace(" and ", "/");
            Some(crate::rate_limits::Limit {
                window_minutes,
                used_percent: (100.0 - remaining).max(0.0),
                resets_at: parse_iso_secs(reset),
                label: Some(label),
            })
        })
        .collect()
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
/// - usageCommand が未設定の既定ツールにはプリセットの usageCommand を補う
fn migrate(file: &mut ToolsFile) -> bool {
    let Ok(defaults) = serde_json::from_str::<ToolsFile>(DEFAULT_TOOLS) else { return false };
    let mut changed = false;
    for t in file.tools.iter_mut() {
        let old_antigravity = t.id == "antigravity" && t.kind == "ide" && t.detect.command.as_deref() == Some("antigravity");
        if old_antigravity {
            if let Some(d) = defaults.tools.iter().find(|d| d.id == t.id) {
                *t = d.clone();
                changed = true;
                continue;
            }
        }
        if t.usage_command.as_deref().map(str::trim).unwrap_or("").is_empty() {
            if let Some(d) = defaults.tools.iter().find(|d| d.id == t.id) {
                if d.usage_command.is_some() {
                    t.usage_command = d.usage_command.clone();
                    changed = true;
                }
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

/// トークン数を読みやすい短い表記（1.2K、3.4M など）にする
fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// ISO 8601 の先頭（`YYYY-MM-DDTHH:MM:SS`）から UNIX 秒を求める
fn parse_iso_secs(s: &str) -> Option<u64> {
    let b = s.trim().as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: i64 = s[5..7].parse().ok()?;
    let d: i64 = s[8..10].parse().ok()?;
    let hh: u64 = s[11..13].parse().ok()?;
    let mm: u64 = s[14..16].parse().ok()?;
    let ss: u64 = s[17..19].parse().ok()?;
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = (m + if m > 2 { -3 } else { 9 }) as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;
    if days < 0 {
        return None;
    }
    Some(days as u64 * 86400 + hh * 3600 + mm * 60 + ss)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// フォルダ以下の指定の拡張子のファイルを深さ上限つきで集める
fn walk_files(dir: &Path, depth: usize, ext: &str, out: &mut Vec<PathBuf>) {
    if depth == 0 || !dir.is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_files(&p, depth - 1, ext, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

/// トークン数の集計から (1行表示, ツールチップ用の詳細) を組み立てる
fn format_token_summary(today: u64, total: u64, rate_prefix: Option<&str>) -> (String, String) {
    let today_s = fmt_tokens(today);
    let total_s = fmt_tokens(total);
    let tok_part = tr!(
        "今日 {today_s} / 計 {total_s} tok",
        "Today {today_s} / Total {total_s} tok",
        "今日 {today_s} / 共 {total_s} tok"
    );
    let short = match rate_prefix.filter(|s| !s.is_empty()) {
        Some(r) if total > 0 => format!("{r} · {today_s} tok"),
        Some(r) => r.to_string(),
        None => tok_part.clone(),
    };
    let detail = match rate_prefix.filter(|s| !s.is_empty()) {
        Some(r) => format!("{r}\n{tok_part}"),
        None => tok_part,
    };
    (short, detail)
}

/// Codex のログの rate_limits の 1 枠（primary / secondary）。ts はその行の時刻（resets_in_seconds の基準）
fn codex_limit(w: &serde_json::Value, ts: Option<u64>) -> Option<crate::rate_limits::Limit> {
    let used = w.get("used_percent").and_then(|x| x.as_f64())?;
    let window_minutes = w.get("window_minutes").and_then(|x| x.as_u64()).unwrap_or(0);
    let resets_at = w
        .get("resets_at")
        .and_then(|x| x.as_u64())
        .or_else(|| Some(ts? + w.get("resets_in_seconds").and_then(|x| x.as_u64())?));
    Some(crate::rate_limits::Limit { window_minutes, used_percent: used, resets_at, label: None })
}

/// Codex の ~/.codex/sessions から最新のレート制限（枠ごとの使用率とリセット時刻）、その時刻、トークン数を読み取る
fn codex_local_stats() -> (Vec<crate::rate_limits::Limit>, Option<u64>, u64, u64) {
    let home = std::env::var("CODEX_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".codex")));
    let Some(base) = home else { return (Vec::new(), None, 0, 0) };
    let mut files = Vec::new();
    walk_files(&base.join("sessions"), 5, "jsonl", &mut files);
    files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    let now = now_secs();
    let mut today_tok = 0u64;
    let mut total_tok = 0u64;
    let mut latest: (Vec<crate::rate_limits::Limit>, Option<u64>) = (Vec::new(), None);
    for f in &files {
        let recent_file = std::fs::metadata(f)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
            .map(|d| d.as_secs() < 86400)
            .unwrap_or(false);
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let mut session_tok = 0u64;
        for line in text.lines() {
            if !line.contains("token_count") && !line.contains("rate_limits") && !line.contains("total_tokens") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let rl = v.get("rate_limits").or_else(|| v.pointer("/payload/rate_limits")).or_else(|| v.pointer("/info/rate_limits"));
            if let Some(rl) = rl {
                let ts = v.get("timestamp").and_then(|x| x.as_str()).and_then(parse_iso_secs);
                let limits: Vec<_> = ["primary", "secondary"].iter().filter_map(|k| codex_limit(rl.get(*k)?, ts)).collect();
                if !limits.is_empty() {
                    latest = (limits, ts.or(Some(now)));
                }
            }
            if let Some(t) = v
                .pointer("/payload/info/total_token_usage/total_tokens")
                .or_else(|| v.pointer("/info/total_token_usage/total_tokens"))
                .and_then(|x| x.as_u64())
            {
                session_tok = session_tok.max(t);
            }
        }
        total_tok = total_tok.saturating_add(session_tok);
        if recent_file {
            today_tok = today_tok.saturating_add(session_tok);
        }
    }
    (latest.0, latest.1, today_tok, total_tok)
}

/// (フィールド番号, varint の値, 長さつきの中身)
type PbField<'a> = (u64, Option<u64>, Option<&'a [u8]>);

/// protobuf のバイト列を (フィールド番号, varint の値, 長さつきの中身) の並びとして読む。壊れていればそこで止める
fn pb_fields(mut b: &[u8]) -> Vec<PbField<'_>> {
    fn varint(b: &mut &[u8]) -> Option<u64> {
        let mut x = 0u64;
        for shift in (0..64).step_by(7) {
            let (&c, rest) = b.split_first()?;
            *b = rest;
            x |= u64::from(c & 0x7f) << shift;
            if c & 0x80 == 0 {
                return Some(x);
            }
        }
        None
    }
    let mut out = Vec::new();
    while !b.is_empty() {
        let Some(tag) = varint(&mut b) else { break };
        let field = tag >> 3;
        match tag & 7 {
            0 => match varint(&mut b) {
                Some(v) => out.push((field, Some(v), None)),
                None => break,
            },
            1 | 5 => {
                let n = if tag & 7 == 1 { 8 } else { 4 };
                if b.len() < n {
                    break;
                }
                b = &b[n..];
            }
            2 => {
                let Some(len) = varint(&mut b).and_then(|l| usize::try_from(l).ok()) else { break };
                if b.len() < len {
                    break;
                }
                out.push((field, None, Some(&b[..len])));
                b = &b[len..];
            }
            _ => break,
        }
    }
    out
}

fn pb_msg(b: &[u8], field: u64) -> Option<&[u8]> {
    pb_fields(b).into_iter().find(|(f, _, s)| *f == field && s.is_some()).and_then(|(_, _, s)| s)
}

fn pb_uint(b: &[u8], field: u64) -> Option<u64> {
    pb_fields(b).into_iter().find(|(f, v, _)| *f == field && v.is_some()).and_then(|(_, v, _)| v)
}

/// Antigravity の 1 回の生成（gen_metadata の 1 行）から (トークン数, 時刻) を取り出す。
/// 1.4 が使用量（2: 入力、3: 出力、4: キャッシュ作成、5: キャッシュ読み込み）、1.9.4.1 が開始時刻（UNIX 秒）
fn antigravity_generation(data: &[u8]) -> Option<(u64, Option<u64>)> {
    let meta = pb_msg(data, 1)?;
    let usage = pb_msg(meta, 4)?;
    let tok = (2..=5).filter_map(|f| pb_uint(usage, f)).fold(0u64, u64::saturating_add);
    let ts = pb_msg(meta, 9).and_then(|m| pb_msg(m, 4)).and_then(|m| pb_uint(m, 1));
    Some((tok, ts))
}

/// Antigravity（CLI・IDE）の会話 DB（~/.gemini/antigravity*/conversations/*.db）から (直近 24 時間, 合計) のトークン数を集計する
fn antigravity_local_stats(now: u64) -> (u64, u64) {
    use rusqlite::{Connection, OpenFlags};
    let mut today = 0u64;
    let mut total = 0u64;
    let Some(home) = dirs::home_dir() else { return (0, 0) };
    for dir in ["antigravity-cli", "antigravity"] {
        let Ok(rd) = std::fs::read_dir(home.join(".gemini").join(dir).join("conversations")) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("db") {
                continue;
            }
            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let Ok(conn) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) else {
                continue;
            };
            let _ = conn.busy_timeout(std::time::Duration::from_millis(500));
            let Ok(mut stmt) = conn.prepare("SELECT data FROM gen_metadata") else { continue };
            let Ok(rows) = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0)) else { continue };
            for data in rows.flatten() {
                let Some((tok, ts)) = antigravity_generation(&data) else { continue };
                total = total.saturating_add(tok);
                if ts.or(mtime).is_some_and(|ts| now.saturating_sub(ts) < 86400) {
                    today = today.saturating_add(tok);
                }
            }
        }
    }
    (today, total)
}

/// 各 AI ツールがローカルに残すセッションログから Usage（トークン数・レート制限）を集計する
fn collect_local_usage(tool_id: &str) -> (String, String) {
    let now = now_secs();
    match tool_id {
        "claude-code" => {
            let mut dirs = Vec::new();
            if let Some(h) = dirs::home_dir() {
                dirs.push(h.join(".claude").join("projects"));
                dirs.push(h.join(".config").join("claude").join("projects"));
            }
            let mut files = Vec::new();
            for d in dirs {
                walk_files(&d, 4, "jsonl", &mut files);
            }
            let mut today = 0u64;
            let mut total = 0u64;
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                for line in text.lines() {
                    if !line.contains("\"usage\"") {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                    let Some(u) = v.pointer("/message/usage").or_else(|| v.get("usage")) else { continue };
                    let tok = u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0)
                        + u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0)
                        + u.get("cache_creation_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0)
                        + u.get("cache_read_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
                    total = total.saturating_add(tok);
                    let is_today = v
                        .get("timestamp")
                        .and_then(|x| x.as_str())
                        .and_then(parse_iso_secs)
                        .map(|ts| now.saturating_sub(ts) < 86400)
                        .unwrap_or(false);
                    if is_today {
                        today = today.saturating_add(tok);
                    }
                }
            }
            format_token_summary(today, total, None)
        }
        "antigravity" => {
            let (today, total) = antigravity_local_stats(now);
            format_token_summary(today, total, None)
        }
        "codex-cli" => {
            // 残り（rate_limits）は with_limits でバーとして出す
            let (_, _, today, total) = codex_local_stats();
            format_token_summary(today, total, None)
        }
        "gemini-cli" => {
            let mut files = Vec::new();
            if let Some(tmp) = dirs::home_dir().map(|h| h.join(".gemini").join("tmp")) {
                walk_files(&tmp, 4, "json", &mut files);
            }
            let mut today = 0u64;
            let mut total = 0u64;
            for f in files {
                let recent_file = std::fs::metadata(&f)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
                    .map(|d| d.as_secs() < 86400)
                    .unwrap_or(false);
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let Some(msgs) = v.get("messages").and_then(|x| x.as_array()) else { continue };
                for m in msgs {
                    let Some(t) = m.get("tokens") else { continue };
                    let tok = t.get("total").and_then(|x| x.as_u64()).unwrap_or_else(|| {
                        t.get("input").and_then(|x| x.as_u64()).unwrap_or(0)
                            + t.get("output").and_then(|x| x.as_u64()).unwrap_or(0)
                            + t.get("cached").and_then(|x| x.as_u64()).unwrap_or(0)
                    });
                    total = total.saturating_add(tok);
                    if recent_file {
                        today = today.saturating_add(tok);
                    }
                }
            }
            format_token_summary(today, total, None)
        }
        _ => (String::from("—"), String::new()),
    }
}

/// コマンドの標準出力（`ccusage --json` の JSON またはプレーンテキスト）を表示用に整える
fn format_usage_output(raw: &str, _tool_id: &str) -> (String, String) {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(totals) = v.get("totals") {
            let total_tokens = totals.get("totalTokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let total_cost = totals.get("totalCost").and_then(|x| x.as_f64());
            let last_day = v.get("daily").and_then(|d| d.as_array()).and_then(|a| a.last());
            let day_tokens = last_day.and_then(|d| d.get("totalTokens")).and_then(|x| x.as_u64()).unwrap_or(0);
            let day_cost = last_day.and_then(|d| d.get("totalCost")).and_then(|x| x.as_f64());
            let day_date = last_day.and_then(|d| d.get("date")).and_then(|x| x.as_str()).unwrap_or("");
            let short = match (day_cost, total_cost) {
                (Some(dc), Some(tc)) if tc > 0.0 => {
                    let dt = fmt_tokens(day_tokens);
                    tr!(
                        "${dc:.2} ({dt}) / 計 ${tc:.2}",
                        "${dc:.2} ({dt}) / Total ${tc:.2}",
                        "${dc:.2} ({dt}) / 共 ${tc:.2}"
                    )
                }
                _ => {
                    let dt = fmt_tokens(day_tokens);
                    let tt = fmt_tokens(total_tokens);
                    tr!("直近 {dt} / 計 {tt} tok", "Latest {dt} / Total {tt} tok", "最近 {dt} / 共 {tt} tok")
                }
            };
            let detail = match (day_cost, total_cost) {
                (Some(dc), Some(tc)) => format!(
                    "{}: ${dc:.2} ({} tok)\nTotal: ${tc:.2} ({} tok)",
                    if day_date.is_empty() { "Latest" } else { day_date },
                    fmt_tokens(day_tokens),
                    fmt_tokens(total_tokens)
                ),
                _ => format!(
                    "{}: {} tok\nTotal: {} tok",
                    if day_date.is_empty() { "Latest" } else { day_date },
                    fmt_tokens(day_tokens),
                    fmt_tokens(total_tokens)
                ),
            };
            return (short, detail);
        }
    }
    let one_line = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (one_line, trimmed.to_string())
}

/// `mark-shell --ai-usage <tool_id>` として呼ばれたときの標準出力
pub fn print_for_usage(tool_id: &str) {
    let (short, _) = collect_local_usage(tool_id);
    println!("{short}");
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
    tauri::async_runtime::spawn_blocking(move || with_limits(run_usage_command(tool_id, command, note, root, wsl)))
        .await
        .unwrap_or_else(|e| UsageResult::new(String::new(), String::new(), String::new(), Some(e.to_string())))
}

/// Usage 取得コマンドを実行して、使用量の表示を作る
/// WSL で Usage のコマンドを実行する前に、PATH から Windows 側のディレクトリ（/mnt/...）を外す。
/// WSL から Windows の実行ファイル（npx など）を呼ぶと、そのたびに wslhost.exe と Windows のプロセスに
/// 新しいコンソールが作られ、黒いウィンドウが一瞬表示されることがある（CREATE_NO_WINDOW は孫まで届かない）
const WSL_NO_INTEROP_PATH: &str =
    "p=; IFS=:; for d in $PATH; do case $d in /mnt/*) ;; *) p=${p:+$p:}$d ;; esac; done; unset IFS; PATH=$p; export PATH; ";

fn run_usage_command(
    tool_id: String,
    command: Option<String>,
    note: Option<String>,
    root: Option<PathBuf>,
    wsl: Option<std::sync::Arc<crate::wsl::WslRoot>>,
) -> UsageResult {
    let tool = load().into_iter().find(|t| t.id == tool_id);
    let raw_cmd = command
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| tool.as_ref().and_then(|t| t.usage_command.as_ref()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    let Some(raw_cmd) = raw_cmd else {
        return UsageResult::new(tool_id, String::new(), String::new(), None);
    };
    // `mark-shell --ai-usage <id>` は自分自身の集計処理を直接呼ぶ
    if let Some(rest) = raw_cmd.strip_prefix("mark-shell --ai-usage") {
        let target = rest.trim();
        let (output, detail) = collect_local_usage(if target.is_empty() { &tool_id } else { target });
        return UsageResult::new(tool_id, output, detail, None);
    }
    let is_default_cmd = serde_json::from_str::<ToolsFile>(DEFAULT_TOOLS)
        .ok()
        .and_then(|f| f.tools.into_iter().find(|d| d.id == tool_id))
        .and_then(|d| d.usage_command)
        .as_deref()
        == Some(raw_cmd.as_str());
    let is_cli = tool.as_ref().map(|t| t.kind == "cli").unwrap_or(true);
    // Windows（Mac）側で実行する。WSL のフォルダ（\\wsl.localhost\...）は cmd.exe がカレントにできないので、ホームで実行する
    let host_cmd = || {
        let vars = match &root {
            Some(r) => Vars::new(r, note.as_deref(), None),
            None => Vars { map: HashMap::new() },
        };
        let line = vars.expand(&raw_cmd);
        let cwd = root
            .clone()
            .filter(|p| wsl.is_none() && p.is_dir())
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
    };
    let res = match (&wsl, is_cli) {
        (Some(w), true) => {
            let mut map = HashMap::new();
            map.insert("root", w.linux_root.clone());
            if let Some(n) = note.as_deref() {
                map.insert("note", w.linux_path(n));
                map.insert("noteRelativePath", n.to_string());
                map.insert("noteDir", w.linux_path(crate::paths::parent_rel(n)));
            }
            let vars = Vars { map };
            let line = format!("{WSL_NO_INTEROP_PATH}{}", vars.expand(&raw_cmd));
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
            let res = run_command_timeout(c, std::time::Duration::from_secs(15));
            let ok = matches!(&res, Ok((s, out, err)) if s.success() && (!out.is_empty() || !err.is_empty()));
            // WSL に Node がなく、プリセットのコマンド（npx ccusage）が動かなければ、Windows 側で実行する
            // （これまで WSL から Windows の npx を呼んでいたのと同じ結果になる）
            if !ok && is_default_cmd {
                run_command_timeout(host_cmd(), std::time::Duration::from_secs(15))
            } else {
                res
            }
        }
        _ => run_command_timeout(host_cmd(), std::time::Duration::from_secs(15)),
    };
    match res {
        Ok((status, stdout, stderr)) => {
            let out = strip_ansi(&String::from_utf8_lossy(&stdout)).trim().to_string();
            let err = strip_ansi(&String::from_utf8_lossy(&stderr)).trim().to_string();
            if status.success() && (!out.is_empty() || !err.is_empty()) {
                let raw = if out.is_empty() { &err } else { &out };
                let (output, detail) = format_usage_output(raw, &tool_id);
                UsageResult::new(tool_id, output, detail, None)
            } else if is_default_cmd {
                // プリセットのコマンド（npx ccusage など）が使えない環境・オフライン時はローカルログの集計にフォールバックする
                let (output, detail) = collect_local_usage(&tool_id);
                UsageResult::new(tool_id, output, detail, None)
            } else {
                let msg = if !err.is_empty() {
                    err
                } else if !out.is_empty() {
                    out
                } else {
                    format!("exit {}", status.code().unwrap_or(-1))
                };
                UsageResult::new(tool_id, String::new(), String::new(), Some(msg))
            }
        }
        Err(_) if is_default_cmd => {
            let (output, detail) = collect_local_usage(&tool_id);
            UsageResult::new(tool_id, output, detail, None)
        }
        Err(e) => UsageResult::new(tool_id, String::new(), String::new(), Some(e)),
    }
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
    fn codex_rate_limits() {
        let rl: serde_json::Value = serde_json::from_str(
            r#"{"primary":{"used_percent":12.5,"window_minutes":300,"resets_in_seconds":600},
                "secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":99999}}"#,
        )
        .unwrap();
        let p = codex_limit(&rl["primary"], Some(1000)).unwrap();
        assert_eq!((p.window_minutes, p.used_percent, p.resets_at), (300, 12.5, Some(1600)));
        let s = codex_limit(&rl["secondary"], None).unwrap();
        assert_eq!((s.window_minutes, s.resets_at), (10080, Some(99999)));
        // 行の時刻がなければ resets_in_seconds からは求めない
        assert_eq!(codex_limit(&rl["primary"], None).unwrap().resets_at, None);
    }

    #[test]
    fn strip_ansi_and_usage_command() {
        assert_eq!(strip_ansi("\x1b[32m50%\x1b[0m ($2.10)"), "50% ($2.10)");
        let f: ToolsFile = serde_json::from_str(DEFAULT_TOOLS).unwrap();
        for t in &f.tools {
            assert!(t.usage_command.as_deref().is_some_and(|s| !s.is_empty()));
        }
        let ccusage_json = r#"{"daily":[{"date":"2026-11-18","totalTokens":142161,"totalCost":0.45}],"totals":{"totalTokens":1421610,"totalCost":12.30}}"#;
        let (short, detail) = format_usage_output(ccusage_json, "claude-code");
        assert!(short.contains("$0.45"));
        assert!(short.contains("142.2K"));
        assert!(short.contains("$12.30"));
        assert!(detail.contains("2026-11-18"));
    }

    fn pb_varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }

    fn pb_u(field: u64, v: u64) -> Vec<u8> {
        let mut o = Vec::new();
        pb_varint(field << 3, &mut o);
        pb_varint(v, &mut o);
        o
    }

    fn pb_m(field: u64, body: &[u8]) -> Vec<u8> {
        let mut o = Vec::new();
        pb_varint((field << 3) | 2, &mut o);
        pb_varint(body.len() as u64, &mut o);
        o.extend_from_slice(body);
        o
    }

    #[test]
    fn antigravity_generation_tokens() {
        // 実際の gen_metadata と同じ形（1.4 に使用量、1.9.4.1 に時刻、ほかのフィールドも混ざる）
        let usage = [pb_u(1, 1020), pb_u(2, 1677), pb_u(3, 202), pb_u(5, 16264), pb_u(6, 24), pb_u(9, 126), pb_m(11, b"abc")].concat();
        let time = pb_m(4, &[pb_u(1, 1_781_180_185), pb_u(2, 980_711_100)].concat());
        let meta = [pb_u(3, 1020), pb_m(4, &usage), pb_m(7, &pb_m(1, b"auto")), pb_m(9, &time)].concat();
        let data = [pb_m(2, b""), pb_m(1, &meta), pb_m(4, b"a3ae5d3f")].concat();
        assert_eq!(antigravity_generation(&data), Some((1677 + 202 + 16264, Some(1_781_180_185))));
        assert_eq!(antigravity_generation(b"\xff\xff"), None);
    }

    #[test]
    fn claude_usage_output() {
        use chrono::TimeZone;
        let jst = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
        let now = jst.with_ymd_and_hms(2026, 11, 18, 12, 20, 0).unwrap();
        let ts = |m, d, h, mi| jst.with_ymd_and_hms(2026, m, d, h, mi, 0).unwrap().timestamp() as u64;
        let out = "You are currently using your subscription to power your Claude Code usage\n\n\
                   Current session: 7% used · resets Nov 18, 3pm (Asia/Tokyo)\n\
                   Current week (all models): 8.5% used · resets Nov 23, 8am (Asia/Tokyo)\n\
                   Current week (Fable): 0% used · resets Nov 23, 12:30am (Asia/Tokyo)\n\n\
                   Last 24h · 138 requests · 2 sessions\n  47%  Start fresh when you switch tasks · run /clear\n";
        let l = parse_claude_usage(out, now);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].window_minutes, l[0].used_percent, l[0].resets_at, l[0].label.as_deref()), (300, 7.0, Some(ts(11, 18, 15, 0)), None));
        assert_eq!((l[1].window_minutes, l[1].used_percent, l[1].resets_at, l[1].label.as_deref()), (10080, 8.5, Some(ts(11, 23, 8, 0)), None));
        assert_eq!((l[2].resets_at, l[2].label.as_deref()), (Some(ts(11, 23, 0, 30)), Some("Fable")));
        // 日付のない時刻（過ぎていれば翌日）と、年明けの日付（翌年）
        assert_eq!(parse_claude_reset("11am (Asia/Tokyo)", &now), Some(ts(11, 19, 11, 0)));
        assert_eq!(parse_claude_reset("12pm", &now), Some(ts(11, 18, 12, 0) + 86400));
        let dec = jst.with_ymd_and_hms(2026, 12, 30, 10, 0, 0).unwrap();
        assert_eq!(
            parse_claude_reset("Jan 2, 9am", &dec),
            Some(jst.with_ymd_and_hms(2027, 1, 2, 9, 0, 0).unwrap().timestamp() as u64)
        );
        assert!(parse_claude_usage("Error: not logged in", now).is_empty());
    }

    #[test]
    fn agy_usage_output() {
        let out = "Gemini Models\tWeekly Limit Remaining\t100%\t2026-11-25T02:50:28Z\n\
                   Gemini Models\tFive Hour Limit Remaining\t75%\t2026-11-18T07:50:28Z\n\
                   Claude and GPT models\tFive Hour Limit Remaining\t12%\t2026-11-18T07:50:28Z\n\
                   Gemini 4 Argon\tGemini 4 Argon Weekly Limit\t99%\t2026-11-25T02:49:38Z\n\
                   \nsomething else\n";
        let l = parse_agy_usage(out);
        assert_eq!(l.len(), 4);
        assert_eq!(l[0].window_minutes, crate::rate_limits::SEVEN_DAYS);
        assert_eq!(l[0].label.as_deref(), Some("Gemini"));
        assert_eq!(l[1].window_minutes, crate::rate_limits::FIVE_HOURS);
        assert_eq!(l[1].used_percent, 25.0);
        assert_eq!(l[1].resets_at, parse_iso_secs("2026-11-18T07:50:28Z"));
        assert_eq!(l[2].label.as_deref(), Some("Claude/GPT"));
        assert_eq!(l[2].used_percent, 88.0);
        assert_eq!(l[3].label.as_deref(), Some("Gemini 4 Argon"));
        assert_eq!(l[3].window_minutes, crate::rate_limits::SEVEN_DAYS);
    }
}
