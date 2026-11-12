//! 次の起動で復元するウィンドウの記録。ウィンドウ（= プロセス）ごとに
//! `%LOCALAPPDATA%\mark-shell\windows\<pid>.json` に開いているフォルダと閉じた時刻を書く。
//!
//! 起動時（コマンドラインでの指定がないとき）に、ほかの mark-shell が動いていなければ、
//! 最後に閉じたウィンドウと、その前後 [`SAME_CLOSE_MS`] 以内に閉じたウィンドウをまとめて復元する。
//! 閉じた時刻がないまま終わったもの（強制終了やシャットダウン）は、最後まで開いていたものとして扱う。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::local_dir;
use crate::textio::write_atomic;

/// この時間以内に続けて閉じたウィンドウは、同時に閉じたものとみなす
const SAME_CLOSE_MS: u64 = 5000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    root: String,
    /// 閉じた時刻（UNIX 時間のミリ秒）。開いている間は None
    closed_at: Option<u64>,
}

fn dir() -> PathBuf {
    local_dir().join("windows")
}

fn own_path() -> PathBuf {
    dir().join(format!("{}.json", std::process::id()))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn write(path: &Path, rec: &Record) {
    let _ = fs::create_dir_all(dir());
    if let Ok(s) = serde_json::to_vec(rec) {
        let _ = write_atomic(path, &s);
    }
}

fn read(path: &Path) -> Option<Record> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// このウィンドウでフォルダを開いた
pub fn register(root: &str) {
    write(&own_path(), &Record { root: root.to_string(), closed_at: None });
}

/// このウィンドウを閉じた（プロセスが終わる）
pub fn mark_closed() {
    let path = own_path();
    if let Some(mut rec) = read(&path) {
        rec.closed_at = Some(now_ms());
        write(&path, &rec);
    }
}

/// pid のプロセスが動いている mark-shell か（pid が別のプロセスに使い回されていても誤らないよう、実行ファイル名も比べる）
#[cfg(windows)]
fn is_running_mark_shell(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let own_name = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let alive = GetExitCodeProcess(h, &mut code) != 0 && code == STILL_ACTIVE as u32;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let name = if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0 {
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            Path::new(&full).file_name().map(|n| n.to_string_lossy().to_lowercase())
        } else {
            None
        };
        CloseHandle(h);
        alive && name.is_some() && name == own_name
    }
}

#[cfg(not(windows))]
fn is_running_mark_shell(pid: u32) -> bool {
    let own_name = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let Ok(out) = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output() else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let comm = String::from_utf8_lossy(&out.stdout);
    let name = Path::new(comm.trim()).file_name().map(|n| n.to_string_lossy().to_lowercase());
    name.is_some() && name == own_name
}

/// 閉じたウィンドウ（root, 閉じた時刻）から、まとめて復元するものを選ぶ。最後に閉じたものが先頭
fn pick(mut closed: Vec<(String, u64)>) -> Vec<String> {
    let Some(latest) = closed.iter().map(|(_, t)| *t).max() else {
        return Vec::new();
    };
    closed.retain(|(_, t)| *t >= latest.saturating_sub(SAME_CLOSE_MS));
    closed.sort_by_key(|c| std::cmp::Reverse(c.1));
    let mut roots: Vec<String> = Vec::new();
    for (root, _) in closed {
        if !roots.iter().any(|r| r.eq_ignore_ascii_case(&root)) {
            roots.push(root);
        }
    }
    roots
}

/// 前回まとめて閉じたウィンドウのフォルダ（最後に閉じたものが先頭）。ほかの mark-shell が動いていれば空。
/// 返したものを含め、閉じたウィンドウの記録は消す
#[tauri::command]
pub fn take_restore_windows() -> Vec<String> {
    let own = std::process::id();
    let Ok(entries) = fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut closed = Vec::new();
    let mut stale = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let Some(pid) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == own {
            continue;
        }
        let Some(rec) = read(&path) else {
            stale.push(path);
            continue;
        };
        match rec.closed_at {
            Some(t) => closed.push((rec.root, t)),
            // ほかのウィンドウが開いているなら、復元しない（記録もそのまま残す）
            None if is_running_mark_shell(pid) => return Vec::new(),
            // 閉じずに終わった（強制終了など）ものは、最後まで開いていたとみなす
            None => closed.push((rec.root, u64::MAX)),
        }
        stale.push(path);
    }
    for p in stale {
        let _ = fs::remove_file(p);
    }
    pick(closed).into_iter().filter(|r| Path::new(r).is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[(&str, u64)]) -> Vec<(String, u64)> {
        v.iter().map(|(r, t)| (r.to_string(), *t)).collect()
    }

    #[test]
    fn picks_windows_closed_together() {
        assert_eq!(pick(s(&[("A", 1_000), ("B", 100_000), ("C", 102_000)])), vec!["C", "B"]);
        assert_eq!(pick(s(&[("A", 1_000)])), vec!["A"]);
        assert!(pick(Vec::new()).is_empty());
    }

    #[test]
    fn unclosed_counts_as_last() {
        assert_eq!(pick(s(&[("A", 1_000), ("B", u64::MAX), ("C", u64::MAX)])), vec!["B", "C"]);
    }

    #[test]
    fn dedupes_roots() {
        assert_eq!(pick(s(&[(r"C:\a", 10), (r"c:\A", 20)])), vec![r"c:\A"]);
    }
}
