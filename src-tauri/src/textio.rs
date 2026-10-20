//! テキストファイルの読み書き（文字コード・改行コードの判定と保持、壊れない保存）

use std::fs;
use std::io::Write;
use std::path::Path;

pub struct Decoded {
    /// 改行を LF にそろえた本文
    pub text: String,
    pub encoding: String,
    pub eol: &'static str,
}

pub fn decode(bytes: &[u8]) -> Decoded {
    let (text, encoding) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(rest).into_owned(), "utf-8-bom".to_string())
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (decode_utf16(rest, true), "utf-16le".to_string())
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (decode_utf16(rest, false), "utf-16be".to_string())
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        (s.to_string(), "utf-8".to_string())
    } else {
        let mut det = chardetng::EncodingDetector::new();
        det.feed(bytes, true);
        let enc = det.guess(Some(b"jp"), true);
        let (s, _) = enc.decode_without_bom_handling(bytes);
        (s.into_owned(), enc.name().to_string())
    };
    let eol = if text.contains("\r\n") { "crlf" } else { "lf" };
    let text = if eol == "crlf" { text.replace("\r\n", "\n") } else { text };
    Decoded { text, encoding, eol }
}

fn decode_utf16(bytes: &[u8], le: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

/// 本文を指定の文字コード・改行コードでバイト列にする。
/// その文字コードで表せない文字があれば Err("unmappable")
pub fn encode(text: &str, encoding: &str, eol: &str) -> Result<Vec<u8>, String> {
    let text = if eol == "crlf" { text.replace('\n', "\r\n") } else { text.to_string() };
    match encoding {
        "utf-8" | "" => Ok(text.into_bytes()),
        "utf-8-bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend(text.into_bytes());
            Ok(v)
        }
        "utf-16le" | "utf-16be" => {
            let le = encoding == "utf-16le";
            let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
            for u in text.encode_utf16() {
                v.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            Ok(v)
        }
        label => {
            let enc = encoding_rs::Encoding::for_label(label.as_bytes())
                .ok_or_else(|| format!("未対応の文字コードです: {label}"))?;
            let (bytes, _, had_errors) = enc.encode(&text);
            if had_errors {
                return Err("unmappable".into());
            }
            Ok(bytes.into_owned())
        }
    }
}

/// 一時ファイルに書いてから置き換える。途中で落ちても元のファイルは壊れない
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("親フォルダがありません"))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.mark-shell.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_crlf_bom() {
        let d = decode(b"\xEF\xBB\xBFa\r\nb");
        assert_eq!(d.text, "a\nb");
        assert_eq!(d.encoding, "utf-8-bom");
        assert_eq!(d.eol, "crlf");
        assert_eq!(encode(&d.text, &d.encoding, d.eol).unwrap(), b"\xEF\xBB\xBFa\r\nb");
    }

    #[test]
    fn shift_jis() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("日本語のメモです。ひらがなとカタカナ。");
        let d = decode(&bytes);
        assert_eq!(d.encoding, "Shift_JIS");
        assert_eq!(d.text, "日本語のメモです。ひらがなとカタカナ。");
        assert_eq!(encode("🍣", "Shift_JIS", "lf").unwrap_err(), "unmappable");
    }
}
