//! mark-shell の Windows 版と WSL 用エージェントで共有する処理。
//! Tauri に依存せず、Windows と Linux のどちらでもビルドできる

pub mod links;
pub mod paths;
pub mod scan;
pub mod search;
pub mod textio;
pub mod watch;
