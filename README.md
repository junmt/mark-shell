# mark-shell

指定したフォルダの中の Markdown ファイルを左のツリーで一覧し、右のエディタで編集・表示する Windows アプリです。内蔵ターミナルから Claude Code などの AI ツールを起動できます。仕様は [docs/](docs/README.md) を参照してください。

## 必要なもの

- Windows 10 / 11
- [Node.js](https://nodejs.org/) 20 以降
- [Rust](https://www.rust-lang.org/)（`rustup` で stable）
- Visual Studio の「C++ によるデスクトップ開発」（MSVC と Windows SDK）
- WebView2 ランタイム（Windows 11 は標準で入っています）

プロジェクトは Google ドライブなどの同期フォルダではなく、ローカルディスクに置いてください。同期フォルダ上では `npm install` が失敗します。

## 開発

```powershell
npm install
npm run tauri dev          # 開発モードで起動（画面の変更は即時反映）
```

ビルドした実行ファイルは `mark-shell.exe C:\Notes` のようにフォルダを指定して起動できます。

## テスト

```powershell
npm test                   # 画面側（パス計算・ツリー・表の整形・Markdown 描画）
npm run check              # 型チェック
cd src-tauri; cargo test   # Rust 側（パス・文字コード・リンク書き換え・AI ツール定義）
```

## 配布用のビルド

```powershell
npm run tauri build        # src-tauri/target/release/bundle/ に MSI と NSIS のインストーラーができる
```

## 構成

| 場所 | 内容 |
| --- | --- |
| `src/` | 画面（Svelte 5 + TypeScript） |
| `src/lib/store.svelte.ts` | アプリ全体の状態（タブ、保存、外部変更の取り込み、ファイル操作、検索、ターミナル） |
| `src/lib/markdown.ts` | Markdown → HTML（GFM、数式、Mermaid、`[[内部リンク]]`、サニタイズ） |
| `src/lib/editor.ts` | CodeMirror 6 の設定と編集コマンド |
| `src-tauri/src/workspace.rs` | フォルダの再帰探索とファイル操作 |
| `src-tauri/src/watcher.rs` | ファイル変更の監視 |
| `src-tauri/src/search.rs` | 全文検索（インデックスなし） |
| `src-tauri/src/links.rs` | 名前変更・移動に合わせたリンクの書き換え |
| `src-tauri/src/pty.rs` | ConPTY によるターミナル |
| `src-tauri/src/ai_tools.rs` | AI ツールの検出と起動 |

## 仕様のうち未実装のもの

- ターミナルのタブを左右・上下に分割する
- HTML へのエクスポート（印刷から「Microsoft Print to PDF」を選べば PDF にはできる）
- ツリーの表示名をフロントマターの `title` や最初の見出しにする設定
- 自動更新
- AI による変更の差分を Git の履歴から出す（いまは開いていたファイルについて、アプリが覚えている変更前の内容と比べる）
- Shift_JIS など UTF-8 以外のファイルの全文検索（開く・編集・保存はできる）

## データの置き場所

アプリは開いたフォルダの中に設定ファイルを作りません。

| 場所 | 内容 |
| --- | --- |
| `%APPDATA%\mark-shell\config.json` | 設定 |
| `%APPDATA%\mark-shell\ai-tools.json` | AI ツールの定義（ツールの追加はここに書く） |
| `%LOCALAPPDATA%\mark-shell\` | フォルダごとの状態、最近開いたフォルダ、異常終了時の復元データ |
