//! 表示言語。画面側で選んだ言語（ja / en / zh）を受け取り、エラーメッセージなどをその言語にする。
//! 文言は呼び出し箇所に 3 言語を並べて書く:
//!
//! ```ignore
//! Err(tr!("{path} を開けません: {e}", "Cannot open {path}: {e}", "无法打开 {path}：{e}"))
//! ```

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Locale {
    Ja,
    En,
    Zh,
}

/// 画面側から言語が届くまでは英語
static LOCALE: AtomicU8 = AtomicU8::new(1);

pub fn locale() -> Locale {
    match LOCALE.load(Ordering::Relaxed) {
        0 => Locale::Ja,
        2 => Locale::Zh,
        _ => Locale::En,
    }
}

/// "ja" / "en" / "zh"（"zh-CN" なども可）。それ以外は英語
pub fn parse(code: &str) -> Locale {
    let c = code.to_ascii_lowercase();
    if c.starts_with("ja") {
        Locale::Ja
    } else if c.starts_with("zh") {
        Locale::Zh
    } else {
        Locale::En
    }
}

pub fn set_locale(l: Locale) {
    LOCALE.store(
        match l {
            Locale::Ja => 0,
            Locale::En => 1,
            Locale::Zh => 2,
        },
        Ordering::Relaxed,
    );
}

pub fn code(l: Locale) -> &'static str {
    match l {
        Locale::Ja => "ja",
        Locale::En => "en",
        Locale::Zh => "zh",
    }
}

/// 今の言語の文言を format! で作る。文字列の中の `{name}` は呼び出し側の変数になる
#[macro_export]
macro_rules! tr {
    ($ja:literal, $en:literal, $zh:literal $(,)?) => {
        match $crate::i18n::locale() {
            $crate::i18n::Locale::Ja => format!($ja),
            $crate::i18n::Locale::Zh => format!($zh),
            $crate::i18n::Locale::En => format!($en),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select() {
        let path = "a.md";
        set_locale(parse("zh-CN"));
        assert_eq!(tr!("{path} を開けません", "Cannot open {path}", "无法打开 {path}"), "无法打开 a.md");
        set_locale(parse("fr"));
        assert_eq!(locale(), Locale::En);
        set_locale(Locale::Ja);
        assert_eq!(tr!("はい", "Yes", "是"), "はい");
        set_locale(Locale::En);
    }
}
