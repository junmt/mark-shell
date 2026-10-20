//! AI ツールに「いま開いているファイル」を伝える仕組み。
//! 画面側が開いているファイルと選択範囲をプロセスごとの文脈ファイルに書き、
//! Claude Code の UserPromptSubmit フックから `mark-shell.exe --ai-context` を呼んで、
//! 送信のたびにその内容を会話の文脈に足す。タブを切り替えても次の質問から追従する。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::local_dir;
use crate::paths::resolve;
use crate::state::AppState;

/// フックに渡す選択範囲の上限（文字数）
const MAX_SELECTION_CHARS: usize = 10_000;

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub from_line: u32,
    pub to_line: u32,
    pub text: String,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Context {
    root: String,
    /// ルートからの相対パス
    note: Option<String>,
    note_abs: Option<String>,
    selection: Option<Selection>,
}

/// このプロセス（ウィンドウ）の文脈ファイル。シェルには MARKSHELL_CONTEXT_FILE で渡す
pub fn context_file() -> PathBuf {
    local_dir().join("tmp").join(format!("context-{}.json", std::process::id()))
}

/// Claude Code の `--settings` に渡すフック定義を書き出し、そのパスを返す。
/// 実行ファイルの場所は開発版とリリース版で違うので、起動のたびに書き直す
pub fn claude_settings_file() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // args を指定するとシェルを通さずに直接起動される（Git Bash の有無や文字コードに左右されない）
    let settings = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{
                "hooks": [{
                    "type": "command",
                    "command": exe.to_string_lossy(),
                    "args": ["--ai-context"],
                    "timeout": 10
                }]
            }]
        }
    });
    let path = local_dir().join("claude-settings.json");
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(local_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// WSL の中で動く Claude Code 用のフック定義。フックは WSL から Windows の mark-shell.exe を呼ぶ
/// （WSL は /mnt/c/... の Windows の実行ファイルを直接起動できる）。戻り値は WSL から見たパス
pub fn claude_settings_file_wsl(wsl: &crate::wsl::WslRoot) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let settings = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{
                "hooks": [{
                    "type": "command",
                    "command": wsl.to_linux(&exe)?,
                    "args": ["--ai-context"],
                    "timeout": 10
                }]
            }]
        }
    });
    let path = local_dir().join("claude-settings-wsl.json");
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(local_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    wsl.to_linux(&path)
}

/// 画面側から、開いているファイルと選択範囲が変わるたびに呼ばれる
#[tauri::command]
pub fn set_ai_context(state: State<'_, AppState>, note: Option<String>, selection: Option<Selection>) -> Result<(), String> {
    let root = state.root()?;
    // WSL のフォルダでは、WSL の中で動く AI ツールに合わせて Linux のパスで伝える
    let note_abs = match (state.wsl(), note.as_deref()) {
        (Some(w), Some(n)) => Some(PathBuf::from(w.linux_path(n))),
        (None, Some(n)) => resolve(&root, n).ok(),
        _ => None,
    };
    let selection = selection.filter(|s| !s.text.is_empty()).map(|mut s| {
        if let Some((i, _)) = s.text.char_indices().nth(MAX_SELECTION_CHARS) {
            s.text.truncate(i);
            s.text.push_str("\n…（以下省略）");
        }
        s
    });
    let ctx = Context {
        root: root.to_string_lossy().into_owned(),
        note,
        note_abs: note_abs.map(|p| p.to_string_lossy().into_owned()),
        selection,
    };
    let path = context_file();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    // フックが書きかけを読まないよう、別名で書いてから置き換える
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&ctx).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// 終了時に文脈ファイルを消す
pub fn remove_context_file() {
    let _ = std::fs::remove_file(context_file());
}

fn render(ctx: &Context) -> String {
    let Some(note) = &ctx.note else {
        return "mark-shell（ユーザーが使っている Markdown エディタ）では、いまファイルを開いていません。".into();
    };
    let mut out = format!(
        "ユーザーは mark-shell（Markdown エディタ）で {note}（{}）を開いています。\
         「このファイル」「この文書」などと言われたら、このファイルのことです。必要なら読んでから答えてください。",
        ctx.note_abs.as_deref().unwrap_or(note)
    );
    if let Some(s) = &ctx.selection {
        let lines = if s.from_line == s.to_line { format!("{} 行目", s.from_line) } else { format!("{}〜{} 行目", s.from_line, s.to_line) };
        out.push_str(&format!(
            "\nエディタでは {lines} を選択しています。「ここ」「選択した部分」などと言われたら、この部分のことです。\n<selection>\n{}\n</selection>",
            s.text
        ));
    }
    out
}

/// `mark-shell.exe --ai-context` として呼ばれたときの処理。標準出力が Claude の文脈に足される。
/// 文脈ファイルがなければ何も出さない（フックの失敗で質問を止めない）
pub fn print_for_hook() {
    use std::io::Write;
    let Some(path) = std::env::var_os("MARKSHELL_CONTEXT_FILE") else { return };
    let Ok(bytes) = std::fs::read(path) else { return };
    let Ok(ctx) = serde_json::from_slice::<Context>(&bytes) else { return };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(render(&ctx).as_bytes());
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_note_and_selection() {
        let ctx = Context {
            root: r"C:\Notes".into(),
            note: Some("a/b.md".into()),
            note_abs: Some(r"C:\Notes\a\b.md".into()),
            selection: Some(Selection { from_line: 3, to_line: 5, text: "選択".into() }),
        };
        let s = render(&ctx);
        assert!(s.contains(r"a/b.md（C:\Notes\a\b.md）"));
        assert!(s.contains("3〜5 行目"));
        assert!(s.contains("<selection>\n選択\n</selection>"));
    }

    #[test]
    fn render_without_note() {
        assert!(render(&Context::default()).contains("開いていません"));
    }
}
