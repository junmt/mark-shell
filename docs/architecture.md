# アーキテクチャ

## 1. 技術スタック

| 層 | 採用技術 | 理由 |
| --- | --- | --- |
| アプリ基盤 | Tauri 2（Rust ＋ WebView2） | Electron より配布サイズとメモリが小さい。WebView2 は Windows 11 に標準搭載（Windows 10 はインストーラーで導入） |
| UI | TypeScript ＋ Svelte | 軽量。画面の数が少ないので十分 |
| エディタ | CodeMirror 6 | Markdown モード、IME 対応、大きなファイルでも速い |
| Markdown 描画 | unified（remark / rehype）＋ remark-gfm ＋ remark-math | GFM 準拠、プラグインで拡張しやすい |
| コードハイライト | Shiki | VS Code と同じ文法定義で高品質 |
| 数式 / 図 | KaTeX / Mermaid | すべて同梱し、オフラインで動かす |
| サニタイズ | DOMPurify | プレビューの XSS 対策 |
| ターミナル | xterm.js（UI）＋ portable-pty（Rust、ConPTY） | Windows ネイティブの疑似端末で PowerShell や CLI ツールが正しく動く |
| ディレクトリ探索 | ignore（Rust、ripgrep のライブラリ） | 並列での再帰探索、`.gitignore` の解釈、除外パターンに対応 |
| 全文検索 | grep-searcher / grep-regex（Rust、ripgrep のライブラリ） | インデックスなしで高速に走査できる |
| ファイル監視 | notify（Rust） | ReadDirectoryChangesW を使う |
| インストーラー | WiX（MSI）/ NSIS | Tauri の標準機能 |
| 自動更新 | Tauri updater（署名つき） | オフライン時は何もしない |

データベース（SQLite、検索インデックスなど）は使わない。

## 2. プロセス構成

```text
┌──────────────── mark-shell.exe（Rust / Tauri）────────────────┐
│                                                                │
│  Core                                                          │
│   ├─ Workspace      ルートディレクトリの探索、ファイル一覧    │
│   │                 （メモリ上のみ）、読み書き、リンク書き換え│
│   ├─ Watcher        ファイル変更の監視 → ファイル一覧と UI へ │
│   ├─ Search         全文検索（その都度走査、結果を逐次通知）  │
│   ├─ PtyManager     ConPTY でシェルを生成・入出力の中継       │
│   └─ AiToolRegistry AI CLI の検出と起動コマンドの組み立て     │
│                                                                │
│        ▲ Tauri IPC（コマンド / イベント）                      │
│        ▼                                                       │
│  WebView2（UI）                                                │
│   ├─ 左ペイン: FileTree / SearchView / Outline                 │
│   └─ 右ペイン: Tabs → Editor (CodeMirror) / Preview (unified)  │
│                        Terminal (xterm.js)                     │
└────────────────────────────────────────────────────────────────┘
          │ 子プロセス
          ▼
   pwsh.exe / cmd.exe / bash.exe / wsl.exe
          └─ claude / gemini / codex など
```

- ファイル操作、プロセス生成はすべて Rust 側で行う。UI からは Tauri のコマンドを通してのみ呼び出す。
- Tauri の capability 設定で、UI から使える API をルートディレクトリ内のファイル操作と定義済みコマンドに絞る。

### 2.1 ファイル一覧の管理

- ディレクトリを開くと Workspace が `ignore` で並列に再帰探索し、見つかったファイルを一定件数ごとに UI へ送る。
- ファイル一覧（パス、名前、更新日時、サイズ）はメモリ上のツリーとして持つ。ディスクには保存しない。
- 以後の追加・削除・名前変更は Watcher のイベントでツリーを差分更新する。
- 監視イベントの取りこぼし（バッファあふれ）を検出したら、そのフォルダ以下を探索し直す。

## 3. データの置き場所

### 3.1 ルートディレクトリ

ユーザーのファイルのみを置く。アプリが勝手にファイルやフォルダを作ることはない（例外はユーザー操作によるもの: 新規ファイル、画像の貼り付けによる `assets/`、AI 向け説明ファイルのひな形作成）。

```text
<ルートディレクトリ>/        … 例: C:\Notes
├─ inbox/
│   └─ memo.md
├─ projects/
│   └─ mark-shell/
│       ├─ 設計メモ.md
│       └─ assets/
│           └─ 設計メモ-1.png
└─ README.md
```

### 3.2 アプリの設定と状態

| 場所 | 内容 |
| --- | --- |
| `%APPDATA%\mark-shell\config.json` | テーマ、フォント、ショートカット、除外パターン、ターミナルの設定 |
| `%APPDATA%\mark-shell\ai-tools.json` | AI ツール定義（[terminal-ai.md](terminal-ai.md)） |
| `%LOCALAPPDATA%\mark-shell\workspaces\<パスのハッシュ>.json` | ディレクトリごとの状態（開いていたタブ、フォルダの開閉、ペインの大きさ） |
| `%LOCALAPPDATA%\mark-shell\recent.json` | 最近開いたディレクトリ |
| `%LOCALAPPDATA%\mark-shell\recovery\` | 異常終了に備えた編集中の内容 |
| `%LOCALAPPDATA%\mark-shell\logs\` | ログ（7 日分をローテーション）。ノートの本文はログに書かない |

いずれも JSON またはプレーンテキストで、消してもアプリは初期状態で起動する。

## 4. 保存とデータ保護

- 保存は「一時ファイルに書く → fsync → 置き換え（MoveFileEx による上書きの名前変更）」で行い、保存中に落ちても元ファイルが壊れないようにする。一時ファイルは対象と同じフォルダに `.<ファイル名>.mark-shell.tmp` という名前で作り（`.` で始まるので探索の対象外）、置き換えた時点でなくなる。
- 編集中の内容は 1 秒ごとに `%LOCALAPPDATA%\mark-shell\recovery\` に書き出し、異常終了後の起動時に復元を提案する。
- 文字コード: 読み込み時に UTF-8（BOM あり / なし）、UTF-16、Shift_JIS を判定する。保存は元の文字コードを保つ（新規作成は UTF-8 BOM なし）。
- 改行コード: 読み込んだファイルの改行コードを保つ（新規作成は LF）。
- 読み取り専用のファイルは編集不可として開き、その旨を表示する。

## 5. ネットワーク利用の一覧

アプリ自身が通信するのは以下だけで、すべて設定で止められる。どれも失敗してもアプリの動作に影響しない。

| 用途 | 通信先 | タイミング |
| --- | --- | --- |
| 更新確認 | リリース配布元 | 起動時に 1 日 1 回（無効にできる） |
| 外部画像の表示 | ファイル内の画像 URL | ユーザーが許可したときのみ |

AI ツールやシェルから実行したコマンドの通信は、アプリではなくそれぞれのプロセスが行う。
