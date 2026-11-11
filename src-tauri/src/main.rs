// リリースビルドでコンソールウィンドウを出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Claude Code のフックから呼ばれたときは、ウィンドウを開かずに文脈を出力して終わる
    if std::env::args().nth(1).as_deref() == Some("--ai-context") {
        mark_shell_lib::print_ai_context();
        return;
    }
    // アンインストーラーから呼ばれたときは、コマンド mark-shell の登録（PATH）を外して終わる
    if std::env::args().nth(1).as_deref() == Some("--unregister-cli") {
        mark_shell_lib::unregister_cli();
        return;
    }
    mark_shell_lib::run()
}
