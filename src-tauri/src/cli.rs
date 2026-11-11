//! コマンド `mark-shell`。WSL のシェル（と cmd / PowerShell）から `mark-shell ~/notes` のように起動できるようにする。
//!
//! `%LOCALAPPDATA%\mark-shell\bin` に起動用のスクリプトを置き、そのフォルダをユーザーの PATH（HKCU）に加える。
//! WSL は既定で Windows の PATH を引き継ぐので、WSL の中からも `mark-shell` が見つかる（VS Code の `code` と同じ仕組み）。
//! WSL 用のスクリプトは Linux のパスを `wslpath -w` で `\\wsl.localhost\...` に直してから mark-shell.exe に渡す。

use std::path::{Path, PathBuf};

use winreg::enums::{RegType, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::{RegKey, RegValue};

fn bin_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir().ok_or("LOCALAPPDATA が見つかりません")?;
    Ok(base.join("mark-shell").join("bin"))
}

/// sh の単一引用符で囲む
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// WSL のシェルから使うスクリプト（改行は LF）
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
"#,
        exe = sh_quote(exe)
    )
}

/// cmd / PowerShell から使うスクリプト
fn cmd_script(exe: &str) -> String {
    format!("@echo off\r\nrem mark-shell を起動する。mark-shell が起動のたびに書き直すので編集しないでください。\r\nstart \"\" \"{exe}\" %*\r\n")
}

fn env_key(write: bool) -> Result<RegKey, String> {
    let flags = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
    RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", flags).map_err(|e| e.to_string())
}

/// ユーザーの PATH（展開前の文字列と、値の種類）
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

fn write_user_path(key: &RegKey, value: &str, vtype: RegType) -> Result<(), String> {
    let bytes: Vec<u8> = value.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    key.set_raw_value("Path", &RegValue { bytes: bytes.into(), vtype }).map_err(|e| e.to_string())
}

fn same_dir(entry: &str, dir: &Path) -> bool {
    let e = entry.trim().trim_end_matches('\\');
    !e.is_empty() && e.eq_ignore_ascii_case(dir.to_string_lossy().trim_end_matches('\\'))
}

/// PATH を変えたことを、エクスプローラーなどこれから端末を起動するアプリに知らせる
fn broadcast_env_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let name: Vec<u16> = "Environment".encode_utf16().chain([0]).collect();
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, name.as_ptr() as isize, SMTO_ABORTIFHUNG, 3000, std::ptr::null_mut());
    }
}

/// PATH に dir を加える（add が false なら外す）。変えたら true
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

/// コマンド `mark-shell` を使えるようにする（enable が false なら外す）。起動のたびにも呼ばれ、
/// アップデートや移動で変わった実行ファイルの場所を書き直す
#[tauri::command]
pub async fn register_cli(enable: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || register_blocking(enable)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn is_cli_registered() -> bool {
    let (Ok(dir), Ok(key)) = (bin_dir(), env_key(false)) else {
        return false;
    };
    let (path, _) = read_user_path(&key);
    dir.join("mark-shell").is_file() && path.split(';').any(|e| same_dir(e, &dir))
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
    }

    #[test]
    fn dir_match() {
        let d = Path::new(r"C:\Users\a\AppData\Local\mark-shell\bin");
        assert!(same_dir(r"c:\users\a\appdata\local\mark-shell\bin\", d));
        assert!(!same_dir(r"C:\Users\a\AppData\Local\other\bin", d));
        assert!(!same_dir("", d));
    }
}
