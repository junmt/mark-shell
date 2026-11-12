//! ウィンドウのタイトルバー（Windows の枠）の色。開いているフォルダの .vscode/settings.json の
//! workbench.colorCustomizations（titleBar.*）に合わせる。Windows 11 の DWM の機能で、Windows 10 では何も起きない

use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};

/// 既定の色に戻す値（DWMWA_COLOR_DEFAULT）
const COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

/// "#rrggbb" → COLORREF（0x00bbggrr）。None や読めない値は既定の色
fn colorref(hex: Option<&str>) -> u32 {
    let Some(h) = hex.and_then(|h| h.strip_prefix('#')).filter(|h| h.len() >= 6 && h.is_char_boundary(6)) else {
        return COLOR_DEFAULT;
    };
    match u32::from_str_radix(&h[..6], 16) {
        Ok(rgb) => ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF),
        Err(_) => COLOR_DEFAULT,
    }
}

/// タイトルバーの背景と文字の色を変える（None なら既定の色に戻す）。枠線も背景と同じ色にする
#[tauri::command]
pub fn set_title_bar_colors(window: tauri::WebviewWindow, background: Option<String>, foreground: Option<String>) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0;
    let set = |attr: i32, color: u32| unsafe {
        DwmSetWindowAttribute(hwnd, attr as u32, &color as *const u32 as *const _, std::mem::size_of::<u32>() as u32)
    };
    let bg = colorref(background.as_deref());
    set(DWMWA_CAPTION_COLOR, bg);
    set(DWMWA_BORDER_COLOR, bg);
    set(DWMWA_TEXT_COLOR, colorref(foreground.as_deref()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_colorref() {
        assert_eq!(colorref(Some("#112233")), 0x0033_2211);
        assert_eq!(colorref(Some("#11223380")), 0x0033_2211);
        assert_eq!(colorref(Some("red")), COLOR_DEFAULT);
        assert_eq!(colorref(Some("#12")), COLOR_DEFAULT);
        assert_eq!(colorref(None), COLOR_DEFAULT);
    }
}
