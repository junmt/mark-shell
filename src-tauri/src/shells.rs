//! PC 上のシェルの検出

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ShellInfo {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub program: PathBuf,
    #[serde(skip)]
    pub args: Vec<String>,
    #[serde(skip)]
    pub wsl: bool,
}

static SHELLS: OnceLock<Vec<ShellInfo>> = OnceLock::new();

fn sys_root() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
}

fn shell(id: &str, name: &str, program: PathBuf, args: &[&str]) -> ShellInfo {
    ShellInfo {
        id: id.into(),
        name: name.into(),
        program,
        args: args.iter().map(|s| s.to_string()).collect(),
        wsl: false,
    }
}

/// PowerShell の出力を UTF-8 にしてから対話モードに入る
const PS_INIT: &str = "[Console]::OutputEncoding=[Console]::InputEncoding=[System.Text.Encoding]::UTF8";

fn detect() -> Vec<ShellInfo> {
    use std::os::windows::process::CommandExt;
    let mut out = Vec::new();
    if let Ok(p) = which::which("pwsh") {
        out.push(shell("pwsh", "PowerShell 7", p, &["-NoLogo", "-NoExit", "-Command", PS_INIT]));
    }
    let ps = sys_root().join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if ps.exists() {
        out.push(shell("powershell", "Windows PowerShell", ps, &["-NoLogo", "-NoExit", "-Command", PS_INIT]));
    }
    let cmd = std::env::var("ComSpec").map(PathBuf::from).unwrap_or_else(|_| sys_root().join(r"System32\cmd.exe"));
    if cmd.exists() {
        out.push(shell("cmd", "コマンドプロンプト", cmd, &["/k", "chcp 65001 >nul"]));
    }
    let mut bash_candidates = vec![];
    if let Ok(git) = which::which("git") {
        // ...\Git\cmd\git.exe → ...\Git\bin\bash.exe
        if let Some(base) = git.parent().and_then(|p| p.parent()) {
            bash_candidates.push(base.join(r"bin\bash.exe"));
        }
    }
    for env in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        if let Ok(pf) = std::env::var(env) {
            bash_candidates.push(PathBuf::from(&pf).join(r"Git\bin\bash.exe"));
            bash_candidates.push(PathBuf::from(&pf).join(r"Programs\Git\bin\bash.exe"));
        }
    }
    if let Some(bash) = bash_candidates.into_iter().find(|p| p.exists()) {
        out.push(shell("gitbash", "Git Bash", bash, &["--login", "-i"]));
    }
    let wsl = sys_root().join(r"System32\wsl.exe");
    if wsl.exists() {
        if let Ok(o) = std::process::Command::new(&wsl)
            .args(["-l", "-q"])
            .creation_flags(0x0800_0000)
            .output()
        {
            // wsl.exe -l は UTF-16LE で出力する
            let units: Vec<u16> = o.stdout.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let text = String::from_utf16_lossy(&units);
            for name in text.lines().map(|l| l.trim().trim_matches('\0')).filter(|l| !l.is_empty()) {
                let mut s = shell(&format!("wsl:{name}"), &format!("WSL: {name}"), wsl.clone(), &["-d", name]);
                s.wsl = true;
                out.push(s);
            }
        }
    }
    out
}

pub fn all() -> &'static [ShellInfo] {
    SHELLS.get_or_init(detect)
}

/// 指定のシェル。なければ PowerShell 7 → Windows PowerShell → 最初に見つかったもの
pub fn find(id: Option<&str>) -> Option<&'static ShellInfo> {
    let shells = all();
    id.and_then(|id| shells.iter().find(|s| s.id == id))
        .or_else(|| powershell())
        .or_else(|| shells.first())
}

pub fn powershell() -> Option<&'static ShellInfo> {
    let shells = all();
    shells
        .iter()
        .find(|s| s.id == "pwsh")
        .or_else(|| shells.iter().find(|s| s.id == "powershell"))
}

#[tauri::command]
pub async fn list_shells() -> Vec<ShellInfo> {
    tauri::async_runtime::spawn_blocking(|| all().to_vec()).await.unwrap_or_default()
}
