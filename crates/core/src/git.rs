//! Git 連携。PC にインストール済みの `git` を子プロセスとして実行し、機械が読む形式の出力を解析する。
//! Windows 版と WSL 用エージェントの両方で使う（WSL のフォルダでは WSL の中の git をエージェントが実行する）

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::tr;

/// git の実行結果。stdout はバイト列のまま（ファイルの中身を取り出すときに文字コードを判定するため）
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Output {
    pub code: i32,
    #[serde(with = "b64")]
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// JSON（エージェントとのやり取り）ではバイト列を base64 にする
mod b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).map_err(serde::de::Error::custom)
    }
}

/// 標準入力に渡すバイト列は JSON（エージェントとのやり取り）では base64 にする
pub fn to_b64(v: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(v)
}

pub fn from_b64(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).map_err(|e| e.to_string())
}

/// 実行中の操作（取り消せるもの）。操作の番号 → 取り消しの印
fn running() -> &'static Mutex<HashMap<u64, Arc<AtomicBool>>> {
    static R: OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// 実行中の操作を取り消す（子プロセスを止める）
pub fn cancel(op: u64) {
    if let Some(flag) = running().lock().unwrap().get(&op) {
        flag.store(true, Ordering::SeqCst);
    }
}

/// git を実行する。
/// - `stdin` を渡すと標準入力に書く（コミットメッセージ、blob の中身など）
/// - `op` を渡すと取り消せるようにし、標準エラーの行（進捗）を `on_progress` に渡す
pub fn run(
    git: &Path,
    cwd: &Path,
    args: &[String],
    stdin: Option<&[u8]>,
    op: Option<u64>,
    on_progress: &(dyn Fn(&str) + Sync),
) -> Result<Output, String> {
    let mut cmd = Command::new(git);
    // 表示言語に左右されない出力にする。認証の入力を端末で待たない。エディタは開かない
    cmd.args(["--no-optional-locks", "-c", "core.quotepath=false", "-c", "color.ui=false"])
        .args(args)
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", ":")
        .env("GIT_SEQUENCE_EDITOR", ":")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| {
        let g = git.display();
        tr!("git を起動できません（{g}）: {e}", "Cannot start git ({g}): {e}", "无法启动 git（{g}）：{e}")
    })?;

    if let (Some(data), Some(mut w)) = (stdin, child.stdin.take()) {
        let data = data.to_vec();
        std::thread::spawn(move || {
            let _ = w.write_all(&data);
        });
    }
    let mut out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    // 進捗は \r 区切りで上書きされるので、\r と \n の両方で区切る
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let (progress_tx, progress_rx) = std::sync::mpsc::channel::<String>();
    let sb = stderr_buf.clone();
    let err_thread = std::thread::spawn(move || {
        let mut r = BufReader::new(err);
        let mut line = Vec::new();
        loop {
            let buf = match r.fill_buf() {
                Ok(b) if !b.is_empty() => b.to_vec(),
                _ => break,
            };
            r.consume(buf.len());
            for b in buf {
                if b == b'\r' || b == b'\n' {
                    let s = String::from_utf8_lossy(&line).into_owned();
                    line.clear();
                    if !s.trim().is_empty() {
                        let _ = progress_tx.send(s.clone());
                    }
                    let mut all = sb.lock().unwrap();
                    // 進捗の途中経過（\r）は最後のものだけ残す
                    if b == b'\n' || !s.contains('%') {
                        all.push_str(&s);
                        all.push('\n');
                    }
                } else {
                    line.push(b);
                }
            }
        }
        if !line.is_empty() {
            let mut all = sb.lock().unwrap();
            all.push_str(&String::from_utf8_lossy(&line));
        }
    });

    let flag = Arc::new(AtomicBool::new(false));
    if let Some(id) = op {
        running().lock().unwrap().insert(id, flag.clone());
    }
    let status = loop {
        while let Ok(line) = progress_rx.try_recv() {
            if op.is_some() {
                on_progress(&line);
            }
        }
        if flag.load(Ordering::SeqCst) {
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => break None,
        }
    };
    if let Some(id) = op {
        running().lock().unwrap().remove(&id);
    }
    let stdout = out_thread.join().unwrap_or_default();
    let _ = err_thread.join();
    if flag.load(Ordering::SeqCst) {
        return Err(tr!("取り消しました", "Cancelled", "已取消"));
    }
    let stderr = stderr_buf.lock().unwrap().clone();
    Ok(Output { code: status.and_then(|s| s.code()).unwrap_or(-1), stdout, stderr })
}

// ================= 出力の解析 =================

/// `git status --porcelain=v2 -z --branch` の 1 ファイル
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileStatus {
    pub path: String,
    /// 名前変更・コピーの元のパス
    pub orig: Option<String>,
    /// ステージ（index）の状態: '.' 変更なし、'M' 'A' 'D' 'R' 'C' 'T'、未追跡は '?'、競合は 'U'
    pub x: char,
    /// 作業ツリーの状態
    pub y: char,
    pub conflict: bool,
    pub untracked: bool,
    pub submodule: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BranchStatus {
    /// 現在のコミット。まだコミットがなければ None
    pub oid: Option<String>,
    /// 現在のブランチ。デタッチ状態なら None
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub branch: BranchStatus,
    pub files: Vec<FileStatus>,
    /// 未追跡のファイルが多すぎて打ち切った
    pub truncated: bool,
}

pub const MAX_UNTRACKED: usize = 5000;

pub fn parse_status(raw: &[u8]) -> Status {
    let text = String::from_utf8_lossy(raw);
    let mut parts = text.split('\0');
    let mut st = Status::default();
    let mut untracked = 0usize;
    while let Some(rec) = parts.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(h) = rec.strip_prefix("# ") {
            let (k, v) = h.split_once(' ').unwrap_or((h, ""));
            match k {
                "branch.oid" if v != "(initial)" => st.branch.oid = Some(v.to_string()),
                "branch.head" if v != "(detached)" => st.branch.head = Some(v.to_string()),
                "branch.upstream" => st.branch.upstream = Some(v.to_string()),
                "branch.ab" => {
                    for n in v.split(' ') {
                        if let Some(a) = n.strip_prefix('+') {
                            st.branch.ahead = a.parse().unwrap_or(0);
                        } else if let Some(b) = n.strip_prefix('-') {
                            st.branch.behind = b.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = rec.as_bytes()[0];
        match kind {
            b'1' | b'2' | b'u' => {
                // 1 XY sub mH mI mW hH hI path / 2 ... Xscore path \0 orig / u XY sub m1 m2 m3 mW h1 h2 h3 path
                let fields = match kind {
                    b'1' => 8,
                    b'2' => 9,
                    _ => 10,
                };
                let mut f = rec.splitn(fields + 1, ' ');
                let _ = f.next();
                let xy: Vec<char> = f.next().unwrap_or("..").chars().collect();
                let sub = f.next().unwrap_or("N...");
                let path = f.nth(fields - 3).unwrap_or("").to_string();
                let orig = if kind == b'2' { parts.next().map(str::to_string) } else { None };
                st.files.push(FileStatus {
                    path,
                    orig,
                    x: if kind == b'u' { 'U' } else { *xy.first().unwrap_or(&'.') },
                    y: if kind == b'u' { 'U' } else { *xy.get(1).unwrap_or(&'.') },
                    conflict: kind == b'u',
                    untracked: false,
                    submodule: sub.starts_with('S'),
                });
            }
            b'?' => {
                untracked += 1;
                if untracked > MAX_UNTRACKED {
                    st.truncated = true;
                    continue;
                }
                st.files.push(FileStatus {
                    path: rec[2..].to_string(),
                    orig: None,
                    x: '?',
                    y: '?',
                    conflict: false,
                    untracked: true,
                    submodule: false,
                });
            }
            _ => {}
        }
    }
    st
}

/// 履歴の 1 コミット
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// UNIX 時刻（秒）
    pub date: i64,
    /// ブランチ名・タグ名（`HEAD -> main`、`origin/main`、`tag: v1`）
    pub refs: Vec<String>,
    pub subject: String,
}

/// `git log` に渡す書式。parse_log と対にする
pub const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%D%x1f%s%x1e";

pub fn parse_log(raw: &[u8]) -> Vec<Commit> {
    String::from_utf8_lossy(raw)
        .split('\x1e')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches(['\n', '\r']);
            if rec.is_empty() {
                return None;
            }
            let f: Vec<&str> = rec.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Commit {
                hash: f[0].to_string(),
                parents: f[1].split(' ').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                author: f[2].to_string(),
                email: f[3].to_string(),
                date: f[4].parse().unwrap_or(0),
                refs: f[5].split(", ").filter(|s| !s.is_empty()).map(str::to_string).collect(),
                subject: f[6].to_string(),
            })
        })
        .collect()
}

/// `--name-status -z` の 1 ファイル
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChangedFile {
    /// 'A' 'M' 'D' 'R' 'C' 'T'
    pub status: char,
    pub path: String,
    pub orig: Option<String>,
}

pub fn parse_name_status(raw: &[u8]) -> Vec<ChangedFile> {
    let text = String::from_utf8_lossy(raw);
    let mut parts = text.split('\0').filter(|s| !s.is_empty());
    let mut out = Vec::new();
    while let Some(code) = parts.next() {
        let code = code.trim_start_matches(['\n', '\r']);
        let Some(status) = code.chars().next() else { continue };
        if !status.is_ascii_uppercase() {
            continue;
        }
        if status == 'R' || status == 'C' {
            let orig = parts.next().map(str::to_string);
            let path = parts.next().unwrap_or("").to_string();
            out.push(ChangedFile { status, path, orig });
        } else {
            let path = parts.next().unwrap_or("").to_string();
            out.push(ChangedFile { status, path, orig: None });
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub hash: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// 上流のブランチがリモートから消えている
    pub gone: bool,
    pub current: bool,
    pub subject: String,
    pub date: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBranch {
    pub remote: String,
    /// リモートの中での名前（`main`）
    pub name: String,
    pub hash: String,
    pub subject: String,
    pub date: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Refs {
    pub branches: Vec<Branch>,
    pub remote_branches: Vec<RemoteBranch>,
    pub tags: Vec<Tag>,
}

/// `git for-each-ref` に渡す書式。parse_refs と対にする
pub const REFS_FORMAT: &str =
    "--format=%(refname)%1f%(objectname)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(HEAD)%1f%(*objectname)%1f%(committerdate:unix)%1f%(contents:subject)%1e";

/// リモート名に `/` を含むこともあるので、リモートの一覧を使って分ける
pub fn parse_refs(raw: &[u8], remotes: &[String]) -> Refs {
    let mut refs = Refs::default();
    for rec in String::from_utf8_lossy(raw).split('\x1e') {
        let rec = rec.trim_start_matches(['\n', '\r']);
        let f: Vec<&str> = rec.split('\x1f').collect();
        if f.len() < 8 {
            continue;
        }
        let date = f[6].parse().unwrap_or(0);
        if let Some(name) = f[0].strip_prefix("refs/heads/") {
            let mut b = Branch {
                name: name.into(),
                hash: f[1].into(),
                upstream: Some(f[2].to_string()).filter(|s| !s.is_empty()),
                current: f[4] == "*",
                subject: f[7].into(),
                date,
                ..Default::default()
            };
            for part in f[3].split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    b.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    b.behind = n.parse().unwrap_or(0);
                } else if part == "gone" {
                    b.gone = true;
                }
            }
            refs.branches.push(b);
        } else if let Some(rest) = f[0].strip_prefix("refs/remotes/") {
            if rest.ends_with("/HEAD") {
                continue;
            }
            let remote = remotes
                .iter()
                .filter(|r| rest.starts_with(&format!("{r}/")))
                .max_by_key(|r| r.len())
                .cloned()
                .unwrap_or_else(|| rest.split('/').next().unwrap_or("").to_string());
            let name = rest[remote.len()..].trim_start_matches('/').to_string();
            refs.remote_branches.push(RemoteBranch { remote, name, hash: f[1].into(), subject: f[7].into(), date });
        } else if let Some(name) = f[0].strip_prefix("refs/tags/") {
            // 注釈付きタグはタグが指すコミット（*objectname）を使う
            let hash = if f[5].is_empty() { f[1] } else { f[5] };
            refs.tags.push(Tag { name: name.into(), hash: hash.into() });
        }
    }
    refs
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    pub index: u32,
    pub message: String,
    pub date: i64,
}

pub const STASH_FORMAT: &str = "--format=%gd%x1f%ct%x1f%gs%x1e";

pub fn parse_stashes(raw: &[u8]) -> Vec<Stash> {
    String::from_utf8_lossy(raw)
        .split('\x1e')
        .filter_map(|rec| {
            let f: Vec<&str> = rec.trim_start_matches(['\n', '\r']).split('\x1f').collect();
            if f.len() < 3 {
                return None;
            }
            let index = f[0].trim_start_matches("stash@{").trim_end_matches('}').parse().ok()?;
            Some(Stash { index, date: f[1].parse().unwrap_or(0), message: f[2].to_string() })
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

/// `git remote -v`
pub fn parse_remotes(raw: &[u8]) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in String::from_utf8_lossy(raw).lines() {
        let mut f = line.split_whitespace();
        let (Some(name), Some(url)) = (f.next(), f.next()) else { continue };
        if !out.iter().any(|r| r.name == name) {
            out.push(Remote { name: name.into(), url: url.into() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status() {
        let raw = b"# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +1 -2\0\
1 .M N... 100644 100644 100644 a b README.md\0\
2 R. N... 100644 100644 100644 a b R100 new name.md\0old.md\0\
u UU N... 100644 100644 100644 100644 a b c conflict.md\0\
? memo.md\0";
        let s = parse_status(raw);
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (1, 2));
        assert_eq!(s.files.len(), 4);
        assert_eq!(s.files[0].path, "README.md");
        assert_eq!((s.files[0].x, s.files[0].y), ('.', 'M'));
        assert_eq!(s.files[1].path, "new name.md");
        assert_eq!(s.files[1].orig.as_deref(), Some("old.md"));
        assert!(s.files[2].conflict);
        assert_eq!(s.files[2].path, "conflict.md");
        assert!(s.files[3].untracked);
    }

    #[test]
    fn initial() {
        let s = parse_status(b"# branch.oid (initial)\0# branch.head main\0");
        assert!(s.branch.oid.is_none());
        assert_eq!(s.branch.head.as_deref(), Some("main"));
    }

    #[test]
    fn log() {
        let raw = b"h1\x1fp1 p2\x1fJun\x1fj@x\x1f100\x1fHEAD -> main, origin/main, tag: v1\x1fMerge it\x1e\nh2\x1f\x1fJun\x1fj@x\x1f50\x1f\x1fFirst\x1e\n";
        let c = parse_log(raw);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["p1", "p2"]);
        assert_eq!(c[0].refs, vec!["HEAD -> main", "origin/main", "tag: v1"]);
        assert!(c[1].parents.is_empty());
        assert!(c[1].refs.is_empty());
    }

    #[test]
    fn name_status() {
        let f = parse_name_status(b"M\0a.md\0R100\0old.md\0new.md\0D\0gone.md\0");
        assert_eq!(f.len(), 3);
        assert_eq!(f[1], ChangedFile { status: 'R', path: "new.md".into(), orig: Some("old.md".into()) });
        assert_eq!(f[2].status, 'D');
    }

    #[test]
    fn refs() {
        let raw = b"refs/heads/main\x1fh1\x1forigin/main\x1fahead 1, behind 2\x1f*\x1f\x1f10\x1fmsg\x1e\n\
refs/remotes/origin/HEAD\x1fh1\x1f\x1f\x1f \x1f\x1f10\x1fmsg\x1e\n\
refs/remotes/my/fork/feature/x\x1fh2\x1f\x1f\x1f \x1f\x1f10\x1fmsg2\x1e\n\
refs/tags/v1\x1ft1\x1f\x1f\x1f \x1fh1\x1f10\x1frel\x1e\n";
        let r = parse_refs(raw, &["origin".into(), "my/fork".into()]);
        assert_eq!(r.branches.len(), 1);
        assert!(r.branches[0].current);
        assert_eq!((r.branches[0].ahead, r.branches[0].behind), (1, 2));
        assert_eq!(r.remote_branches.len(), 1);
        assert_eq!(r.remote_branches[0].remote, "my/fork");
        assert_eq!(r.remote_branches[0].name, "feature/x");
        assert_eq!(r.tags[0].hash, "h1");
    }

    #[test]
    fn stashes() {
        let s = parse_stashes(b"stash@{0}\x1f100\x1fOn main: wip\x1e\nstash@{1}\x1f90\x1fWIP on main: abc\x1e\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].index, 1);
        assert_eq!(s[0].message, "On main: wip");
    }

    #[test]
    fn run_git() {
        let out = run(Path::new("git"), Path::new("."), &["--version".into()], None, None, &|_| {}).unwrap();
        assert!(out.ok());
        assert!(out.text().starts_with("git version"));
    }
}
