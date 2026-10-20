use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde_json::Value;

/// 既定で探索から外すフォルダ名
const DEFAULT_EXCLUDED: &[&str] = &[
    ".git",
    "node_modules",
    ".venv",
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// 探索の対象と除外の設定
pub struct FilterConfig {
    pub extensions: Vec<String>,
    pub excluded_names: Vec<String>,
    pub exclude_globs: GlobSet,
    pub respect_gitignore: bool,
    pub follow_symlinks: bool,
}

impl FilterConfig {
    pub fn from_config(config: &Value) -> Self {
        let extensions = config
            .get("markdownExtensions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim_start_matches('.').to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec!["md".into(), "markdown".into(), "mdx".into()]);

        let mut builder = GlobSetBuilder::new();
        if let Some(patterns) = config.get("excludePatterns").and_then(Value::as_array) {
            for p in patterns.iter().filter_map(Value::as_str) {
                let p = p.trim().trim_end_matches('/');
                if p.is_empty() {
                    continue;
                }
                if let Ok(g) = Glob::new(p) {
                    builder.add(g);
                }
            }
        }
        let exclude_globs = builder.build().unwrap_or_else(|_| GlobSet::empty());

        Self {
            extensions,
            excluded_names: DEFAULT_EXCLUDED.iter().map(|s| s.to_lowercase()).collect(),
            exclude_globs,
            respect_gitignore: config
                .get("respectGitignore")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            follow_symlinks: config
                .get("followSymlinks")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub fn is_markdown(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
            .unwrap_or(false)
    }

    /// フォルダ名・ファイル名、またはルートからの相対パスが除外対象かどうか
    pub fn is_excluded(&self, name: &str, rel: &str) -> bool {
        let lower = name.to_lowercase();
        if self.excluded_names.iter().any(|n| *n == lower) {
            return true;
        }
        !self.exclude_globs.is_empty()
            && (self.exclude_globs.is_match(rel) || self.exclude_globs.is_match(name))
    }

    /// 相対パスのいずれかの要素が除外対象か（ファイル監視用。隠しフォルダも除く）
    pub fn is_rel_excluded(&self, rel: &str) -> bool {
        let mut acc = String::new();
        for part in rel.split('/') {
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(part);
            if part.starts_with('.') || self.is_excluded(part, &acc) {
                return true;
            }
        }
        false
    }
}

/// ルートからの相対パス（区切りは `/`）に変換する
pub fn to_rel(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    Some(parts.join("/"))
}

/// 相対パスを正規化する。ルートより上に出る場合は None
pub fn normalize_rel(rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => {
                if p.contains(':') {
                    return None;
                }
                parts.push(p);
            }
        }
    }
    Some(parts.join("/"))
}

/// ルートからの相対パスを絶対パスに解決する。ルートの外は拒否する
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let norm = normalize_rel(rel).ok_or_else(|| format!("不正なパスです: {rel}"))?;
    let mut p = root.to_path_buf();
    for part in norm.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    Ok(p)
}

/// 相対パスの親フォルダ（ルート直下なら空文字列）
pub fn parent_rel(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

pub fn file_name(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[i + 1..]).unwrap_or(rel)
}

pub fn file_stem(rel: &str) -> &str {
    let name = file_name(rel);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

pub fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// `from_dir` から `to` への相対パス（どちらもルートからの相対パス）
pub fn relative(from_dir: &str, to: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let mut common = 0;
    while common < from.len()
        && common < to_parts.len()
        && from[common].eq_ignore_ascii_case(to_parts[common])
    {
        common += 1;
    }
    let mut out: Vec<&str> = std::iter::repeat("..").take(from.len() - common).collect();
    out.extend(&to_parts[common..]);
    out.join("/")
}

pub fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_rel("a/./b/../c").as_deref(), Some("a/c"));
        assert_eq!(normalize_rel("a\\b").as_deref(), Some("a/b"));
        assert_eq!(normalize_rel("../x"), None);
        assert_eq!(normalize_rel("C:/x"), None);
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative("a/b", "a/c/d.md"), "../c/d.md");
        assert_eq!(relative("", "x.md"), "x.md");
        assert_eq!(relative("a", "a/x.md"), "x.md");
    }
}
