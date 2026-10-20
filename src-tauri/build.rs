fn main() {
    // WSL 用エージェント（crates/agent を Linux 向けにビルドしたもの）があれば埋め込む。
    // `npm run agent` で作る。なければ WSL のフォルダは \\wsl.localhost 経由だけで扱う
    println!("cargo::rustc-check-cfg=cfg(has_wsl_agent)");
    let agent = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/agent/target/x86_64-unknown-linux-musl/release/mark-shell-agent");
    println!("cargo::rerun-if-changed={}", agent.display());
    if agent.exists() {
        let agent = agent.canonicalize().unwrap();
        println!("cargo::rustc-cfg=has_wsl_agent");
        println!("cargo::rustc-env=MARKSHELL_AGENT_BIN={}", agent.display());
    }
    tauri_build::build()
}
