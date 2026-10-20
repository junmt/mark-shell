//! プレビューで画像を表示するための `mdimg` プロトコル。
//! ルートディレクトリ内の画像だけを返す（ルートの外は 403）。

use std::borrow::Cow;

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext};

use crate::paths::resolve;
use crate::state::AppState;

fn mime(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

fn respond(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .body(Cow::Owned(body))
        .unwrap()
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let state = ctx.app_handle().state::<AppState>();
    let Ok(root) = state.root() else {
        return respond(StatusCode::NOT_FOUND, "text/plain", vec![]);
    };
    let raw = request.uri().path().trim_start_matches('/');
    let rel = percent_encoding::percent_decode_str(raw).decode_utf8_lossy();
    let Ok(path) = resolve(&root, &rel) else {
        return respond(StatusCode::FORBIDDEN, "text/plain", vec![]);
    };
    let Some(ct) = path.extension().and_then(|e| e.to_str()).and_then(mime) else {
        return respond(StatusCode::FORBIDDEN, "text/plain", vec![]);
    };
    match std::fs::read(&path) {
        Ok(bytes) => respond(StatusCode::OK, ct, bytes),
        Err(_) => respond(StatusCode::NOT_FOUND, "text/plain", vec![]),
    }
}
