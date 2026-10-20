//! 名前変更・移動に合わせたリンクの書き換え。
//!
//! 動かしたパスの対応表（旧 → 新）を作り、すべての Markdown ファイルについて
//! 「旧い場所から見たリンク先」を「新しい場所から見たリンク先」に書き直す。
//! 動かしたファイル自身が持つ相対リンクも同じ方法で直る。

use std::path::Path;
use std::sync::{Arc, Mutex};

use ignore::WalkState;
use regex::Regex;
use serde::Serialize;

use crate::paths::{self, normalize_rel, relative, resolve, to_rel, FilterConfig};
use crate::textio;
use crate::workspace::walker;

#[derive(Serialize, Default)]
pub struct Plan {
    pub links: usize,
    pub files: Vec<String>,
}

struct Move {
    from: String,
    to: String,
    dir: bool,
}

impl Move {
    /// 旧パス → 新パス（動かした範囲に入らなければそのまま）
    fn map(&self, p: &str) -> Option<String> {
        if p.eq_ignore_ascii_case(&self.from) {
            return Some(self.to.clone());
        }
        if self.dir {
            let prefix = format!("{}/", self.from);
            if p.len() > prefix.len() && p[..prefix.len()].eq_ignore_ascii_case(&prefix) {
                return Some(format!("{}/{}", self.to, &p[prefix.len()..]));
            }
        }
        None
    }

    /// 新パス → 旧パス
    fn unmap(&self, p: &str) -> Option<String> {
        Move { from: self.to.clone(), to: self.from.clone(), dir: self.dir }.map(p)
    }
}

fn md_link_re() -> Regex {
    // [text](target "title") と ![alt](target)。target は <...> 形式も扱う
    Regex::new(r#"(!?\[[^\]\n]*\]\()(<[^>\n]+>|[^)\s]+)((?:\s+"[^"\n]*")?\))"#).unwrap()
}

fn wiki_re() -> Regex {
    Regex::new(r"\[\[([^\[\]\n|#]+)((?:#[^\[\]\n|]*)?)((?:\|[^\[\]\n]*)?)\]\]").unwrap()
}

fn is_external(target: &str) -> bool {
    target.starts_with('#')
        || target.starts_with('/')
        || target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with("data:")
}

fn encode_target(path: &str) -> String {
    path.replace('%', "%25").replace(' ', "%20").replace('(', "%28").replace(')', "%29")
}

/// 1 ファイル分の書き換え。戻り値は（新しい本文, 書き換えたリンク数）
fn rewrite(text: &str, old_self: &str, new_self: &str, mv: &Move, renamed_stem: Option<(&str, &str)>) -> (String, usize) {
    let old_dir = paths::parent_rel(old_self);
    let new_dir = paths::parent_rel(new_self);
    let self_moved = !old_dir.eq_ignore_ascii_case(new_dir);
    let mut count = 0;

    let out = md_link_re().replace_all(text, |c: &regex::Captures| {
        let whole = c[0].to_string();
        let raw = &c[2];
        let angle = raw.starts_with('<');
        let target = raw.trim_start_matches('<').trim_end_matches('>');
        if is_external(target) {
            return whole;
        }
        let (path_part, frag) = match target.find('#') {
            Some(i) => (&target[..i], &target[i..]),
            None => (target, ""),
        };
        let decoded = percent_encoding::percent_decode_str(path_part).decode_utf8_lossy();
        let Some(old_target) = normalize_rel(&paths::join_rel(old_dir, &decoded)) else { return whole };
        let new_target = mv.map(&old_target);
        if new_target.is_none() && !self_moved {
            return whole;
        }
        let new_target = new_target.unwrap_or(old_target);
        let rel = relative(new_dir, &new_target);
        let new_raw = if angle { format!("<{rel}{frag}>") } else { format!("{}{frag}", encode_target(&rel)) };
        if new_raw == raw {
            return whole;
        }
        count += 1;
        format!("{}{}{}", &c[1], new_raw, &c[3])
    });

    let out = match renamed_stem {
        Some((old_stem, new_stem)) => wiki_re()
            .replace_all(&out, |c: &regex::Captures| {
                let name = c[1].trim();
                // [[フォルダ/ファイル名]] の形も最後の要素で比べる
                let last = paths::file_name(name);
                let last = last.strip_suffix(".md").unwrap_or(last);
                if !last.eq_ignore_ascii_case(old_stem) {
                    return c[0].to_string();
                }
                count += 1;
                let prefix = &name[..name.len() - paths::file_name(name).len()];
                format!("[[{prefix}{new_stem}{}{}]]", &c[2], &c[3])
            })
            .into_owned(),
        None => out.into_owned(),
    };
    (out, count)
}

/// `from` を `to` に動かしたときのリンク書き換え。`apply` が false なら数えるだけ。
/// `apply` が true のときは既に動かした後で呼ぶ
pub fn plan(root: &Path, filter: &Arc<FilterConfig>, from: &str, to: &str, apply: bool) -> Result<Plan, String> {
    let from = normalize_rel(from).ok_or("不正なパスです")?;
    let to = normalize_rel(to).ok_or("不正なパスです")?;
    let current = if apply { &to } else { &from };
    let dir = resolve(root, current)?.is_dir();
    let mv = Move { from: from.clone(), to: to.clone(), dir };

    let from_stem = paths::file_stem(&from).to_string();
    let to_stem = paths::file_stem(&to).to_string();
    let renamed_stem = (!dir && filter.is_markdown(Path::new(&from)) && !from_stem.eq_ignore_ascii_case(&to_stem))
        .then_some((from_stem.as_str(), to_stem.as_str()));

    let result = Mutex::new(Plan::default());
    let error = Mutex::new(None::<String>);
    walker(root, root, filter).build_parallel().run(|| {
        let mv = &mv;
        let result = &result;
        let error = &error;
        Box::new(move |res| {
            let Ok(e) = res else { return WalkState::Continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) || !filter.is_markdown(e.path()) {
                return WalkState::Continue;
            }
            let Some(rel) = to_rel(root, e.path()) else { return WalkState::Continue };
            // 今の場所と、動かす前（後）の場所
            let (old_self, new_self) = if apply {
                (mv.unmap(&rel).unwrap_or_else(|| rel.clone()), rel.clone())
            } else {
                (rel.clone(), mv.map(&rel).unwrap_or_else(|| rel.clone()))
            };
            let Ok(bytes) = std::fs::read(e.path()) else { return WalkState::Continue };
            let d = textio::decode(&bytes);
            let (text, n) = rewrite(&d.text, &old_self, &new_self, mv, renamed_stem);
            if n == 0 {
                return WalkState::Continue;
            }
            if apply {
                let bytes = textio::encode(&text, &d.encoding, d.eol).or_else(|_| textio::encode(&text, "utf-8", d.eol));
                match bytes.map(|b| textio::write_atomic(e.path(), &b).map_err(|e| e.to_string())) {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) | Err(err) => {
                        *error.lock().unwrap() = Some(format!("{rel}: {err}"));
                        return WalkState::Continue;
                    }
                }
            }
            let mut r = result.lock().unwrap();
            r.links += n;
            r.files.push(rel);
            WalkState::Continue
        })
    });
    if let Some(err) = error.into_inner().unwrap() {
        return Err(format!("一部のリンクを書き換えられませんでした: {err}"));
    }
    Ok(result.into_inner().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(from: &str, to: &str, dir: bool) -> Move {
        Move { from: from.into(), to: to.into(), dir }
    }

    #[test]
    fn link_to_moved_file() {
        let m = mv("notes/a.md", "archive/a.md", false);
        let (t, n) = rewrite("see [A](a.md#x) and ![](img.png)", "notes/b.md", "notes/b.md", &m, None);
        assert_eq!(t, "see [A](../archive/a.md#x) and ![](img.png)");
        assert_eq!(n, 1);
    }

    #[test]
    fn moved_file_own_links() {
        let m = mv("notes/a.md", "a.md", false);
        let (t, n) = rewrite("[b](b.md) [web](https://x.y/z)", "notes/a.md", "a.md", &m, None);
        assert_eq!(t, "[b](notes/b.md) [web](https://x.y/z)");
        assert_eq!(n, 1);
    }

    #[test]
    fn dir_move_and_wiki() {
        let m = mv("old", "new dir", true);
        let (t, _) = rewrite("[x](old/x.md)", "top.md", "top.md", &m, None);
        assert_eq!(t, "[x](new%20dir/x.md)");
        let m = mv("a.md", "b.md", false);
        let (t, n) = rewrite("[[a]] [[a#h|表示]] [[ab]]", "c.md", "c.md", &m, Some(("a", "b")));
        assert_eq!(t, "[[b]] [[b#h|表示]] [[ab]]");
        assert_eq!(n, 2);
    }
}
