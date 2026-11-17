//! コマンド `mark-shell`。シェルから `mark-shell ~/notes` のように起動できるようにする。
//!
//! - Windows: `%LOCALAPPDATA%\mark-shell\bin` に起動用のスクリプトを置き、そのフォルダをユーザーの PATH（HKCU）に加える。
//!   WSL は既定で Windows の PATH を引き継ぐので、WSL の中からも `mark-shell` が見つかる（VS Code の `code` と同じ仕組み）。
//!   WSL 用のスクリプトは Linux のパスを `wslpath -w` で `\\wsl.localhost\...` に直してから mark-shell.exe に渡す。
//! - Mac: `~/.local/bin/mark-shell` に起動用のスクリプトを置き、`~/.zprofile` と `~/.zshrc` で `~/.local/bin` を PATH に加える。

use std::path::{Path, PathBuf};

#[cfg(windows)]
use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
#[cfg(windows)]
use winreg::{RegKey, RegValue};

#[cfg(windows)]
fn bin_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir().ok_or("LOCALAPPDATA が見つかりません")?;
    Ok(base.join("mark-shell").join("bin"))
}

#[cfg(not(windows))]
fn bin_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("HOME が見つかりません")?;
    Ok(home.join(".local").join("bin"))
}

/// sh の単一引用符で囲む
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// WSL のシェルから使うスクリプト（改行は LF）
#[cfg_attr(not(windows), allow(dead_code))]
fn sh_script(exe: &str) -> String {
    format!(
        r#"#!/bin/sh
# mark-shell を起動する（WSL 用）。mark-shell が起動のたびに書き直すので編集しないでください。
# 使い方: mark-shell [フォルダかファイル]
exe="$(wslpath -u {exe})" || exit 1
if [ $# -gt 0 ]; then
  if [ ! -e "$1" ]; then
    echo "mark-shell: $1: No such file or directory" >&2
    exit 1
  fi
  win="$(wslpath -w "$(realpath -- "$1")")" || exit 1
  set -- "$win"
fi
# 端末を閉じてもアプリが終わらないよう、端末から切り離して起動する
if command -v setsid >/dev/null 2>&1; then
  setsid "$exe" "$@" </dev/null >/dev/null 2>&1 &
else
  nohup "$exe" "$@" </dev/null >/dev/null 2>&1 &
fi
# Windows 側でアプリが起動する前にシェルが終わると（wsl.exe -e mark-shell . など）、起動が取り消されることがあるので少し待つ
sleep 0.5
"#,
        exe = sh_quote(exe)
    )
}

/// Mac のシェルから使うスクリプト（改行は LF）
#[cfg_attr(windows, allow(dead_code))]
fn mac_script(exe: &str) -> String {
    format!(
        r#"#!/bin/sh
# mark-shell を起動する。mark-shell が起動のたびに書き直すので編集しないでください。
# 使い方: mark-shell [フォルダかファイル]
exe={exe}
if [ $# -gt 0 ]; then
  if [ ! -e "$1" ]; then
    echo "mark-shell: $1: No such file or directory" >&2
    exit 1
  fi
  if [ -d "$1" ]; then
    target="$(cd -- "$1" && pwd)" || exit 1
  else
    target="$(cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")" || exit 1
  fi
  set -- "$target"
fi
nohup "$exe" "$@" </dev/null >/dev/null 2>&1 &
"#,
        exe = sh_quote(exe)
    )
}

/// 文字列を cmd の中で、日本語などを含まない形にする（%LOCALAPPDATA% などに置き換える）。できなければ None
#[cfg_attr(not(windows), allow(dead_code))]
fn ascii_for_cmd(path: &str, env: &[(&str, Option<String>)]) -> Option<String> {
    if path.is_ascii() {
        return Some(path.to_string());
    }
    for (name, value) in env {
        let Some(value) = value.as_deref().map(|v| v.trim_end_matches('\\')) else { continue };
        if value.is_empty() || path.len() <= value.len() || !path.is_char_boundary(value.len()) {
            continue;
        }
        let (head, rest) = path.split_at(value.len());
        if head.eq_ignore_ascii_case(value) && rest.starts_with('\\') && rest.is_ascii() {
            return Some(format!("%{name}%{rest}"));
        }
    }
    None
}

/// cmd / PowerShell から使うスクリプト。cmd は .cmd をコンソールのコードページ（日本語版なら 932）で読むので、
/// UTF-8 の日本語を書くと文字化けする。パスは %LOCALAPPDATA% などに置き換えて ASCII だけにし、
/// それでも日本語が残るときは、その間だけコードページを UTF-8 にする
#[cfg_attr(not(windows), allow(dead_code))]
fn cmd_script(exe: &str) -> String {
    let env = [
        ("LOCALAPPDATA", std::env::var("LOCALAPPDATA").ok()),
        ("APPDATA", std::env::var("APPDATA").ok()),
        ("USERPROFILE", std::env::var("USERPROFILE").ok()),
    ];
    let head = "@echo off\r\nrem Launches mark-shell. Rewritten by mark-shell on every start; do not edit.\r\nsetlocal\r\n";
    match ascii_for_cmd(exe, &env) {
        Some(exe) => format!("{head}start \"\" \"{exe}\" %*\r\n"),
        None => format!(
            "{head}for /f \"tokens=2 delims=:\" %%c in ('chcp') do set \"MARKSHELL_CP=%%c\"\r\nchcp 65001 >nul\r\nstart \"\" \"{exe}\" %*\r\nchcp %MARKSHELL_CP% >nul\r\n"
        ),
    }
}

#[cfg(windows)]
fn env_key(write: bool) -> Result<RegKey, String> {
    let flags = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
    RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", flags).map_err(|e| e.to_string())
}

/// ユーザーの PATH（展開前の文字列と、値の種類）
#[cfg(windows)]
fn read_user_path(key: &RegKey) -> (String, RegType) {
    match key.get_raw_value("Path") {
        Ok(v) => {
            let words: Vec<u16> = v.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let s = String::from_utf16_lossy(&words);
            (s.trim_end_matches('\0').to_string(), v.vtype)
        }
        Err(_) => (String::new(), RegType::REG_EXPAND_SZ),
    }
}

#[cfg(windows)]
fn write_user_path(key: &RegKey, value: &str, vtype: RegType) -> Result<(), String> {
    let bytes: Vec<u8> = value.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    key.set_raw_value("Path", &RegValue { bytes: bytes.into(), vtype }).map_err(|e| e.to_string())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn same_dir(entry: &str, dir: &Path) -> bool {
    let e = entry.trim().trim_end_matches(['\\', '/']);
    !e.is_empty() && e.eq_ignore_ascii_case(dir.to_string_lossy().trim_end_matches(['\\', '/']))
}

/// PATH を変えたことを、エクスプローラーなどこれから端末を起動するアプリに知らせる
#[cfg(windows)]
fn broadcast_env_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let name: Vec<u16> = "Environment".encode_utf16().chain([0]).collect();
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, name.as_ptr() as isize, SMTO_ABORTIFHUNG, 3000, std::ptr::null_mut());
    }
}

/// PATH に dir を加える（add が false なら外す）。変えたら true
#[cfg(windows)]
fn update_user_path(dir: &Path, add: bool) -> Result<bool, String> {
    let key = env_key(true)?;
    let (current, vtype) = read_user_path(&key);
    let entries: Vec<&str> = current.split(';').filter(|e| !e.trim().is_empty()).collect();
    let present = entries.iter().any(|e| same_dir(e, dir));
    let next = if add {
        if present {
            return Ok(false);
        }
        let mut v: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
        v.push(dir.to_string_lossy().into_owned());
        v.join(";")
    } else {
        if !present {
            return Ok(false);
        }
        entries.into_iter().filter(|e| !same_dir(e, dir)).collect::<Vec<_>>().join(";")
    };
    write_user_path(&key, &next, vtype)?;
    broadcast_env_change();
    Ok(true)
}

#[cfg(windows)]
fn register_blocking(enable: bool) -> Result<(), String> {
    let dir = bin_dir()?;
    if enable {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy();
        let exe = exe.strip_prefix(r"\\?\").unwrap_or(&exe);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("mark-shell"), sh_script(exe)).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("mark-shell.cmd"), cmd_script(exe)).map_err(|e| e.to_string())?;
        update_user_path(&dir, true)?;
    } else {
        update_user_path(&dir, false)?;
        let _ = std::fs::remove_dir_all(&dir);
    }
    Ok(())
}

#[cfg(not(windows))]
const PROFILE_LINE: &str = "export PATH=\"$HOME/.local/bin:$PATH\" # mark-shell";

#[cfg(not(windows))]
fn update_shell_profiles(add: bool) {
    let Some(home) = dirs::home_dir() else { return };
    for name in [".zprofile", ".zshrc"] {
        let p = home.join(name);
        let content = std::fs::read_to_string(&p).unwrap_or_default();
        if add {
            if !content.contains(".local/bin") {
                let mut next = content;
                if !next.is_empty() && !next.ends_with('\n') {
                    next.push('\n');
                }
                next.push_str(PROFILE_LINE);
                next.push('\n');
                let _ = std::fs::write(&p, next);
            }
        } else if content.contains(PROFILE_LINE) {
            let next: Vec<&str> = content.lines().filter(|l| l.trim() != PROFILE_LINE).collect();
            let _ = std::fs::write(&p, if next.is_empty() { String::new() } else { format!("{}\n", next.join("\n")) });
        }
    }
}

#[cfg(not(windows))]
fn register_blocking(enable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = bin_dir()?;
    let script_path = dir.join("mark-shell");
    if enable {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&script_path, mac_script(&exe.to_string_lossy())).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        update_shell_profiles(true);
    } else {
        let _ = std::fs::remove_file(&script_path);
        update_shell_profiles(false);
    }
    Ok(())
}

/// コマンド `mark-shell` を使えるようにする（enable が false なら外す）。起動のたびにも呼ばれ、
/// アップデートや移動で変わった実行ファイルの場所を書き直す
#[tauri::command]
pub async fn register_cli(enable: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || register_blocking(enable)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn is_cli_registered() -> bool {
    #[cfg(windows)]
    {
        let (Ok(dir), Ok(key)) = (bin_dir(), env_key(false)) else {
            return false;
        };
        let (path, _) = read_user_path(&key);
        dir.join("mark-shell").is_file() && path.split(';').any(|e| same_dir(e, &dir))
    }
    #[cfg(not(windows))]
    {
        bin_dir().map(|d| d.join("mark-shell").is_file()).unwrap_or(false)
    }
}

/// アンインストーラーから `mark-shell.exe --unregister-cli` で呼ばれる
pub fn unregister() {
    let _ = register_blocking(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote() {
        assert_eq!(sh_quote(r"C:\Program Files\mark-shell.exe"), r"'C:\Program Files\mark-shell.exe'");
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn script_uses_lf() {
        assert!(!sh_script(r"C:\x\mark-shell.exe").contains('\r'));
        assert!(!mac_script("/Applications/mark-shell.app/Contents/MacOS/mark-shell").contains('\r'));
    }

    #[test]
    fn cmd_path_ascii() {
        let env = [
            ("LOCALAPPDATA", Some(r"C:\Users\松田\AppData\Local".to_string())),
            ("USERPROFILE", Some(r"C:\Users\松田".to_string())),
        ];
        assert_eq!(ascii_for_cmd(r"C:\Tools\mark-shell.exe", &env).as_deref(), Some(r"C:\Tools\mark-shell.exe"));
        assert_eq!(
            ascii_for_cmd(r"C:\Users\松田\AppData\Local\mark-shell\mark-shell.exe", &env).as_deref(),
            Some(r"%LOCALAPPDATA%\mark-shell\mark-shell.exe")
        );
        assert_eq!(ascii_for_cmd(r"C:\Users\松田\Desktop\mark-shell.exe", &env).as_deref(), Some(r"%USERPROFILE%\Desktop\mark-shell.exe"));
        assert_eq!(ascii_for_cmd(r"C:\Users\松田\デスクトップ\mark-shell.exe", &env), None);
        assert_eq!(ascii_for_cmd(r"D:\松田\mark-shell.exe", &env), None);
    }

    #[test]
    fn dir_match() {
        let d = Path::new(r"C:\Users\a\AppData\Local\mark-shell\bin");
        assert!(same_dir(r"c:\users\a\appdata\local\mark-shell\bin\", d));
        assert!(!same_dir(r"C:\Users\a\AppData\Local\other\bin", d));
        assert!(!same_dir("", d));
    }
}
