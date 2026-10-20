//! 全文検索。インデックスは作らず、その都度ファイルを走査して見つかった順に送る

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::WalkState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::paths::{normalize_rel, resolve, to_rel};
use crate::state::AppState;
use crate::textio;
use crate::workspace::walker;

const MAX_RESULTS: usize = 5_000;
const MAX_LINE_CHARS: usize = 400;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
    /// 絞り込むフォルダ（ルートからの相対パス）
    #[serde(default)]
    pub folder: String,
}

#[derive(Serialize, Clone)]
struct Hit {
    path: String,
    line: u64,
    text: String,
    /// UTF-16 単位のオフセット（JavaScript の文字列位置）
    ranges: Vec<(usize, usize)>,
}

#[derive(Serialize, Clone)]
struct SearchBatch {
    id: u64,
    hits: Vec<Hit>,
}

#[derive(Serialize, Clone)]
struct SearchDone {
    id: u64,
    total: usize,
    truncated: bool,
    error: Option<String>,
}

fn build_matcher(o: &SearchOptions) -> Result<RegexMatcher, String> {
    RegexMatcherBuilder::new()
        .case_insensitive(!o.case_sensitive)
        .word(o.whole_word)
        .fixed_strings(!o.regex)
        .line_terminator(Some(b'\n'))
        .build(&o.query)
        .map_err(|e| format!("検索条件が正しくありません: {e}"))
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn make_hit(path: &str, lnum: u64, line: &str, matcher: &RegexMatcher) -> Hit {
    let line = line.trim_end_matches(['\n', '\r']);
    let mut ranges = Vec::new();
    let _ = matcher.find_iter(line.as_bytes(), |m| {
        ranges.push((m.start(), m.end()));
        true
    });
    // 長い行は最初の一致の手前から切り出す
    let mut start = 0;
    if line.chars().count() > MAX_LINE_CHARS {
        if let Some(&(s, _)) = ranges.first() {
            let mut back = 0;
            start = s;
            for (i, _) in line[..s].char_indices().rev() {
                back += 1;
                start = i;
                if back >= 40 {
                    break;
                }
            }
        }
    }
    let mut text: String = line[start..].chars().take(MAX_LINE_CHARS).collect();
    let end = start + text.len();
    let ranges = ranges
        .into_iter()
        .filter(|&(s, e)| s >= start && e <= end)
        .map(|(s, e)| (utf16_len(&line[start..s]), utf16_len(&line[start..e])))
        .collect();
    if start > 0 {
        text.insert(0, '…');
    }
    Hit { path: path.to_string(), line: lnum, text, ranges: shift(ranges, start > 0) }
}

fn shift(ranges: Vec<(usize, usize)>, prefixed: bool) -> Vec<(usize, usize)> {
    if prefixed {
        ranges.into_iter().map(|(s, e)| (s + 1, e + 1)).collect()
    } else {
        ranges
    }
}

#[tauri::command]
pub fn search_start(app: AppHandle, state: State<'_, AppState>, options: SearchOptions) -> Result<u64, String> {
    let root = state.root()?;
    let filter = state.filter();
    let id = state.search_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let gen = state.search_gen.clone();
    if options.query.is_empty() {
        return Ok(id);
    }
    let matcher = match build_matcher(&options) {
        Ok(m) => m,
        Err(e) => {
            let _ = app.emit("search:done", SearchDone { id, total: 0, truncated: false, error: Some(e) });
            return Ok(id);
        }
    };
    let start_dir = resolve(&root, &options.folder)?;

    std::thread::spawn(move || {
        let total = AtomicUsize::new(0);
        let buf: Mutex<(Vec<Hit>, Instant)> = Mutex::new((Vec::new(), Instant::now()));
        let flush = |force: bool| {
            let mut b = buf.lock().unwrap();
            if !b.0.is_empty() && (force || b.0.len() >= 200 || b.1.elapsed() > Duration::from_millis(60)) {
                let hits = std::mem::take(&mut b.0);
                b.1 = Instant::now();
                let _ = app.emit("search:batch", SearchBatch { id, hits });
            }
        };
        let cancelled = || gen.load(Ordering::Relaxed) != id;

        walker(&root, &start_dir, &filter).build_parallel().run(|| {
            let matcher = matcher.clone();
            let mut searcher = SearcherBuilder::new()
                .line_number(true)
                .binary_detection(BinaryDetection::quit(b'\x00'))
                .build();
            let root = &root;
            let filter = &filter;
            let total = &total;
            let buf = &buf;
            let flush = &flush;
            let cancelled = &cancelled;
            Box::new(move |res| {
                if cancelled() || total.load(Ordering::Relaxed) >= MAX_RESULTS {
                    return WalkState::Quit;
                }
                let Ok(e) = res else { return WalkState::Continue };
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) || !filter.is_listed(e.path()) {
                    return WalkState::Continue;
                }
                let Some(rel) = to_rel(root, e.path()) else { return WalkState::Continue };
                let mut hits = Vec::new();
                let _ = searcher.search_path(
                    &matcher,
                    e.path(),
                    UTF8(|lnum, line| {
                        hits.push(make_hit(&rel, lnum, line, &matcher));
                        Ok(hits.len() < 500)
                    }),
                );
                if !hits.is_empty() {
                    let n = total.fetch_add(hits.len(), Ordering::Relaxed) + hits.len();
                    buf.lock().unwrap().0.extend(hits);
                    flush(n >= MAX_RESULTS);
                }
                WalkState::Continue
            })
        });
        if cancelled() {
            return;
        }
        flush(true);
        let t = total.load(Ordering::Relaxed);
        let _ = app.emit("search:done", SearchDone { id, total: t.min(MAX_RESULTS), truncated: t >= MAX_RESULTS, error: None });
    });
    Ok(id)
}

#[tauri::command]
pub fn search_cancel(state: State<'_, AppState>) {
    state.search_gen.fetch_add(1, Ordering::SeqCst);
}

#[derive(Serialize)]
pub struct ReplaceResult {
    files: Vec<String>,
    replacements: usize,
}

fn build_regex(o: &SearchOptions) -> Result<regex::Regex, String> {
    let mut pat = if o.regex { o.query.clone() } else { regex::escape(&o.query) };
    if o.whole_word {
        pat = format!(r"\b(?:{pat})\b");
    }
    regex::RegexBuilder::new(&pat)
        .case_insensitive(!o.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|e| format!("検索条件が正しくありません: {e}"))
}

/// 検索結果のファイルに一括置換をかける（文字コード・改行コードは保つ）
#[tauri::command]
pub async fn replace_in_files(app: AppHandle, options: SearchOptions, replacement: String, paths: Vec<String>) -> Result<ReplaceResult, String> {
    let root = app.state::<AppState>().root()?;
    tauri::async_runtime::spawn_blocking(move || replace_blocking(&root, &options, &replacement, &paths))
        .await
        .map_err(|e| e.to_string())?
}

fn replace_blocking(root: &Path, o: &SearchOptions, replacement: &str, paths: &[String]) -> Result<ReplaceResult, String> {
    let re = build_regex(o)?;
    let mut files = Vec::new();
    let mut replacements = 0;
    for rel in paths {
        let rel = normalize_rel(rel).ok_or("不正なパスです")?;
        let p = resolve(root, &rel)?;
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let d = textio::decode(&bytes);
        let lines_changed: usize = d.text.lines().map(|l| re.find_iter(l).count()).sum();
        if lines_changed == 0 {
            continue;
        }
        let out = if o.regex {
            d.text.lines().map(|l| re.replace_all(l, replacement).into_owned()).collect::<Vec<_>>()
        } else {
            d.text.lines().map(|l| re.replace_all(l, regex::NoExpand(replacement)).into_owned()).collect::<Vec<_>>()
        };
        let mut text = out.join("\n");
        if d.text.ends_with('\n') {
            text.push('\n');
        }
        let bytes = textio::encode(&text, &d.encoding, d.eol).or_else(|_| textio::encode(&text, "utf-8", d.eol))?;
        textio::write_atomic(&p, &bytes).map_err(|e| e.to_string())?;
        replacements += lines_changed;
        files.push(rel);
    }
    Ok(ReplaceResult { files, replacements })
}
