//! 使っていないウィンドウのメモリを減らす。フォーカスを失ったら（最小化を含む）WebView2 の
//! MemoryUsageTargetLevel を Low にし、キャッシュを捨てたりディスクへ逃がしたりさせる。
//! フォーカスが戻ったら Normal に戻す。Windows だけの機能で、Mac では何もしない

/// ウィンドウのイベントを受けて、WebView2 のメモリの目安を切り替える
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Focused(focused) = event {
        set_low_memory(window, !*focused);
    }
}

#[cfg(not(windows))]
fn set_low_memory(_window: &tauri::Window, _low: bool) {}

#[cfg(windows)]
fn set_low_memory(window: &tauri::Window, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;

    let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
    for webview in window.webviews() {
        let _ = webview.with_webview(move |wv| unsafe {
            // 古い WebView2 ランタイムには ICoreWebView2_19 がないので、そのときは何もしない
            let Ok(core) = wv.controller().CoreWebView2() else { return };
            if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                let _ = core19.SetMemoryUsageTargetLevel(level);
            }
        });
    }
}
