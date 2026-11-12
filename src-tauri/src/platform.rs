//! OS ごとの違いを吸収する小さな処理

use std::process::Command;

/// コンソールのウィンドウを出さずに起動する（Windows だけ。ほかの OS では何もしない）
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// アプリから切り離して起動する（アプリを閉じても終わらない）
pub fn detach(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0008); // CREATE_NO_WINDOW | DETACHED_PROCESS
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// Mac で Finder や Dock から起動したアプリには、最小限の PATH（/usr/bin:/bin:...）しか渡されない。
/// Homebrew や npm で入れたコマンド（claude, git など）を見つけられるよう、ログインシェルの PATH を引き継ぐ
#[cfg(target_os = "macos")]
pub fn inherit_login_path() {
    const MARK: &str = "__MARKSHELL_PATH__";
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
    let Ok(out) = Command::new(&shell)
        .args(["-ilc", &format!("printf '%s' \"{MARK}$PATH{MARK}\"")])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(path) = text.split(MARK).nth(1).filter(|p| !p.is_empty()) else {
        return;
    };
    std::env::set_var("PATH", path);
}
