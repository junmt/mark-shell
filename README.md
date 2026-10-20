# mark-shell

A Windows app that lists the files in a folder in a tree on the left and lets you edit Markdown and other text files on the right, with a live preview for Markdown. A built-in terminal launches AI tools such as Claude Code, which are told which file you have open. Folders inside WSL are supported: like VS Code's WSL extension, the terminal and AI tools then run inside WSL. The specifications are in [docs/](docs/README.md) (in Japanese).

## Requirements

- Windows 10 / 11
- [Node.js](https://nodejs.org/) 20 or later
- [Rust](https://www.rust-lang.org/) (stable, via `rustup`) and the Linux target used to build the WSL agent (`rustup target add x86_64-unknown-linux-musl`)
- Visual Studio "Desktop development with C++" (MSVC and the Windows SDK)
- WebView2 Runtime (included with Windows 11)

Keep the project on a local disk, not in a synced folder such as Google Drive. `npm install` fails in synced folders.

## Development

```powershell
npm install
npm run tauri dev          # Start in development mode (UI changes reload instantly). Builds the WSL agent first
```

A built executable can be started with a folder, e.g. `mark-shell.exe C:\Notes`.

## Tests

```powershell
npm test                   # UI (path handling, tree, table formatting, Markdown rendering, color themes)
npm run check              # Type checking
cd crates/core; cargo test # Shared core (paths, text encodings, link rewriting)
cd src-tauri; cargo test   # App (AI tool definitions, context, WSL paths)
```

## Building for distribution

```powershell
npm run tauri build        # Creates the MSI and NSIS installers in src-tauri/target/release/bundle/
```

Pushing a tag of the form `v1.2.3` to GitHub makes GitHub Actions (`.github/workflows/release.yml`) run the tests, build the installers (`-setup.exe` and `.msi`) and a standalone executable (`-portable.exe`), and publish them on Releases. The app version is taken from the tag. The release also includes signed update files and `latest.json`, which installed copies use to update themselves.

```powershell
git tag v0.2.0
git push origin v0.2.0
```

## Layout

| Path | Contents |
| --- | --- |
| `src/` | UI (Svelte 5 + TypeScript) |
| `src/lib/store.svelte.ts` | App state (tabs, saving, picking up external changes, file operations, search, terminal) |
| `src/lib/markdown.ts` | Markdown → HTML (GFM, math, Mermaid, `[[wiki links]]`, sanitizing) |
| `src/lib/editor.ts` | CodeMirror 6 setup and editing commands |
| `src/lib/themes.ts` | Color theme presets and custom colors |
| `crates/core/` | Code shared by the Windows app and the WSL agent (scanning, search, watching, link rewriting, text encodings) |
| `crates/agent/` | Agent that runs inside WSL. Built on Windows for Linux (musl) and embedded in the app |
| `src-tauri/src/workspace.rs` | Opening folders, file operations |
| `src-tauri/src/watcher.rs` | Watching for file changes |
| `src-tauri/src/search.rs` | Full-text search (no index) |
| `src-tauri/src/wsl.rs` | WSL integration (deploying, starting and talking to the agent) |
| `src-tauri/src/pty.rs` | Terminal via ConPTY |
| `src-tauri/src/ai_tools.rs` | Detecting and launching AI tools |
| `src-tauri/src/ai_context.rs` | Telling Claude Code which file is open (via a hook) |

## Not yet implemented

- Splitting terminal tabs side by side or top and bottom
- Exporting to HTML (you can make a PDF by printing to "Microsoft Print to PDF")
- Showing the front matter `title` or the first heading as the name in the tree
- Showing diffs of AI changes from Git history (currently the app compares open files with the content it remembers from before the change)
- Full-text search in files that are not UTF-8, such as Shift_JIS (opening, editing and saving them works)

## Where data is stored

The app does not create settings files inside the folders you open.

| Path | Contents |
| --- | --- |
| `%APPDATA%\mark-shell\config.json` | Settings |
| `%APPDATA%\mark-shell\ai-tools.json` | AI tool definitions (add your own tools here) |
| `%LOCALAPPDATA%\mark-shell\` | Per-folder state, recently opened folders, crash recovery data, Claude Code hook settings |
| `~/.mark-shell/` (inside WSL) | The WSL agent, placed there the first time you open a WSL folder |
