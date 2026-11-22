//! 画像を OS のクリップボードへコピーする処理。
//! 画面側が PNG 以外の形式（JPEG・WebP・SVG など）を PNG に変換して渡すことで、
//! 追加のデコーダを持たずに全形式をロスレス＆透過付きでクリップボードへ書けるようにする。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use markshell_core::tr;
use tauri::{AppHandle, Manager};

use crate::paths::resolve;
use crate::state::AppState;

const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_CLIPBOARD_BYTES: usize = 50 * 1024 * 1024;
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Drop 時に一時ファイルを確実に削除するガード
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn create(bytes: &[u8]) -> Result<Self, String> {
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let name = format!("mark-shell-clip-{}-{seq}.png", std::process::id());
        let path = std::env::temp_dir().join(name);
        fs::write(&path, bytes).map_err(|e| {
            tr!(
                "一時ファイルを作れません: {e}",
                "Cannot create temporary file: {e}",
                "无法创建临时文件：{e}"
            )
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < PNG_MAGIC.len() || &bytes[..PNG_MAGIC.len()] != PNG_MAGIC {
        return Err(tr!(
            "PNG 画像のデータではありません",
            "Invalid PNG image data",
            "不是有效的 PNG 图片数据"
        ));
    }
    if bytes.len() > MAX_CLIPBOARD_BYTES {
        return Err(tr!(
            "画像が大きすぎるためコピーできません",
            "Image is too large to copy",
            "图片过大，无法复制"
        ));
    }
    Ok(())
}

/// PNG バイト列を OS のクリップボードに書き込む
pub fn write_png(bytes: &[u8]) -> Result<(), String> {
    validate_png(bytes)?;
    write_png_os(bytes)
}

#[cfg(target_os = "macos")]
fn write_png_os(bytes: &[u8]) -> Result<(), String> {
    let tmp = TempFile::create(bytes)?;
    // PNG（透過を保持）と TIFF（古い macOS アプリ向け）の両方を NSPasteboard に設定する
    const SCRIPT: &str = r#"
use framework "AppKit"
use framework "Foundation"
on run argv
  set d to current application's NSData's dataWithContentsOfFile:(item 1 of argv)
  if d is missing value then error "Failed to read image data"
  set pb to current application's NSPasteboard's generalPasteboard()
  pb's clearContents()
  set ok to pb's setData:d forType:(current application's NSPasteboardTypePNG)
  if not ok then error "Failed to write PNG to pasteboard"
  set img to current application's NSImage's alloc()'s initWithData:d
  if img is not missing value then
    set tiff to img's TIFFRepresentation()
    if tiff is not missing value then
      pb's setData:tiff forType:(current application's NSPasteboardTypeTIFF)
    end if
  end if
end run
"#;
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT, "--"])
        .arg(tmp.path())
        .output()
        .map_err(|e| {
            tr!(
                "クリップボードにコピーできません: {e}",
                "Cannot copy to clipboard: {e}",
                "无法复制到剪贴板：{e}"
            )
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(tr!(
            "クリップボードにコピーできません: {stderr}",
            "Cannot copy to clipboard: {stderr}",
            "无法复制到剪贴板：{stderr}"
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn write_png_os(bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let tmp = TempFile::create(bytes)?;
    // 透過 PNG に対応したアプリ向けに PNG ストリーム、従来アプリ向けに Bitmap の両方を DataObject に積む
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bytes = [System.IO.File]::ReadAllBytes($env:MARKSHELL_CLIP_PNG)
$ms = [System.IO.MemoryStream]::new($bytes)
$img = [System.Drawing.Image]::FromStream($ms)
$pngStream = [System.IO.MemoryStream]::new($bytes)
$data = [System.Windows.Forms.DataObject]::new()
$data.SetData('PNG', $false, $pngStream)
$data.SetImage($img)
[System.Windows.Forms.Clipboard]::SetDataObject($data, $true)
"#;
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", SCRIPT])
        .env("MARKSHELL_CLIP_PNG", tmp.path())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| {
            tr!(
                "クリップボードにコピーできません: {e}",
                "Cannot copy to clipboard: {e}",
                "无法复制到剪贴板：{e}"
            )
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(tr!(
            "クリップボードにコピーできません: {stderr}",
            "Cannot copy to clipboard: {stderr}",
            "无法复制到剪贴板：{stderr}"
        ));
    }
    Ok(())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn write_png_os(bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let (cmd, args): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() && which::which("wl-copy").is_ok() {
        ("wl-copy", &["--type", "image/png"])
    } else if which::which("xclip").is_ok() {
        ("xclip", &["-selection", "clipboard", "-t", "image/png", "-i"])
    } else {
        return Err(tr!(
            "クリップボードへ画像をコピーするには wl-copy または xclip が必要です",
            "wl-copy or xclip is required to copy images to the clipboard",
            "需要安装 wl-copy 或 xclip 才能将图片复制到剪贴板"
        ));
    };

    // xclip は stdout/stderr が開いたままだと親プロセスの wait() が終わらないので null にする
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            tr!(
                "クリップボードにコピーできません: {e}",
                "Cannot copy to clipboard: {e}",
                "无法复制到剪贴板：{e}"
            )
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(tr!(
            "クリップボードにコピーできませんでした",
            "Failed to copy to clipboard",
            "复制到剪贴板失败"
        ));
    }
    Ok(())
}

/// ワークスペース内の PNG ファイルをそのままクリップボードにコピーする
#[tauri::command]
pub async fn copy_image_file(app: AppHandle, path: String) -> Result<(), String> {
    let root = app.state::<AppState>().root()?;
    tauri::async_runtime::spawn_blocking(move || {
        let p = resolve(&root, &path)?;
        let bytes = fs::read(&p).map_err(|e| {
            tr!(
                "{path} を読めません: {e}",
                "Cannot read {path}: {e}",
                "无法读取 {path}：{e}"
            )
        })?;
        write_png(&bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// base64 エンコードされた PNG データをクリップボードにコピーする
#[tauri::command]
pub async fn copy_image_png(data: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|e| e.to_string())?;
        write_png(&bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_png_magic() {
        assert!(validate_png(b"\x89PNG\r\n\x1a\nrest").is_ok());
        assert!(validate_png(b"not a png").is_err());
        assert!(validate_png(b"").is_err());
    }

    #[test]
    fn temp_file_is_removed_on_drop() {
        let path = {
            let tmp = TempFile::create(b"\x89PNG\r\n\x1a\n").unwrap();
            let p = tmp.path().to_path_buf();
            assert!(p.exists());
            p
        };
        assert!(!path.exists());
    }
}
