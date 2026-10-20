//! ConPTY によるターミナル。シェルや AI CLI を疑似端末で動かし、入出力を UI と中継する

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::Ordering;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::ai_context;
use crate::ai_tools;
use crate::config::local_dir;
use crate::paths::{parent_rel, resolve};
use crate::shells;
use crate::state::AppState;
use crate::wsl::{self, WslRoot};

pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnRequest {
    pub shell_id: Option<String>,
    pub tool_id: Option<String>,
    pub action_id: Option<String>,
    /// 作業ディレクトリ（ルートからの相対パス）
    pub cwd: Option<String>,
    /// 開いているファイル（ルートからの相対パス）
    pub note: Option<String>,
    /// エディタの選択範囲
    pub selection: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Serialize)]
pub struct SpawnResult {
    id: u32,
    title: String,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PtyEvent {
    Data { data: String },
    Exit { code: Option<u32> },
}

fn write_selection_file(text: &str) -> Result<std::path::PathBuf, String> {
    let dir = local_dir().join("tmp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("selection-{stamp}.md"));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// WSL のフォルダで CLI の AI ツールを WSL の中で起動する。
/// ログインシェルで起動して PATH などをそろえ、ツールが終わったらそのままシェルに戻る
fn wsl_tool_command(w: &WslRoot, tool: &ai_tools::ToolDef, action: Option<&str>, note: Option<&str>, selection_file: Option<&Path>) -> Result<CommandBuilder, String> {
    let exe = ai_tools::find_in_wsl(tool, w)
        .ok_or_else(|| format!("{} が WSL（{}）に見つかりません。WSL の中にインストールされているか確認してください", tool.name, w.distro))?;
    let mut map = HashMap::new();
    map.insert("root", w.linux_root.clone());
    if let Some(n) = note {
        map.insert("note", w.linux_path(n));
        map.insert("noteRelativePath", n.to_string());
        map.insert("noteDir", w.linux_path(parent_rel(n)));
    }
    if let Some(s) = selection_file {
        map.insert("selectionFile", w.to_linux(s)?);
    }
    let is_claude = tool.id == "claude-code";
    if is_claude {
        map.insert("claudeSettings", ai_context::claude_settings_file_wsl(w)?);
    }
    let vars = ai_tools::Vars { map };
    let mut args = ai_tools::args_for(tool, action, &vars)?;
    if is_claude && !args.iter().any(|a| a == "--settings") {
        args.splice(0..0, ["--settings".to_string(), vars.map["claudeSettings"].clone()]);
    }
    let cwd = tool.launch.cwd.as_deref().map(|c| vars.expand(c)).filter(|c| c.starts_with('/')).unwrap_or_else(|| w.linux_root.clone());
    let mut c = CommandBuilder::new(wsl::wsl_exe());
    c.args(["-d", &w.distro, "--cd", &cwd, "--exec", "sh", "-lc", "\"$@\"; exec \"${SHELL:-sh}\" -l", "sh", &exe]);
    c.args(&args);
    Ok(c)
}

#[tauri::command]
pub fn pty_spawn(app: AppHandle, state: State<'_, AppState>, req: SpawnRequest, on_event: Channel<PtyEvent>) -> Result<SpawnResult, String> {
    let root = state.root()?;
    let wsl = state.wsl();
    let (cwd, cwd_rel) = match req.cwd.as_deref() {
        Some(c) => (resolve(&root, c)?, c.to_string()),
        None => (root.clone(), String::new()),
    };
    let (cwd, cwd_rel) = if cwd.is_dir() { (cwd, cwd_rel) } else { (root.clone(), String::new()) };
    let note_abs = req.note.as_deref().and_then(|n| resolve(&root, n).ok());
    let selection_file = match req.selection.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => Some(write_selection_file(s)?),
        None => None,
    };

    // in_wsl: WSL のフォルダを WSL の中で扱う（環境変数を Linux のパスで渡す）
    let (mut cmd, title, in_wsl) = if let Some(tool_id) = req.tool_id.as_deref() {
        let tool = ai_tools::get(tool_id)?;
        match &wsl {
            Some(w) if tool.kind == "cli" => {
                let c = wsl_tool_command(w, &tool, req.action_id.as_deref(), req.note.as_deref(), selection_file.as_deref())?;
                (c, format!("{} (WSL)", tool.name), true)
            }
            _ => {
                let (tool, path) = ai_tools::find(tool_id)?;
                let vars = ai_tools::Vars::new(&root, req.note.as_deref(), selection_file.as_deref());
                let mut args = ai_tools::args_for(&tool, req.action_id.as_deref(), &vars)?;
                // Claude Code には開いているファイルを伝えるフックを渡す。
                // 以前の既定値のまま保存された ai-tools.json でも効くよう、定義に --settings がなければ足す
                if tool.id == "claude-code" && !args.iter().any(|a| a == "--settings") {
                    if let Ok(p) = ai_context::claude_settings_file() {
                        args.splice(0..0, ["--settings".to_string(), p.to_string_lossy().into_owned()]);
                    }
                }
                let tool_cwd = tool.launch.cwd.as_deref().map(|c| std::path::PathBuf::from(vars.expand(c))).filter(|p| p.is_dir());
                let mut cmd = if tool.launch.shell.as_deref() == Some("none") {
                    let mut c = CommandBuilder::new(&path);
                    c.args(&args);
                    c
                } else {
                    // PowerShell 経由で起動する。.cmd / .ps1 の起動スクリプトもそのまま動き、
                    // 終了後もシェルが残る。引数は単一引用符で囲むので、そのまま渡る
                    let ps = shells::powershell().ok_or("PowerShell が見つかりません")?;
                    let line = std::iter::once(format!("& {}", ai_tools::ps_quote(&path.to_string_lossy())))
                        .chain(args.iter().map(|a| ai_tools::ps_quote(a)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut c = CommandBuilder::new(&ps.program);
                    c.args(["-NoLogo", "-NoExit", "-Command", &format!("{}; {line}", PS_INIT)]);
                    c
                };
                cmd.cwd(tool_cwd.unwrap_or_else(|| cwd.clone()));
                (cmd, tool.name, false)
            }
        }
    } else {
        let sh = shells::find(req.shell_id.as_deref()).ok_or("使えるシェルが見つかりません")?;
        let mut c = CommandBuilder::new(&sh.program);
        c.args(&sh.args);
        // 開いている WSL のフォルダと同じディストリビューションのシェルなら、Linux のパスに移動する
        let same = wsl.as_ref().filter(|w| sh.wsl && sh.id == format!("wsl:{}", w.distro));
        if let Some(w) = same {
            c.args(["--cd", &w.linux_path(&cwd_rel)]);
        } else if sh.wsl {
            c.args(["--cd", &cwd.to_string_lossy()]);
        }
        c.cwd(&cwd);
        (c, sh.name.clone(), same.is_some())
    };

    match (&wsl, in_wsl) {
        (Some(w), true) => {
            cmd.env("MARKSHELL_ROOT", &w.linux_root);
            if let Some(n) = req.note.as_deref() {
                cmd.env("MARKSHELL_NOTE", w.linux_path(n));
            }
            cmd.env("WSLENV", "MARKSHELL_ROOT:MARKSHELL_NOTE:MARKSHELL_SELECTION_FILE/p:MARKSHELL_CONTEXT_FILE/p");
        }
        _ => {
            cmd.env("MARKSHELL_ROOT", root.to_string_lossy().as_ref());
            if let Some(n) = &note_abs {
                cmd.env("MARKSHELL_NOTE", n.to_string_lossy().as_ref());
            }
            cmd.env("WSLENV", "MARKSHELL_ROOT/p:MARKSHELL_NOTE/p:MARKSHELL_SELECTION_FILE/p:MARKSHELL_CONTEXT_FILE/p");
        }
    }
    if let Some(s) = &selection_file {
        cmd.env("MARKSHELL_SELECTION_FILE", s.to_string_lossy().as_ref());
    }
    cmd.env("MARKSHELL_CONTEXT_FILE", ai_context::context_file().to_string_lossy().as_ref());
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "mark-shell");

    let pair = native_pty_system()
        .openpty(PtySize { rows: req.rows.max(2), cols: req.cols.max(10), pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("起動できません: {e}"))?;
    drop(pair.slave);
    let killer = child.clone_killer();
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let id = state.next_pty.fetch_add(1, Ordering::SeqCst);
    state.ptys.lock().unwrap().insert(id, PtySession { writer, master: pair.master, killer });

    // 出力の中継。UTF-8 の途中で切れたバイトは次の読み込みまで持ち越す
    let ch = on_event.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    let valid = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(e) if e.error_len().is_none() => e.valid_up_to(),
                        Err(_) => {
                            let s = String::from_utf8_lossy(&pending).into_owned();
                            pending.clear();
                            let _ = ch.send(PtyEvent::Data { data: s });
                            continue;
                        }
                    };
                    if valid > 0 {
                        let s = String::from_utf8(pending.drain(..valid).collect()).unwrap_or_default();
                        if ch.send(PtyEvent::Data { data: s }).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // 終了待ち。終わったら疑似端末を閉じて読み込みスレッドも終わらせる
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|s| s.exit_code());
        let state = app.state::<AppState>();
        state.ptys.lock().unwrap().remove(&id);
        let _ = on_event.send(PtyEvent::Exit { code });
        if let Some(f) = selection_file {
            // AI ツールが読み終わるまで少し残しておく
            std::thread::sleep(std::time::Duration::from_secs(5));
            let _ = std::fs::remove_file(f);
        }
    });

    Ok(SpawnResult { id, title })
}

const PS_INIT: &str = "[Console]::OutputEncoding=[Console]::InputEncoding=[System.Text.Encoding]::UTF8";

#[tauri::command]
pub fn pty_write(state: State<'_, AppState>, id: u32, data: String) -> Result<(), String> {
    let mut ptys = state.ptys.lock().unwrap();
    let s = ptys.get_mut(&id).ok_or("ターミナルは終了しています")?;
    s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    s.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(state: State<'_, AppState>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let ptys = state.ptys.lock().unwrap();
    if let Some(s) = ptys.get(&id) {
        s.master
            .resize(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn pty_kill(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    if let Some(mut s) = state.ptys.lock().unwrap().remove(&id) {
        let _ = s.killer.kill();
    }
    Ok(())
}
