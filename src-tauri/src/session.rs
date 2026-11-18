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
    let own_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "mark-shell".into());
    let Ok(out) = std::process::Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let comm = String::from_utf8_lossy(&out.stdout);
    let comm = comm.trim();
    Path::new(comm)
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case(&own_name))
        .unwrap_or(false)
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
        push_unique(&mut roots, root);
    }
    roots
}

fn push_unique(roots: &mut Vec<String>, root: String) {
    if !roots.iter().any(|r| r.eq_ignore_ascii_case(&root)) {
        roots.push(root);
    }
}

/// 記録ファイルの pid（記録ファイルでなければ None）
fn record_pid(path: &Path) -> Option<u32> {
    path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok())
}

/// 更新で再起動するときに開き直すウィンドウの記録
fn update_marker() -> PathBuf {
    dir().join("update-restart.json")
}

/// 更新の記録がこれより古ければ使わない（インストールが取り消されたときなど）
const UPDATE_MARKER_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMarker {
    roots: Vec<String>,
    created_at: u64,
}

/// 更新のインストール直前に呼ぶ。いま開いているすべてのウィンドウのフォルダを記録し、再起動後にまとめて開き直す。
/// Windows ではインストーラーがほかのウィンドウを強制終了し、更新したウィンドウも閉じた時刻を書かずに
/// 終わる（さらに起動引数を引き継いで起動し直す）ので、通常の復元では 1 つしか戻らない。
/// macOS などでは、更新してもほかのウィンドウ（プロセス）は閉じられないので何もしない
#[tauri::command]
pub fn mark_update_restart() {
    if !cfg!(windows) {
        return;
    }
    let own = std::process::id();
    let mut roots = Vec::new();
    if let Some(rec) = read(&own_path()) {
        push_unique(&mut roots, rec.root);
    }
    if let Ok(entries) = fs::read_dir(dir()) {
        for e in entries.flatten() {
            let path = e.path();
            let Some(pid) = record_pid(&path) else { continue };
            if pid == own {
                continue;
            }
            if let Some(rec) = read(&path) {
                if rec.closed_at.is_none() && is_running_mark_shell(pid) {
                    push_unique(&mut roots, rec.root);
                }
            }
        }
    }
    let _ = fs::create_dir_all(dir());
    if let Ok(s) = serde_json::to_vec(&UpdateMarker { roots, created_at: now_ms() }) {
        let _ = write_atomic(&update_marker(), &s);
    }
}

/// 更新のインストールに失敗したとき、記録を取り消す
#[tauri::command]
pub fn cancel_update_restart() {
    let _ = fs::remove_file(update_marker());
}

/// 更新の記録があれば取り出す（記録は消す）
fn take_update_marker() -> Option<Vec<String>> {
    let path = update_marker();
    let marker: Option<UpdateMarker> = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let _ = fs::remove_file(&path);
    let marker = marker?;
    (now_ms().saturating_sub(marker.created_at) <= UPDATE_MARKER_TTL_MS).then_some(marker.roots)
}

/// 動いていない mark-shell の記録を消す
fn remove_stale_records() {
    let own = std::process::id();
    let Ok(entries) = fs::read_dir(dir()) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Some(pid) = record_pid(&path) else { continue };
        if pid != own && !is_running_mark_shell(pid) {
            let _ = fs::remove_file(path);
        }
    }
}

/// 起動時に開き直すウィンドウのフォルダ。
/// - 更新で再起動したときは、更新前に開いていたすべてのウィンドウ（更新したウィンドウが先頭）。
///   `launched`（コマンドラインで指定された）でも返すので、呼び出し側で重複を除く
/// - それ以外で `launched` なら空
/// - それ以外は、前回まとめて閉じたウィンドウのフォルダ（最後に閉じたものが先頭）。ほかの mark-shell が動いていれば空。
///   返したものを含め、閉じたウィンドウの記録は消す
#[tauri::command]
pub fn take_restore_windows(launched: bool) -> Vec<String> {
    if let Some(roots) = take_update_marker() {
        remove_stale_records();
        return roots.into_iter().filter(|r| Path::new(r).is_dir()).collect();
    }
    if launched {
        return Vec::new();
    }
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
