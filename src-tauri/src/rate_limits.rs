//! AI ツールの「あとどれくらい使えるか」（レート制限の使用率とリセット時刻）。
//!
//! - Claude Code: mark-shell が起動する Claude Code の `--settings` にステータスラインとして
//!   `mark-shell.exe --statusline` を登録する。Claude Code はステータスラインのコマンドへ
//!   `rate_limits`（5 時間・7 日の使用率とリセット時刻）を渡すので、それを
//!   `%LOCALAPPDATA%\mark-shell\claude-rate-limits.json` に記録し、AI Usage の表示で読む。
//!   ユーザーが自分でステータスラインを設定しているときは、上書きしないよう登録しない。
//! - Codex: セッションログ（~/.codex/sessions）の `rate_limits` を読む（ai_tools.rs）

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use markshell_core::i18n::{self, Locale};

use crate::config::local_dir;

/// 1 つの制限の枠（5 時間、1 週間など）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    /// 枠の長さ（分）。5 時間なら 300、1 週間なら 10080
    pub window_minutes: u64,
    /// 使った割合（0〜100。超えると 100 より大きいこともある）
    pub used_percent: f64,
    /// 枠がリセットされる時刻（UNIX 秒）
    pub resets_at: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
    /// 記録した時刻（UNIX 秒）
    pub updated_at: u64,
    pub limits: Vec<Limit>,
}

pub const FIVE_HOURS: u64 = 300;
pub const SEVEN_DAYS: u64 = 10080;

fn record_path() -> PathBuf {
    local_dir().join("claude-rate-limits.json")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// ステータスラインの入力（JSON）の `rate_limits` から枠を取り出す
pub fn parse_statusline(input: &Value) -> Vec<Limit> {
    let Some(rl) = input.get("rate_limits") else { return Vec::new() };
    [("five_hour", FIVE_HOURS), ("seven_day", SEVEN_DAYS)]
        .into_iter()
        .filter_map(|(key, minutes)| {
            let w = rl.get(key)?;
            Some(Limit {
                window_minutes: minutes,
                used_percent: w.get("used_percentage")?.as_f64()?,
                resets_at: w.get("resets_at").and_then(Value::as_u64),
            })
        })
        .collect()
}

/// 記録した Claude Code の残量を読む
pub fn read_claude() -> Option<Recorded> {
    serde_json::from_slice(&std::fs::read(record_path()).ok()?).ok()
}

/// 残り時間を短く（2時間14分、3日 など）
fn fmt_left(secs: u64, l: Locale) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    match l {
        Locale::Ja | Locale::Zh => {
            let (dd, hh, mm) = if l == Locale::Ja { ("日", "時間", "分") } else { ("天", "小时", "分钟") };
            if d > 0 {
                format!("{d}{dd}{h}{hh}")
            } else if h > 0 {
                format!("{h}{hh}{m}{mm}")
            } else {
                format!("{m}{mm}")
            }
        }
        Locale::En => {
            if d > 0 {
                format!("{d}d{h}h")
            } else if h > 0 {
                format!("{h}h{m}m")
            } else {
                format!("{m}m")
            }
        }
    }
}

/// ステータスラインに出す 1 行（モデル名・コンテキスト・残量）
fn render_statusline(input: &Value, limits: &[Limit], now: u64, l: Locale) -> String {
    let mut parts = Vec::new();
    if let Some(m) = input.pointer("/model/display_name").and_then(Value::as_str) {
        parts.push(m.to_string());
    }
    if let Some(p) = input.pointer("/context_window/used_percentage").and_then(Value::as_f64) {
        parts.push(format!("ctx {}%", p.round()));
    }
    for lim in limits {
        let name = if lim.window_minutes == FIVE_HOURS { "5h" } else { "7d" };
        // 画面（Math.round）と同じく 0.5 は切り上げる
        let left = (100.0 - lim.used_percent).clamp(0.0, 100.0).round();
        let reset = lim.resets_at.filter(|&t| t > now).map(|t| fmt_left(t - now, l));
        parts.push(match (l, reset) {
            (Locale::Ja, Some(r)) => format!("{name} 残り {left:.0}%（{r}後に回復）"),
            (Locale::Ja, None) => format!("{name} 残り {left:.0}%"),
            (Locale::Zh, Some(r)) => format!("{name} 剩余 {left:.0}%（{r}后恢复）"),
            (Locale::Zh, None) => format!("{name} 剩余 {left:.0}%"),
            (Locale::En, Some(r)) => format!("{name} {left:.0}% left (resets in {r})"),
            (Locale::En, None) => format!("{name} {left:.0}% left"),
        });
    }
    parts.join(" · ")
}

/// `mark-shell.exe --statusline`（Claude Code のステータスライン）の処理。
/// 標準入力の `rate_limits` を記録し、残量を 1 行で出す。失敗しても Claude Code の動作は止めない
pub fn print_for_statusline() {
    use std::io::{Read, Write};
    let mut buf = String::new();
    let _ = std::io::stdin().take(1024 * 1024).read_to_string(&mut buf);
    let Ok(input) = serde_json::from_str::<Value>(&buf) else { return };
    let limits = parse_statusline(&input);
    let now = now_secs();
    if !limits.is_empty() {
        let rec = Recorded { updated_at: now, limits: limits.clone() };
        if let Ok(bytes) = serde_json::to_vec(&rec) {
            let _ = std::fs::create_dir_all(local_dir());
            let _ = markshell_core::textio::write_atomic(&record_path(), &bytes);
        }
    }
    // 言語はアプリと合わせる（ターミナルに渡した文脈ファイルに書いてある）
    let l = crate::ai_context::context_locale().unwrap_or_else(i18n::locale);
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(render_statusline(&input, &limits, now, l).as_bytes());
    let _ = stdout.flush();
}

/// settings.json の文字列にステータスラインの設定があるか
fn has_statusline(settings_json: &str) -> bool {
    serde_json::from_str::<Value>(settings_json).ok().and_then(|v| v.get("statusLine").cloned()).is_some_and(|s| !s.is_null())
}

/// Windows の ~/.claude/settings.json にユーザー自身のステータスラインがあるか
pub fn user_has_statusline() -> bool {
    let Some(home) = dirs::home_dir() else { return false };
    std::fs::read_to_string(home.join(".claude").join("settings.json")).is_ok_and(|s| has_statusline(&s))
}

/// WSL の ~/.claude/settings.json にユーザー自身のステータスラインがあるか（ディストリビューションごとに 1 度だけ調べる）
pub fn user_has_statusline_wsl(distro: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(&v) = cache.lock().unwrap().get(distro) {
        return v;
    }
    let mut c = std::process::Command::new(crate::wsl::wsl_exe());
    c.args(["-d", distro, "--exec", "sh", "-c", "cat ~/.claude/settings.json 2>/dev/null"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let v = c.output().ok().is_some_and(|o| has_statusline(&String::from_utf8_lossy(&o.stdout)));
    cache.lock().unwrap().insert(distro.to_string(), v);
    v
}

/// Windows の Claude Code に登録するステータスラインのコマンド。
/// Claude Code は Git Bash（なければ PowerShell）で実行するので、区切りは / にし、空白があるときだけ " で囲む
pub fn statusline_command(exe: &std::path::Path) -> String {
    let p = exe.to_string_lossy().replace('\\', "/");
    if p.contains(' ') {
        format!("\"{p}\" --statusline")
    } else {
        format!("{p} --statusline")
    }
}

/// WSL の Claude Code に登録するステータスラインのコマンド（sh で実行される）
pub fn statusline_command_wsl(linux_exe: &str) -> String {
    format!("'{}' --statusline", linux_exe.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rate_limits() {
        let input: Value = serde_json::from_str(
            r#"{"model":{"display_name":"Opus"},"context_window":{"used_percentage":34.2},
                "rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1000},"seven_day":{"used_percentage":41.2,"resets_at":90000}}}"#,
        )
        .unwrap();
        let limits = parse_statusline(&input);
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0], Limit { window_minutes: FIVE_HOURS, used_percent: 23.5, resets_at: Some(1000) });
        assert_eq!(limits[1].window_minutes, SEVEN_DAYS);
        let s = render_statusline(&input, &limits, 0, Locale::Ja);
        assert_eq!(s, "Opus · ctx 34% · 5h 残り 77%（16分後に回復） · 7d 残り 59%（1日1時間後に回復）");
        let s = render_statusline(&input, &limits, 0, Locale::En);
        assert!(s.contains("5h 77% left (resets in 16m)"));
        // rate_limits がない（API キーで使っているなど）
        assert!(parse_statusline(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn detect_user_statusline() {
        assert!(has_statusline(r#"{"statusLine":{"type":"command","command":"x"}}"#));
        assert!(!has_statusline(r#"{"permissions":{}}"#));
        assert!(!has_statusline(""));
    }

    #[test]
    fn commands() {
        assert_eq!(statusline_command(std::path::Path::new(r"C:\Users\a\mark-shell.exe")), "C:/Users/a/mark-shell.exe --statusline");
        assert_eq!(statusline_command(std::path::Path::new(r"C:\Program Files\m\mark-shell.exe")), "\"C:/Program Files/m/mark-shell.exe\" --statusline");
        assert_eq!(statusline_command_wsl("/mnt/c/a b/mark-shell.exe"), "'/mnt/c/a b/mark-shell.exe' --statusline");
    }
}
