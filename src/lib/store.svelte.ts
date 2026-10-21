// アプリ全体の状態

import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { check as checkUpdate, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';
import { escapeHtml, renderMarkdown } from './markdown';
import { EditorState } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import {
  api,
  NOT_TEXT_PREFIX,
  errorMessage,
  type Change,
  type Entry,
  type ReadResult,
  type SearchHit,
  type SearchOptions,
  type AiSelection,
  type ShellInfo,
  type SpawnRequest,
  type ToolInfo,
  type WslInfo,
} from './api';
import { defaultConfig, mergeConfig, type Config } from './config';
import { resolveTheme } from './themes';
import { i18n, resolveLocale, tr } from './i18n.svelte';
import { absToRel, basename, dirname, encodeLinkTarget, extname, isImage, isInside, join, normalize, rebase, relative } from './paths';
import { buildTree, flatten, type TreeNode } from './tree';

const RELEASES_URL = 'https://github.com/junmt/mark-shell/releases/latest';

export type Mode = 'edit' | 'preview' | 'split';
export type LeftView = 'files' | 'search' | 'outline';

export class Doc {
  path = $state('');
  /** 編集中の本文 */
  text = $state('');
  /** ディスク上の本文（最後に読み込んだ・保存した内容） */
  saved = $state('');
  readonly = $state(false);
  pinned = $state(false);
  mode = $state<Mode>('edit');
  deleted = $state(false);
  encoding = 'utf-8';
  eol: 'lf' | 'crlf' = 'lf';
  /** タブを切り替えても元に戻せるよう、エディタの状態をタブごとに持つ */
  cm: EditorState | null = null;
  scrollTop = 0;
  saving: Promise<void> | null = null;
  /** 表示用（saving はリアクティブでないため） */
  busy = $state(false);
  autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  recoveryTimer: ReturnType<typeof setTimeout> | null = null;
  dirty = $derived(this.text !== this.saved || this.deleted);

  constructor(path: string, r: ReadResult) {
    this.path = path;
    this.text = r.content;
    this.saved = r.content;
    this.readonly = r.readonly;
    this.encoding = r.encoding;
    this.eol = r.eol;
  }
}

export class TermSession {
  key: number;
  title = $state('');
  exited = $state(false);
  ptyId = $state<number | null>(null);
  ai: boolean;
  request: Omit<SpawnRequest, 'cols' | 'rows'>;
  constructor(key: number, title: string, request: Omit<SpawnRequest, 'cols' | 'rows'>, ai: boolean) {
    this.key = key;
    this.title = title;
    this.request = request;
    this.ai = ai;
  }
}

export type MenuItem = { label: string; action?: () => void; shortcut?: string; disabled?: boolean; separator?: boolean };
export type DialogButton = { label: string; value: string; primary?: boolean; danger?: boolean };
export type DialogRequest = {
  title: string;
  message?: string;
  input?: { value: string; placeholder?: string; selectStem?: boolean };
  buttons: DialogButton[];
  resolve: (r: { button: string; value: string } | null) => void;
};
export type Toast = { id: number; message: string; kind: 'info' | 'error' };
export type Command = { id: string; label: string; shortcut?: string; run: () => void };

class Store {
  // ---- ルートとファイル一覧 ----
  root = $state<string | null>(null);
  rootName = $state('');
  scanTruncated = $state(false);
  /** 最新の走査の世代と、走査を終えた世代。違っていれば読み込み中 */
  scanGen = $state(0);
  scanDoneGen = $state(0);
  scanning = $derived(this.scanDoneGen < this.scanGen);
  entries = new Map<string, Entry>();
  treeVersion = $state(0);
  expanded = new SvelteSet<string>();
  keepDirs = new SvelteSet<string>();
  selected = $state<string | null>(null);
  /** 外からファイルをドラッグしている間、コピー先になるツリーのフォルダ（ルートは ''、ツリーの外なら null） */
  fileDropDir = $state<string | null>(null);
  recent = $state<string[]>([]);
  /** WSL のフォルダを開いているとき（ターミナルと AI ツールは WSL の中で動く） */
  wsl = $state<WslInfo | null>(null);

  // ---- タブ ----
  docs = $state<Doc[]>([]);
  activePath = $state<string | null>(null);
  cursor = $state({ line: 1, col: 1 });
  view: EditorView | null = null;
  pendingReveal: { path: string; line: number } | null = null;
  revealSeq = $state(0);

  // ---- レイアウト ----
  leftVisible = $state(true);
  leftView = $state<LeftView>('files');
  leftWidth = $state(260);
  terminalVisible = $state(false);
  terminalHeight = $state(260);

  // ---- 設定 ----
  config = $state<Config>(structuredClone(defaultConfig));
  systemDark = $state(false);
  /** 配色（プリセットかカスタム）から決めた実際の色 */
  colors = $derived(resolveTheme(this.config.theme, this.config.customColors, this.systemDark));
  dark = $derived(this.colors.dark);

  // ---- 検索 ----
  searchOptions = $state<SearchOptions>({ query: '', caseSensitive: false, wholeWord: false, regex: false, folder: '' });
  searchHits = $state<SearchHit[]>([]);
  searchRunning = $state(false);
  searchError = $state<string | null>(null);
  searchTruncated = $state(false);
  searchId = 0;

  // ---- ターミナルと AI ----
  terminals = $state<TermSession[]>([]);
  activeTerminal = $state<number | null>(null);
  termSeq = 0;
  shells = $state<ShellInfo[]>([]);
  aiTools = $state<ToolInfo[]>([]);
  /** AI セッション中に変わったファイル → 変わる前の内容（開いていたものだけ） */
  aiChanged = new SvelteSet<string>();
  aiBefore = new SvelteMap<string, string>();

  // ---- 画面の部品 ----
  dialog = $state<DialogRequest | null>(null);
  toasts = $state<Toast[]>([]);
  menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  palette = $state<'files' | 'commands' | 'folders' | null>(null);
  settingsOpen = $state(false);
  conflict = $state<{ doc: Doc; disk: ReadResult } | null>(null);
  diff = $state<{ title: string; before: string; after: string } | null>(null);
  printHtml = $state<string | null>(null);

  private toastSeq = 0;
  private wsSaveTimer: ReturnType<typeof setTimeout> | null = null;
  private aiContextTimer: ReturnType<typeof setTimeout> | null = null;

  get activeDoc(): Doc | null {
    return this.docs.find((d) => d.path === this.activePath) ?? null;
  }

  get tree(): TreeNode {
    void this.treeVersion;
    return buildTree(this.entries.values(), {
      // 空のフォルダも含めてすべて出す
      showEmptyDirs: true,
      sort: this.config.treeSort,
      keepDirs: this.keepDirs,
    });
  }

  rows = $derived(flatten(this.tree, this.expanded));

  get aiRunning(): boolean {
    return this.terminals.some((t) => t.ai && !t.exited);
  }

  /** Markdown として扱うファイルか。それ以外（ログなどのテキストファイル）はプレビューなしで編集だけできる */
  isMarkdown(path: string): boolean {
    const ext = extname(path).replace(/^\./, '').toLowerCase();
    return this.config.markdownExtensions.some((x) => x.toLowerCase() === ext);
  }

  /** 開けるファイルの一覧（リンク補完用） */
  markdownFiles(): string[] {
    const out: string[] = [];
    for (const e of this.entries.values()) if (!e.dir) out.push(e.path);
    return out;
  }

  // ================= 起動 =================

  async init() {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    this.systemDark = mq.matches;
    mq.addEventListener('change', (e) => (this.systemDark = e.matches));
    this.config = mergeConfig(await api.getConfig());
    await this.applyLanguage();

    // 走査結果は世代番号の新しいものだけを採る。invoke の戻り値より先に届くことがあるので、
    // ここで新しい世代を見つけたら一覧を作り直す
    await listen<{ gen: number; entries: Entry[] }>('scan:batch', (e) => {
      if (!this.adoptScanGen(e.payload.gen)) return;
      for (const en of e.payload.entries) this.entries.set(en.path, en);
      this.treeVersion++;
    });
    await listen<{ gen: number; truncated: boolean; files: number }>('scan:done', (e) => {
      if (!this.adoptScanGen(e.payload.gen)) return;
      this.scanDoneGen = e.payload.gen;
      this.scanTruncated = e.payload.truncated;
      if (e.payload.truncated) {
        const n = e.payload.files.toLocaleString();
        this.toast(
          tr(
            `ファイルが多すぎるため ${n} 件で探索を止めました。中のフォルダを開くなど、より狭い範囲を開いてください。`,
            `Too many files. Stopped scanning at ${n} files. Open a narrower folder, such as a subfolder.`,
            `文件过多，已在 ${n} 个时停止扫描。请打开范围更小的文件夹，例如其中的子文件夹。`,
          ),
          'error',
        );
      }
    });
    await listen<{ changes: Change[] }>('fs:change', (e) => this.onFsChange(e.payload.changes));
    await listen('fs:rescan', () => void this.rescan());
    await listen<{ id: number; hits: SearchHit[] }>('search:batch', (e) => {
      if (e.payload.id === this.searchId) this.searchHits.push(...e.payload.hits);
    });
    await listen<{ id: number; error: string | null; truncated: boolean }>('search:done', (e) => {
      if (e.payload.id !== this.searchId) return;
      this.searchRunning = false;
      this.searchError = e.payload.error;
      this.searchTruncated = e.payload.truncated;
    });

    // ファイルツリーの上に落としたらそのフォルダにコピー、それ以外は onDrop（画像のリンク・ファイルやフォルダを開く）
    await getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === 'leave') {
        this.fileDropDir = null;
      } else if (p.type === 'enter' || p.type === 'over') {
        this.fileDropDir = this.treeDropDir(p.position);
      } else if (p.type === 'drop') {
        const dir = this.treeDropDir(p.position);
        this.fileDropDir = null;
        if (dir !== null) void this.importFiles(p.paths, dir);
        else void this.onDrop(p.paths);
      }
    });
    await getCurrentWindow().onCloseRequested(async () => {
      await this.saveAll();
      this.saveWorkspaceStateNow();
    });

    void api.listShells().then((s) => (this.shells = s));
    void this.refreshAiTools();

    // 起動時に指定がなければ、前回開いていたフォルダを開く（なくなっていればようこそ画面）。
    // エクスプローラーの右クリックからファイルを指定されたら、そのフォルダを開いてファイルを表示する
    const launch = await api.getLaunchTarget();
    this.recent = await api.getRecent();
    const target = launch?.root ?? this.recent[0];
    if (target) await this.openRoot(target);
    if (launch?.file && this.root) await this.openFile(launch.file, { pin: true });

    // エクスプローラーの右クリックメニューを登録し直す（実行ファイルの場所と言語を最新にする）。開発中は登録しない
    if (this.config.explorerMenu && !import.meta.env.DEV) void api.registerExplorerMenu(true).catch(() => {});

    // 起動の邪魔をしないよう少し待ってから確認する。開発中は確認しない
    if (this.config.checkUpdates && !import.meta.env.DEV) setTimeout(() => void this.checkForUpdate(false), 5000);
  }

  async refreshAiTools() {
    this.aiTools = await api.listAiTools();
  }

  // ================= アップデート =================

  private updateDeclined = false;

  /** GitHub のリリースに新しいバージョンがあれば、ダウンロードして入れ替え、再起動する。manual は設定画面などから呼んだとき */
  async checkForUpdate(manual: boolean) {
    if (!manual && this.updateDeclined) return;
    let update: Update | null;
    try {
      update = await checkUpdate();
    } catch (e) {
      if (manual) {
        const msg = errorMessage(e);
        this.toast(tr(`アップデートを確認できません: ${msg}`, `Couldn't check for updates: ${msg}`, `无法检查更新：${msg}`), 'error');
      }
      return;
    }
    if (!update) {
      if (manual) {
        const v = await getVersion();
        this.toast(tr(`最新のバージョン（${v}）です`, `You're on the latest version (${v})`, `已是最新版本（${v}）`));
      }
      return;
    }
    if (!(await api.isInstalled())) {
      // 単体実行版の exe は自分を置き換えられないので、ダウンロードしてもらう
      const ok = await this.confirm(
        tr('アップデート', 'Update', '更新'),
        tr(
          `新しいバージョン ${update.version} があります（いまは ${update.currentVersion}）。インストールせずに使う版なので、自動では更新できません。リリースのページを開きますか？`,
          `Version ${update.version} is available (current: ${update.currentVersion}). This portable version can't update itself. Open the releases page?`,
          `有新版本 ${update.version}（当前为 ${update.currentVersion}）。此版本为免安装版，无法自动更新。要打开发布页面吗？`,
        ),
        tr('ページを開く', 'Open page', '打开页面'),
        tr('あとで', 'Later', '稍后'),
      );
      if (ok) await openUrl(RELEASES_URL);
      else this.updateDeclined = true;
      return;
    }
    const running = this.terminals.some((t) => !t.exited);
    const ok = await this.confirm(
      tr('アップデート', 'Update', '更新'),
      tr(
        `新しいバージョン ${update.version} があります（いまは ${update.currentVersion}）。更新して再起動しますか？編集中のファイルは保存します。`,
        `Version ${update.version} is available (current: ${update.currentVersion}). Update and restart? Open files will be saved.`,
        `有新版本 ${update.version}（当前为 ${update.currentVersion}）。要更新并重新启动吗？正在编辑的文件会被保存。`,
      ) +
        (running
          ? tr(
              'ターミナル（AI ツールを含む）は終了します。',
              ' Terminals (including AI tools) will be closed.',
              '终端（包括 AI 工具）将被关闭。',
            )
          : ''),
      tr('更新して再起動', 'Update and restart', '更新并重新启动'),
      tr('あとで', 'Later', '稍后'),
    );
    if (!ok) {
      this.updateDeclined = true;
      return;
    }
    await this.saveAll();
    this.saveWorkspaceStateNow();
    this.toast(tr(`バージョン ${update.version} をダウンロードしています…`, `Downloading version ${update.version}…`, `正在下载版本 ${update.version}…`));
    try {
      // Windows ではインストーラーが動くとアプリは終了し、入れ替え後に起動し直す
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      const msg = errorMessage(e);
      this.toast(tr(`アップデートできませんでした: ${msg}`, `Couldn't update: ${msg}`, `更新失败：${msg}`), 'error');
    }
  }

  // ================= ルートディレクトリ =================

  async chooseRoot() {
    const dir = await openDialog({ directory: true, multiple: false, title: tr('フォルダを開く', 'Open folder', '打开文件夹') });
    if (typeof dir === 'string') await this.openRoot(dir);
  }

  /** 最近開いたフォルダを名前で絞り込んで選ぶ */
  async chooseRecentRoot() {
    this.recent = await api.getRecent();
    this.palette = 'folders';
  }

  async openRoot(path: string) {
    await this.saveAll();
    if (this.root) this.saveWorkspaceStateNow();
    let res;
    // WSL のフォルダは、初回に WSL へエージェントを置いて起動するので数秒かかる
    if (/^\\\\(\?\\UNC\\)?wsl(\.localhost|\$)\\/i.test(path)) this.toast(tr('WSL に接続しています…', 'Connecting to WSL…', '正在连接 WSL…'));
    try {
      res = await api.openRoot(path);
    } catch (e) {
      this.toast(errorMessage(e), 'error');
      await api.removeRecent(path).catch(() => {});
      this.recent = await api.getRecent();
      return;
    }
    this.adoptScanGen(res.gen);
    this.root = res.root;
    this.rootName = res.name;
    this.wsl = res.wsl;
    if (res.wslError) {
      const err = res.wslError;
      this.toast(
        tr(
          `WSL のエージェントを使えないため、変更の自動反映などが動きません: ${err}`,
          `The WSL agent isn't available, so features like live reload of changes won't work: ${err}`,
          `无法使用 WSL 代理，自动同步更改等功能将无法工作：${err}`,
        ),
        'error',
      );
    }
    // WSL のフォルダでは AI ツールを WSL の中で探し直す
    void this.refreshAiTools();
    this.expanded.clear();
    this.keepDirs.clear();
    this.aiChanged.clear();
    this.aiBefore.clear();
    this.docs = [];
    this.activePath = null;
    this.searchHits = [];
    this.treeVersion++;

    const ws = (await api.loadWorkspaceState(res.root)) as WorkspaceState | null;
    if (ws) {
      ws.expanded?.forEach((p) => this.expanded.add(p));
      this.leftWidth = ws.leftWidth ?? this.leftWidth;
      this.leftVisible = ws.leftVisible ?? true;
      this.leftView = ws.leftView === 'search' || ws.leftView === 'outline' ? ws.leftView : 'files';
      this.terminalHeight = ws.terminalHeight ?? this.terminalHeight;
      this.terminalVisible = ws.terminalVisible ?? false;
      for (const t of ws.tabs ?? []) {
        await this.openFile(t.path, { pin: true, mode: t.mode, quiet: true });
      }
      if (ws.active && this.docs.some((d) => d.path === ws.active)) this.activePath = ws.active;
    }
    await this.restoreRecovery();
  }

  async rescan() {
    if (!this.root) return;
    this.adoptScanGen(await api.rescan());
  }

  /** gen が今の世代以上なら true。新しい世代なら一覧を空にして採用する */
  private adoptScanGen(gen: number): boolean {
    if (gen < this.scanGen) return false;
    if (gen > this.scanGen) {
      this.scanGen = gen;
      this.entries.clear();
      this.treeVersion++;
    }
    return true;
  }

  private async restoreRecovery() {
    const list = await api.listRecovery().catch(() => []);
    const pending = [];
    for (const r of list) {
      try {
        const disk = await api.readFile(r.path);
        if (disk.content !== r.content) pending.push(r);
        else await api.deleteRecovery(r.path);
      } catch {
        pending.push(r);
      }
    }
    if (pending.length === 0) return;
    const n = pending.length;
    const bullet = tr('・', '• ', '• ');
    const names = pending.map((p) => bullet + p.path).join('\n');
    const ok = await this.confirm(
      tr('編集内容の復元', 'Restore unsaved changes', '恢复编辑内容'),
      tr(
        `前回、保存されないまま終了したファイルが ${n} 件あります。\n${names}\n\n編集内容を復元しますか？（復元後に保存すると反映されます）`,
        `${n} file(s) had unsaved changes when the app last closed.\n${names}\n\nRestore the changes? (They're written to disk when you save.)`,
        `上次有 ${n} 个文件未保存就退出了。\n${names}\n\n要恢复编辑内容吗？（恢复后保存即可生效）`,
      ),
      tr('復元する', 'Restore', '恢复'),
      tr('破棄する', 'Discard', '丢弃'),
    );
    for (const r of pending) {
      if (ok) {
        await this.openFile(r.path, { pin: true, quiet: true, missingOk: true });
        const doc = this.docs.find((d) => d.path === r.path);
        if (doc) this.setDocText(doc, r.content);
      } else {
        await api.deleteRecovery(r.path);
      }
    }
  }

  workspaceState(): WorkspaceState {
    return {
      expanded: [...this.expanded],
      tabs: this.docs.filter((d) => d.pinned).map((d) => ({ path: d.path, mode: d.mode })),
      active: this.activePath,
      leftWidth: this.leftWidth,
      leftVisible: this.leftVisible,
      leftView: this.leftView,
      terminalHeight: this.terminalHeight,
      terminalVisible: this.terminalVisible,
    };
  }

  saveWorkspaceState() {
    if (!this.root) return;
    if (this.wsSaveTimer) clearTimeout(this.wsSaveTimer);
    this.wsSaveTimer = setTimeout(() => this.saveWorkspaceStateNow(), 800);
  }

  saveWorkspaceStateNow() {
    if (!this.root) return;
    if (this.wsSaveTimer) clearTimeout(this.wsSaveTimer);
    void api.saveWorkspaceState(this.root, this.workspaceState()).catch(() => {});
  }

  // ================= ファイル監視 =================

  private onFsChange(changes: Change[]) {
    const touched = new Set<string>();
    for (const c of changes) {
      if (c.kind === 'remove') {
        const lower = c.path.toLowerCase();
        for (const key of [...this.entries.keys()]) {
          const k = key.toLowerCase();
          if (k === lower || k.startsWith(lower + '/')) this.entries.delete(key);
        }
        touched.add(c.path);
      } else {
        this.entries.set(c.path, { path: c.path, dir: c.dir, mtime: c.mtime, size: c.size });
        touched.add(c.path);
      }
    }
    this.treeVersion++;
    for (const p of touched) {
      const lower = p.toLowerCase();
      for (const doc of this.docs) {
        if (doc.path.toLowerCase() === lower || doc.path.toLowerCase().startsWith(lower + '/')) {
          void this.checkExternal(doc);
        }
      }
      if (this.aiRunning && !this.entries.get(p)?.dir) this.aiChanged.add(p);
    }
  }

  /** 開いているファイルが外で変わったかを確かめて取り込む */
  private async checkExternal(doc: Doc) {
    if (doc.saving) await doc.saving;
    let disk: ReadResult;
    try {
      disk = await api.readFile(doc.path);
    } catch {
      const exists = await api.pathExists(doc.path).catch(() => false);
      if (!exists && !doc.deleted) {
        doc.deleted = true;
        this.toast(
          tr(
            `${doc.path} は削除されました。保存すると作り直します。`,
            `${doc.path} was deleted. Save to recreate it.`,
            `${doc.path} 已被删除。保存即可重新创建。`,
          ),
        );
      }
      return;
    }
    doc.deleted = false;
    doc.readonly = disk.readonly;
    if (disk.content === doc.saved) return;
    if (this.aiRunning && !this.aiBefore.has(doc.path)) this.aiBefore.set(doc.path, doc.saved);
    if (!doc.dirty || doc.text === disk.content) {
      doc.encoding = disk.encoding;
      doc.eol = disk.eol;
      this.setDocText(doc, disk.content);
      doc.saved = disk.content;
    } else if (!this.conflict) {
      // 解決するまで自動保存しない（外部の変更を上書きしてしまうため）
      if (doc.autosaveTimer) clearTimeout(doc.autosaveTimer);
      this.conflict = { doc, disk };
    }
  }

  resolveConflict(choice: 'mine' | 'theirs' | 'both') {
    const c = this.conflict;
    if (!c) return;
    this.conflict = null;
    const { doc, disk } = c;
    if (choice !== 'mine') {
      // エディタの内容をファイルに合わせるので、退避していた編集内容は要らない
      if (doc.recoveryTimer) clearTimeout(doc.recoveryTimer);
      void api.deleteRecovery(doc.path).catch(() => {});
    }
    if (choice === 'mine') {
      doc.saved = disk.content;
      void this.save(doc);
    } else if (choice === 'theirs') {
      this.setDocText(doc, disk.content);
      doc.saved = disk.content;
    } else {
      const mine = doc.text;
      const d = new Date();
      const p2 = (n: number) => String(n).padStart(2, '0');
      const stamp = `${d.getFullYear()}${p2(d.getMonth() + 1)}${p2(d.getDate())}-${p2(d.getHours())}${p2(d.getMinutes())}`;
      const suffix = tr(` (競合 ${stamp})`, ` (conflict ${stamp})`, ` (冲突 ${stamp})`);
      const copy = join(dirname(doc.path), `${basename(doc.path).replace(/(\.[^.]+)?$/, `${suffix}$1`)}`);
      void api
        .createFile(copy, mine)
        .then((p) => this.toast(tr(`自分の変更を ${p} に保存しました`, `Saved your changes to ${p}`, `已将你的更改保存到 ${p}`)))
        .catch((e) => this.toast(errorMessage(e), 'error'));
      this.setDocText(doc, disk.content);
      doc.saved = disk.content;
    }
  }

  /** 本文を差し替える。変わった部分だけを置き換えるのでカーソルと元に戻す履歴が残る */
  setDocText(doc: Doc, text: string) {
    // 表示中のエディタがあればその内容が最新。doc.cm はタブを離れたときにしか更新されない
    const live = this.view && doc.path === this.activePath ? this.view : null;
    const old = live ? live.state.doc.toString() : doc.cm ? doc.cm.doc.toString() : doc.text;
    if (old !== text) {
      let start = 0;
      while (start < old.length && start < text.length && old[start] === text[start]) start++;
      let endOld = old.length;
      let endNew = text.length;
      while (endOld > start && endNew > start && old[endOld - 1] === text[endNew - 1]) {
        endOld--;
        endNew--;
      }
      const change = { from: start, to: endOld, insert: text.slice(start, endNew) };
      if (live) {
        live.dispatch({ changes: change, userEvent: 'external' });
      } else if (doc.cm) {
        doc.cm = doc.cm.update({ changes: change }).state;
      }
    }
    doc.text = text;
  }

  // ================= タブ =================

  async openFile(path: string, opts: { pin?: boolean; line?: number; mode?: Mode; quiet?: boolean; missingOk?: boolean } = {}) {
    const existing = this.docs.find((d) => d.path.toLowerCase() === path.toLowerCase());
    if (existing) {
      if (opts.pin) existing.pinned = true;
      this.activate(existing.path, opts.line);
      return;
    }
    let r: ReadResult;
    try {
      r = await api.readFile(path);
    } catch (e) {
      const msg = errorMessage(e);
      if (opts.missingOk) {
        r = { content: '', encoding: 'utf-8', eol: 'lf', readonly: false, mtime: 0 };
      } else if (msg.startsWith(NOT_TEXT_PREFIX)) {
        // 画像や Office のファイルなどは、Windows の既定のアプリで開けるようにする
        if (opts.quiet) return;
        const reason = msg.slice(NOT_TEXT_PREFIX.length);
        const ok = await this.confirm(
          tr('ファイルを開く', 'Open file', '打开文件'),
          tr(`${reason}。既定のアプリで開きますか？`, `${reason}. Open it in the default app?`, `${reason}。要用默认应用打开吗？`),
          tr('既定のアプリで開く', 'Open in default app', '用默认应用打开'),
        );
        if (ok) await api.openWithDefaultApp(path).catch((err) => this.toast(errorMessage(err), 'error'));
        return;
      } else {
        if (!opts.quiet) this.toast(msg, 'error');
        return;
      }
    }
    const doc = new Doc(path, r);
    doc.pinned = !!opts.pin;
    doc.mode = opts.mode ?? 'edit';
    // 仮タブ（固定していないタブ）は次に開くファイルで置き換える
    const preview = this.docs.findIndex((d) => !d.pinned && !d.dirty);
    if (preview >= 0 && !opts.pin) this.docs.splice(preview, 1, doc);
    else this.docs.push(doc);
    this.activate(path, opts.line);
  }

  activate(path: string, line?: number) {
    this.activePath = path;
    this.selected = path;
    this.revealInTree(path);
    if (line) {
      this.pendingReveal = { path, line };
      this.revealSeq++;
    }
    this.saveWorkspaceState();
  }

  revealInTree(path: string) {
    let dir = dirname(path);
    while (dir) {
      this.expanded.add(dir);
      dir = dirname(dir);
    }
  }

  async closeTab(path: string) {
    const doc = this.docs.find((d) => d.path === path);
    if (!doc) return;
    if (doc.dirty) {
      const ok = await this.save(doc);
      if (!ok) {
        const discard = await this.confirm(
          tr('保存できません', "Couldn't save", '无法保存'),
          tr(
            `${doc.path} を保存できませんでした。変更を破棄して閉じますか？`,
            `Couldn't save ${doc.path}. Close and discard changes?`,
            `无法保存 ${doc.path}。要放弃更改并关闭吗？`,
          ),
          tr('破棄して閉じる', 'Discard and close', '放弃并关闭'),
          tr('キャンセル', 'Cancel', '取消'),
          true,
        );
        if (!discard) return;
        void api.deleteRecovery(doc.path).catch(() => {});
      }
    }
    const i = this.docs.indexOf(doc);
    this.docs.splice(i, 1);
    if (this.activePath === path) {
      const next = this.docs[i] ?? this.docs[i - 1];
      this.activePath = next?.path ?? null;
    }
    this.saveWorkspaceState();
  }

  cycleTab(dir: 1 | -1) {
    if (this.docs.length === 0) return;
    const i = this.docs.findIndex((d) => d.path === this.activePath);
    const next = this.docs[(i + dir + this.docs.length) % this.docs.length];
    this.activate(next.path);
  }

  cycleMode() {
    const doc = this.activeDoc;
    if (!doc || !this.isMarkdown(doc.path)) return;
    doc.mode = doc.mode === 'edit' ? 'split' : doc.mode === 'split' ? 'preview' : 'edit';
    this.saveWorkspaceState();
  }

  // ================= 保存 =================

  onDocChanged(doc: Doc, text: string) {
    doc.text = text;
    if (!doc.pinned) doc.pinned = true;
    if (doc.autosaveTimer) clearTimeout(doc.autosaveTimer);
    if (this.config.autosaveDelay > 0) {
      doc.autosaveTimer = setTimeout(() => void this.save(doc), this.config.autosaveDelay);
    }
    if (doc.recoveryTimer) clearTimeout(doc.recoveryTimer);
    doc.recoveryTimer = setTimeout(() => {
      if (doc.dirty) void api.writeRecovery(doc.path, doc.text).catch(() => {});
    }, 1000);
  }

  /** 保存する。成功すれば true */
  async save(doc: Doc, force = false): Promise<boolean> {
    if (doc.autosaveTimer) clearTimeout(doc.autosaveTimer);
    if (doc.saving) await doc.saving;
    if (!doc.dirty && !force) return true;
    if (this.conflict?.doc === doc) return false;
    if (doc.readonly) {
      this.toast(tr(`${doc.path} は読み取り専用です`, `${doc.path} is read-only`, `${doc.path} 是只读文件`), 'error');
      return false;
    }
    const text = doc.text;
    let ok = false;
    doc.busy = true;
    doc.saving = (async () => {
      try {
        await api.writeFile(doc.path, text, doc.encoding, doc.eol);
        doc.saved = text;
        doc.deleted = false;
        ok = true;
      } catch (e) {
        const msg = errorMessage(e);
        if (msg === 'unmappable') {
          const conv = await this.confirm(
            tr('文字コード', 'Encoding', '编码'),
            tr(
              `${doc.path} は ${doc.encoding} で保存されていますが、この文字コードでは表せない文字が含まれています。UTF-8 に変えて保存しますか？`,
              `${doc.path} is saved in ${doc.encoding}, but it contains characters that this encoding can't represent. Save as UTF-8 instead?`,
              `${doc.path} 以 ${doc.encoding} 编码保存，但包含该编码无法表示的字符。要改为 UTF-8 保存吗？`,
            ),
            tr('UTF-8 で保存', 'Save as UTF-8', '以 UTF-8 保存'),
          );
          if (conv) {
            doc.encoding = 'utf-8';
            try {
              await api.writeFile(doc.path, text, 'utf-8', doc.eol);
              doc.saved = text;
              ok = true;
            } catch (e2) {
              this.toast(errorMessage(e2), 'error');
            }
          }
        } else {
          this.toast(msg, 'error');
        }
      }
    })();
    await doc.saving;
    doc.saving = null;
    doc.busy = false;
    if (ok) {
      if (doc.recoveryTimer) clearTimeout(doc.recoveryTimer);
      void api.deleteRecovery(doc.path).catch(() => {});
      // 保存中にさらに編集されていたら続けて保存する
      if (doc.text !== doc.saved && this.config.autosaveDelay > 0) {
        doc.autosaveTimer = setTimeout(() => void this.save(doc), this.config.autosaveDelay);
      }
    }
    return ok;
  }

  async saveAll() {
    await Promise.all(this.docs.filter((d) => d.dirty).map((d) => this.save(d)));
  }

  // ================= ファイル操作 =================

  targetDir(): string {
    const sel = this.selected;
    if (sel === null) return '';
    const e = this.entries.get(sel);
    return e?.dir ? sel : dirname(sel);
  }

  async newFile(dir = this.targetDir()) {
    if (!this.root) return;
    const where = dir || this.rootName;
    const name = await this.prompt(
      tr('新しいファイル', 'New file', '新建文件'),
      tr(`${where} に作るファイルの名前`, `Name of the new file in ${where}`, `要在 ${where} 中创建的文件名`),
      tr('無題.md', 'Untitled.md', '无标题.md'),
      true,
    );
    if (!name) return;
    const file = /\.[^./]+$/.test(name) ? name : `${name}.md`;
    try {
      const path = await api.createFile(join(dir, file));
      this.entries.set(path, { path, dir: false, mtime: Date.now(), size: 0 });
      this.treeVersion++;
      await this.openFile(path, { pin: true });
      setTimeout(() => this.view?.focus(), 50);
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async newFolder(dir = this.targetDir()) {
    if (!this.root) return;
    const where = dir || this.rootName;
    const name = await this.prompt(
      tr('新しいフォルダ', 'New folder', '新建文件夹'),
      tr(`${where} に作るフォルダの名前`, `Name of the new folder in ${where}`, `要在 ${where} 中创建的文件夹名`),
      '',
    );
    if (!name) return;
    try {
      const path = await api.createDir(join(dir, name));
      this.entries.set(path, { path, dir: true, mtime: Date.now(), size: 0 });
      this.keepDirs.add(path);
      if (dir) this.expanded.add(dir);
      this.selected = path;
      this.treeVersion++;
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async rename(path: string) {
    const name = await this.prompt(tr('名前の変更', 'Rename', '重命名'), path, basename(path), true);
    if (!name || name === basename(path)) return;
    const entry = this.entries.get(path);
    let newName = name;
    if (!entry?.dir && extname(path) && !/\.[^./]+$/.test(name)) newName = `${name}.${extname(path)}`;
    await this.move(path, join(dirname(path), newName));
  }

  /** 名前変更・移動。リンクの書き換えも確認する */
  async move(from: string, to: string) {
    if (from === to) return;
    if (isInside(to, from)) {
      this.toast(tr('フォルダをその中に移動することはできません', "Can't move a folder into itself", '无法将文件夹移动到其自身内部'), 'error');
      return;
    }
    await this.saveAll();
    let rewrite = false;
    try {
      const count = await api.countLinkUpdates(from, to);
      if (count > 0) {
        const r = await this.ask({
          title: tr('リンクの書き換え', 'Update links', '更新链接'),
          message: tr(
            `${from} へのリンクなど、${count} 件のリンクが影響を受けます。新しい場所に合わせて書き換えますか？`,
            `${count} link(s), including links to ${from}, are affected. Update them to point to the new location?`,
            `包括指向 ${from} 的链接在内，共有 ${count} 个链接受到影响。要按新位置更新这些链接吗？`,
          ),
          buttons: [
            { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
            { label: tr('書き換えない', "Don't update", '不更新'), value: 'no' },
            { label: tr('書き換える', 'Update', '更新'), value: 'yes', primary: true },
          ],
        });
        if (!r || r.button === 'cancel') return;
        rewrite = r.button === 'yes';
      }
      const res = await api.renamePath(from, to, rewrite);
      // 一覧・タブ・展開状態を新しい場所に付け替える
      for (const [key, e] of [...this.entries]) {
        if (isInside(key, from)) {
          this.entries.delete(key);
          const p = rebase(key, from, res.path);
          this.entries.set(p, { ...e, path: p });
        }
      }
      for (const doc of this.docs) if (isInside(doc.path, from)) doc.path = rebase(doc.path, from, res.path);
      if (this.activePath && isInside(this.activePath, from)) this.activePath = rebase(this.activePath, from, res.path);
      for (const d of [...this.expanded]) {
        if (isInside(d, from)) {
          this.expanded.delete(d);
          this.expanded.add(rebase(d, from, res.path));
        }
      }
      this.selected = res.path;
      this.treeVersion++;
      if (res.links > 0) {
        const nf = res.files.length;
        const nl = res.links;
        this.toast(tr(`${nf} 個のファイルで ${nl} 件のリンクを書き換えました`, `Updated ${nl} link(s) in ${nf} file(s)`, `已在 ${nf} 个文件中更新 ${nl} 个链接`));
      }
      this.saveWorkspaceState();
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async remove(path: string) {
    const entry = this.entries.get(path);
    const isDir = !!entry?.dir;
    const ok = await this.confirm(
      tr('削除', 'Delete', '删除'),
      tr(
        `${path}${isDir ? ' フォルダとその中身' : ''} をごみ箱に移動しますか？`,
        `Move ${isDir ? 'the folder ' : ''}${path}${isDir ? ' and its contents' : ''} to the Recycle Bin?`,
        `要将 ${path}${isDir ? ' 文件夹及其内容' : ''}移到回收站吗？`,
      ),
      tr('ごみ箱に移動', 'Move to Recycle Bin', '移到回收站'),
      tr('キャンセル', 'Cancel', '取消'),
      true,
    );
    if (!ok) return;
    try {
      await api.deletePath(path);
      for (const doc of [...this.docs]) {
        if (isInside(doc.path, path)) {
          if (doc.autosaveTimer) clearTimeout(doc.autosaveTimer);
          this.docs.splice(this.docs.indexOf(doc), 1);
          void api.deleteRecovery(doc.path).catch(() => {});
        }
      }
      if (this.activePath && !this.docs.some((d) => d.path === this.activePath)) this.activePath = this.docs[0]?.path ?? null;
      for (const key of [...this.entries.keys()]) if (isInside(key, path)) this.entries.delete(key);
      this.treeVersion++;
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async duplicate(path: string) {
    try {
      const p = await api.duplicatePath(path);
      this.selected = p;
      if (!this.entries.get(path)?.dir) {
        this.entries.set(p, { path: p, dir: false, mtime: Date.now(), size: 0 });
        this.treeVersion++;
      }
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async copyPath(path: string, rel: boolean) {
    const text = rel ? path : await api.absolutePath(path);
    await navigator.clipboard.writeText(text);
    this.toast(tr(`コピーしました: ${text}`, `Copied: ${text}`, `已复制：${text}`));
  }

  // ================= 貼り付け・ドロップ =================

  /** 画像を保存し、エディタにリンクを入れる */
  insertImageLink(rel: string) {
    const doc = this.activeDoc;
    if (!doc || !this.view) return;
    const link = `![](${encodeLinkTarget(relative(dirname(doc.path), rel))})`;
    this.view.dispatch(this.view.state.replaceSelection(link), { userEvent: 'input', scrollIntoView: true });
    this.view.focus();
  }

  async pasteImage(file: File) {
    const doc = this.activeDoc;
    if (!doc) return;
    const buf = new Uint8Array(await file.arrayBuffer());
    let bin = '';
    for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
    const ext = file.type.split('/')[1]?.replace('jpeg', 'jpg').replace('svg+xml', 'svg') || 'png';
    try {
      const rel = await api.saveAsset(doc.path, btoa(bin), ext, this.config.imageFolder);
      this.insertImageLink(rel);
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async plainPaste() {
    const text = await navigator.clipboard.readText();
    this.view?.dispatch(this.view.state.replaceSelection(text), { userEvent: 'input.paste', scrollIntoView: true });
  }

  /** 外からのドロップの位置（物理ピクセル）がファイルツリーの上なら、コピー先のフォルダ（ルートは ''）。外なら null */
  private treeDropDir(pos: { x: number; y: number }): string | null {
    if (!this.root) return null;
    const scale = window.devicePixelRatio || 1;
    const el = document.elementFromPoint(pos.x / scale, pos.y / scale);
    if (!el?.closest('.tree')) return null;
    const path = el.closest<HTMLElement>('[data-path]')?.dataset.path;
    if (path === undefined) return '';
    return this.entries.get(path)?.dir ? path : dirname(path);
  }

  /** ファイルツリーに落としたファイル・フォルダを dir にコピーする */
  async importFiles(paths: string[], dir: string) {
    try {
      const copied = await api.importPaths(paths, dir);
      if (copied.length === 0) return;
      if (dir) this.expanded.add(dir);
      this.revealInTree(copied[0]);
      this.selected = copied[0];
      const where = dir || this.rootName;
      this.toast(
        copied.length === 1
          ? tr(`${basename(copied[0])} を ${where} にコピーしました`, `Copied ${basename(copied[0])} to ${where}`, `已将 ${basename(copied[0])} 复制到 ${where}`)
          : tr(`${copied.length} 個を ${where} にコピーしました`, `Copied ${copied.length} items to ${where}`, `已将 ${copied.length} 项复制到 ${where}`),
      );
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  private async onDrop(paths: string[]) {
    for (const p of paths) {
      const rel = this.root ? absToRel(this.root, p) : null;
      // 画像のリンクを入れるのは Markdown を開いているときだけ
      if (isImage(p) && this.activeDoc && this.isMarkdown(this.activeDoc.path)) {
        try {
          const r = await api.copyAsset(this.activeDoc.path, p, this.config.imageFolder);
          this.insertImageLink(r);
        } catch (e) {
          this.toast(errorMessage(e), 'error');
        }
      } else if (rel !== null && rel !== '' && this.entries.get(rel)?.dir === false) {
        await this.openFile(rel, { pin: true });
      } else if (!/\.[^\\/.]+$/.test(p) || rel === '') {
        // フォルダ（拡張子なし）とみなして開く
        if (!this.root) await this.openRoot(p);
        else {
          const r = await this.ask({
            title: tr('フォルダを開く', 'Open folder', '打开文件夹'),
            message: p,
            buttons: [
              { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
              { label: tr('新しいウィンドウで開く', 'Open in new window', '在新窗口中打开'), value: 'new' },
              { label: tr('このウィンドウで開く', 'Open in this window', '在此窗口中打开'), value: 'here', primary: true },
            ],
          });
          if (r?.button === 'here') await this.openRoot(p);
          else if (r?.button === 'new') await api.openNewWindow(p);
        }
      } else {
        this.toast(
          tr(
            '開いているフォルダの外のファイルは開けません。フォルダごと開いてください。',
            "Can't open files outside the open folder. Open their folder instead.",
            '无法打开当前文件夹以外的文件。请打开其所在的文件夹。',
          ),
          'error',
        );
      }
    }
  }

  // ================= 検索 =================

  async runSearch() {
    this.searchHits = [];
    this.searchError = null;
    this.searchTruncated = false;
    if (!this.searchOptions.query) {
      this.searchRunning = false;
      await api.searchCancel();
      return;
    }
    this.searchRunning = true;
    this.searchId = await api.searchStart($state.snapshot(this.searchOptions));
  }

  async replaceAll(replacement: string) {
    const files = [...new Set(this.searchHits.map((h) => h.path))];
    if (files.length === 0) return;
    const count = this.searchHits.reduce((n, h) => n + h.ranges.length, 0);
    const nf = files.length;
    const ok = await this.confirm(
      tr('すべて置換', 'Replace all', '全部替换'),
      tr(
        `${nf} 個のファイルで約 ${count} 件を「${replacement}」に置き換えます。よろしいですか？`,
        `Replace about ${count} match(es) in ${nf} file(s) with "${replacement}"?`,
        `将在 ${nf} 个文件中把约 ${count} 处替换为“${replacement}”。确定吗？`,
      ),
      tr('置換する', 'Replace', '替换'),
      tr('キャンセル', 'Cancel', '取消'),
      true,
    );
    if (!ok) return;
    await this.saveAll();
    try {
      const r = await api.replaceInFiles($state.snapshot(this.searchOptions), replacement, files);
      const rf = r.files.length;
      const rn = r.replacements;
      this.toast(tr(`${rf} 個のファイルで ${rn} 件を置き換えました`, `Replaced ${rn} match(es) in ${rf} file(s)`, `已在 ${rf} 个文件中替换 ${rn} 处`));
      await this.runSearch();
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  // ================= ターミナル・AI =================

  /** 開いているファイルと選択範囲を、Claude Code のフックが読む文脈ファイルに書き出す（変わるたびに呼ぶ） */
  scheduleAiContext() {
    if (this.aiContextTimer) clearTimeout(this.aiContextTimer);
    this.aiContextTimer = setTimeout(() => void this.syncAiContext(), 200);
  }

  async syncAiContext() {
    if (this.aiContextTimer) clearTimeout(this.aiContextTimer);
    this.aiContextTimer = null;
    if (!this.root) return;
    const doc = this.activeDoc;
    let selection: AiSelection | null = null;
    if (doc && this.view) {
      const { from, to } = this.view.state.selection.main;
      if (from !== to) {
        const lines = this.view.state.doc;
        selection = { fromLine: lines.lineAt(from).number, toLine: lines.lineAt(to).number, text: this.view.state.sliceDoc(from, to) };
      }
    }
    try {
      await api.setAiContext(doc?.path ?? null, selection);
    } catch {
      // 書けなくても AI ツールが文脈なしで動くだけなので知らせない
    }
  }

  terminalCwd(): string | null {
    const doc = this.activeDoc;
    if (this.config.terminal.cwd === 'noteDir' && doc) return dirname(doc.path);
    return null;
  }

  newTerminal(opts: { shellId?: string | null; toolId?: string; actionId?: string; cwd?: string | null; title?: string } = {}) {
    if (!this.root) {
      this.toast(tr('先にフォルダを開いてください', 'Open a folder first', '请先打开文件夹'), 'error');
      return;
    }
    void this.syncAiContext();
    const doc = this.activeDoc;
    const selection = this.view && doc ? this.view.state.sliceDoc(this.view.state.selection.main.from, this.view.state.selection.main.to) : '';
    // WSL のフォルダでは、シェルを指定しなければそのディストリビューションのシェルで開く
    const wslShell = this.wsl ? this.shells.find((s) => s.id === `wsl:${this.wsl!.distro}`)?.id : undefined;
    const shellId = opts.shellId ?? wslShell ?? (this.config.terminal.defaultShell || null);
    // シェル未指定なら Rust 側と同じく PowerShell 7 → Windows PowerShell の順で使われる
    const shell =
      this.shells.find((s) => s.id === shellId) ?? this.shells.find((s) => s.id === 'pwsh') ?? this.shells.find((s) => s.id === 'powershell');
    const title = opts.title ?? (opts.toolId ? this.aiTools.find((t) => t.id === opts.toolId)?.name : shell?.name) ?? tr('ターミナル', 'Terminal', '终端');
    const s = new TermSession(
      ++this.termSeq,
      title,
      {
        shellId,
        toolId: opts.toolId ?? null,
        actionId: opts.actionId ?? null,
        cwd: opts.cwd !== undefined ? opts.cwd : this.terminalCwd(),
        note: doc?.path ?? null,
        selection: selection || null,
      },
      !!opts.toolId,
    );
    this.terminals.push(s);
    this.activeTerminal = s.key;
    this.terminalVisible = true;
  }

  closeTerminal(key: number) {
    const i = this.terminals.findIndex((t) => t.key === key);
    if (i < 0) return;
    const t = this.terminals[i];
    if (t.ptyId !== null && !t.exited) void api.ptyKill(t.ptyId);
    this.terminals.splice(i, 1);
    if (this.activeTerminal === key) this.activeTerminal = this.terminals[i]?.key ?? this.terminals[i - 1]?.key ?? null;
    if (this.terminals.length === 0) this.terminalVisible = false;
  }

  toggleTerminal() {
    this.terminalVisible = !this.terminalVisible;
    if (this.terminalVisible && this.terminals.length === 0) this.newTerminal();
    this.saveWorkspaceState();
  }

  async launchTool(tool: ToolInfo, actionId?: string) {
    if (!tool.found) {
      if (tool.installUrl) {
        const ok = await this.confirm(
          tool.name,
          tr(
            `${tool.name} が見つかりません。インストール方法のページをブラウザで開きますか？`,
            `${tool.name} wasn't found. Open the installation page in your browser?`,
            `未找到 ${tool.name}。要在浏览器中打开安装说明页面吗？`,
          ),
          tr('開く', 'Open', '打开'),
        );
        if (ok) await openUrl(tool.installUrl);
      }
      return;
    }
    if (tool.kind === 'ide') {
      try {
        await api.launchIdeTool(tool.id, actionId ?? null, this.activeDoc?.path ?? null);
      } catch (e) {
        this.toast(errorMessage(e), 'error');
      }
      return;
    }
    const label = actionId ? tool.actions.find((a) => a.id === actionId)?.label : null;
    const doc = this.activeDoc;
    this.newTerminal({
      toolId: tool.id,
      actionId,
      cwd: null,
      title: label && doc ? `${tool.name}: ${basename(doc.path)}` : tool.name,
    });
  }

  dismissAiChange(path: string) {
    this.aiChanged.delete(path);
    this.aiBefore.delete(path);
  }

  showAiDiff(doc: Doc) {
    const before = this.aiBefore.get(doc.path);
    if (before === undefined) return;
    this.diff = {
      title: tr(`${doc.path} の変更（AI セッション中）`, `Changes to ${doc.path} (during AI session)`, `${doc.path} 的更改（AI 会话期间）`),
      before,
      after: doc.text,
    };
  }

  async revertAiChange(doc: Doc) {
    const before = this.aiBefore.get(doc.path);
    if (before === undefined) return;
    this.setDocText(doc, before);
    await this.save(doc);
    this.dismissAiChange(doc.path);
  }

  async createAiGuide() {
    const ja = `# このフォルダについて

このフォルダは Markdown のメモ置き場です。mark-shell というアプリで表示・編集しています。

## 書式の決まり

- メモは 1 ファイル 1 メモの \`.md\` ファイルです。文字コードは UTF-8 です。
- 先頭に YAML のフロントマター（\`title\`, \`tags\` など）を書くことがあります。なくても構いません。
- ほかのメモへのリンクは相対パスの \`[表示名](../folder/file.md)\` か \`[[ファイル名]]\` で書きます。
- 画像はメモと同じフォルダの \`assets/\` に置き、\`![](assets/xxx.png)\` で参照します。
- 図は \`\`\`mermaid のコードブロック、数式は \`$...$\` / \`$$...$$\` で書けます。

## お願い

- ファイルを移動・名前変更するときは、それを参照しているリンクも直してください。
- 指示されていないファイルは変更しないでください。
`;
    const en = `# About this folder

This folder holds Markdown notes. They are viewed and edited with an app called mark-shell.

## Conventions

- Each note is a single \`.md\` file. Files are encoded in UTF-8.
- A note may start with YAML front matter (\`title\`, \`tags\`, etc.). It is optional.
- Link to other notes with a relative path, \`[label](../folder/file.md)\`, or with \`[[file name]]\`.
- Put images in an \`assets/\` folder next to the note and reference them as \`![](assets/xxx.png)\`.
- Diagrams can be written in \`\`\`mermaid code blocks, and math as \`$...$\` / \`$$...$$\`.

## Requests

- When you move or rename a file, also fix the links that point to it.
- Don't change files you weren't asked to change.
`;
    const zh = `# 关于此文件夹

此文件夹用于存放 Markdown 笔记，使用名为 mark-shell 的应用查看和编辑。

## 格式约定

- 每条笔记是一个 \`.md\` 文件，字符编码为 UTF-8。
- 开头可能带有 YAML front matter（\`title\`、\`tags\` 等），也可以没有。
- 链接到其他笔记时，使用相对路径 \`[显示名](../folder/file.md)\` 或 \`[[文件名]]\`。
- 图片放在笔记所在文件夹的 \`assets/\` 中，用 \`![](assets/xxx.png)\` 引用。
- 图表可以用 \`\`\`mermaid 代码块编写，公式用 \`$...$\` / \`$$...$$\` 编写。

## 注意事项

- 移动文件或重命名时，请同时修正引用它的链接。
- 请不要修改未被要求修改的文件。
`;
    const body = tr(ja, en, zh);
    for (const name of ['CLAUDE.md', 'AGENTS.md']) {
      if (await api.pathExists(name)) {
        this.toast(tr(`${name} は既にあるので変更しませんでした`, `${name} already exists, so it was left unchanged`, `${name} 已存在，未作修改`));
        continue;
      }
      await api.createFile(name, body);
      this.toast(tr(`${name} を作りました`, `Created ${name}`, `已创建 ${name}`));
    }
  }

  // ================= プレビューのリンク =================

  /** [[名前]] を解決する。同名が複数あればリンク元に近いもの */
  resolveWiki(name: string, from: string): string | null {
    const target = name.replace(/\.md$/i, '').toLowerCase();
    const hasDir = target.includes('/');
    const candidates = this.markdownFiles().filter((f) => {
      const noExt = f.replace(/\.[^./]+$/, '').toLowerCase();
      return hasDir ? noExt === target || noExt.endsWith('/' + target) : basename(noExt) === target;
    });
    if (candidates.length === 0) return null;
    const fromDir = dirname(from).toLowerCase();
    const score = (f: string) => {
      const d = dirname(f).toLowerCase();
      if (d === fromDir) return 0;
      if (fromDir.startsWith(d + '/') || d === '') return 1 + fromDir.split('/').length - d.split('/').length;
      return 100 + relative(fromDir, d).split('/').length;
    };
    return candidates.sort((a, b) => score(a) - score(b))[0];
  }

  async followLink(rel: string) {
    const [path, hash] = rel.split('#');
    let target = normalize(path) ?? path;
    if (!this.entries.has(target) && !/\.[^./]+$/.test(target)) {
      const md = `${target}.md`;
      if (this.entries.has(md)) target = md;
    }
    const entry = this.entries.get(target);
    if (entry?.dir) {
      this.revealInTree(target);
      this.expanded.add(target);
      this.selected = target;
      return;
    }
    if (!entry && !isImage(target)) {
      const create = await this.confirm(
        tr('リンク先がありません', 'Link target not found', '链接目标不存在'),
        tr(`${target} はありません。作りますか？`, `${target} doesn't exist. Create it?`, `${target} 不存在。要创建吗？`),
        tr('作る', 'Create', '创建'),
      );
      if (!create) return;
      try {
        await api.createFile(target);
        this.entries.set(target, { path: target, dir: false, mtime: Date.now(), size: 0 });
        this.treeVersion++;
      } catch (e) {
        this.toast(errorMessage(e), 'error');
        return;
      }
    } else if (isImage(target)) {
      await api.revealInExplorer(target).catch(() => {});
      return;
    }
    await this.openFile(target, { pin: true });
    if (hash) {
      this.pendingAnchor = hash;
      this.revealSeq++;
    }
  }

  pendingAnchor: string | null = null;

  async openExternal(url: string) {
    const ok = await this.confirm(
      tr('外部リンク', 'External link', '外部链接'),
      tr(`ブラウザで開きますか？\n${url}`, `Open in your browser?\n${url}`, `要在浏览器中打开吗？\n${url}`),
      tr('開く', 'Open', '打开'),
    );
    if (ok) await openUrl(url);
  }

  // ================= 設定 =================

  /** Ctrl+ホイールで変えた文字の大きさの表示（少しして消える） */
  zoomHint = $state<string | null>(null);
  private zoomTimer: ReturnType<typeof setTimeout> | null = null;
  private zoomSaveTimer: ReturnType<typeof setTimeout> | null = null;

  /** 文字の大きさを 1px ずつ変える。保存はまとめて行う */
  adjustFontSize(target: 'editor' | 'preview' | 'terminal', delta: number) {
    const clamp = (n: number) => Math.min(40, Math.max(8, n + delta));
    let size: number;
    if (target === 'editor') {
      size = clamp(this.config.editorFontSize);
      this.config.editorFontSize = size;
    } else if (target === 'preview') {
      size = clamp(this.config.previewFontSize);
      this.config.previewFontSize = size;
    } else {
      size = clamp(this.config.terminal.fontSize);
      this.config.terminal.fontSize = size;
    }
    this.zoomHint = {
      editor: tr(`エディタの文字: ${size}px`, `Editor font: ${size}px`, `编辑器字号：${size}px`),
      preview: tr(`プレビューの文字: ${size}px`, `Preview font: ${size}px`, `预览字号：${size}px`),
      terminal: tr(`ターミナルの文字: ${size}px`, `Terminal font: ${size}px`, `终端字号：${size}px`),
    }[target];
    if (this.zoomTimer) clearTimeout(this.zoomTimer);
    this.zoomTimer = setTimeout(() => (this.zoomHint = null), 1200);
    if (this.zoomSaveTimer) clearTimeout(this.zoomSaveTimer);
    this.zoomSaveTimer = setTimeout(() => {
      void api.setConfig($state.snapshot(this.config)).catch((e) => this.toast(errorMessage(e), 'error'));
    }, 500);
  }

  /** 設定の言語（system なら PC の言語）を画面と Rust 側（エラーメッセージ・WSL のエージェント）に反映する */
  async applyLanguage() {
    i18n.locale = resolveLocale(this.config.language);
    document.documentElement.lang = i18n.locale === 'zh' ? 'zh-CN' : i18n.locale;
    await api.setLocale(i18n.locale).catch(() => {});
  }

  async updateConfig(patch: Partial<Config>) {
    const before = this.config;
    this.config = { ...this.config, ...patch, terminal: { ...this.config.terminal, ...(patch.terminal ?? {}) } };
    try {
      await api.setConfig($state.snapshot(this.config));
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
    if (before.language !== this.config.language) {
      await this.applyLanguage();
      // AI ツールのメニュー名も言語に合わせて読み直す
      void this.refreshAiTools();
    }
    const scanKeys: (keyof Config)[] = ['markdownExtensions', 'followSymlinks'];
    if (scanKeys.some((k) => JSON.stringify(before[k]) !== JSON.stringify(this.config[k])) && this.root) {
      await this.openRoot(this.root);
    }
    this.treeVersion++;
  }

  // ================= 画面の部品 =================

  toast(message: string, kind: 'info' | 'error' = 'info') {
    const id = ++this.toastSeq;
    this.toasts.push({ id, message, kind });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === 'error' ? 8000 : 4000);
  }

  ask(req: Omit<DialogRequest, 'resolve'>): Promise<{ button: string; value: string } | null> {
    return new Promise((resolve) => {
      this.dialog = { ...req, resolve };
    });
  }

  async confirm(title: string, message: string, ok = 'OK', cancel = tr('キャンセル', 'Cancel', '取消'), danger = false): Promise<boolean> {
    const r = await this.ask({
      title,
      message,
      buttons: [
        { label: cancel, value: 'cancel' },
        { label: ok, value: 'ok', primary: !danger, danger },
      ],
    });
    return r?.button === 'ok';
  }

  async prompt(title: string, message: string, value: string, selectStem = false): Promise<string | null> {
    const r = await this.ask({
      title,
      message,
      input: { value, selectStem },
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: 'OK', value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return null;
    const v = r.value.trim();
    if (!v) return null;
    if (/[\\:*?"<>|]/.test(v) || v.includes('..')) {
      this.toast(
        tr('名前に \\ : * ? " < > | や .. は使えません', 'Names can\'t contain \\ : * ? " < > | or ..', '名称中不能包含 \\ : * ? " < > | 或 ..'),
        'error',
      );
      return null;
    }
    return v;
  }

  openMenu(e: MouseEvent, items: MenuItem[]) {
    e.preventDefault();
    this.menu = { x: e.clientX, y: e.clientY, items };
  }

  async print() {
    const doc = this.activeDoc;
    if (!doc) return;
    this.printHtml = this.isMarkdown(doc.path)
      ? renderMarkdown(doc.text, { root: this.root ?? '', notePath: doc.path, externalImages: 'always' })
      : `<pre>${escapeHtml(doc.text)}</pre>`;
    // 描画を待ってから印刷する
    setTimeout(() => {
      window.print();
      this.printHtml = null;
    }, 400);
  }

  commands(): Command[] {
    const doc = this.activeDoc;
    const list: Command[] = [
      { id: 'open', label: tr('フォルダを開く', 'Open folder', '打开文件夹'), shortcut: 'Ctrl+O', run: () => void this.chooseRoot() },
      { id: 'openRecent', label: tr('最近開いたフォルダを開く', 'Open recent folder', '打开最近的文件夹'), shortcut: 'Ctrl+R', run: () => void this.chooseRecentRoot() },
      { id: 'newWindow', label: tr('新しいウィンドウ', 'New window', '新建窗口'), run: () => void api.openNewWindow() },
      { id: 'newFile', label: tr('新しいファイル', 'New file', '新建文件'), shortcut: 'Ctrl+N', run: () => void this.newFile() },
      { id: 'newFolder', label: tr('新しいフォルダ', 'New folder', '新建文件夹'), run: () => void this.newFolder() },
      { id: 'save', label: tr('保存', 'Save', '保存'), shortcut: 'Ctrl+S', run: () => doc && void this.save(doc, true) },
      { id: 'saveAll', label: tr('すべて保存', 'Save all', '全部保存'), run: () => void this.saveAll() },
      { id: 'close', label: tr('タブを閉じる', 'Close tab', '关闭标签页'), shortcut: 'Ctrl+W', run: () => doc && void this.closeTab(doc.path) },
      {
        id: 'quickOpen',
        label: tr('ファイルを開く（名前で検索）', 'Go to file (search by name)', '转到文件（按名称搜索）'),
        shortcut: 'Ctrl+P',
        run: () => (this.palette = 'files'),
      },
      { id: 'search', label: tr('全文検索', 'Search in files', '全文搜索'), shortcut: 'Ctrl+Shift+F', run: () => this.showLeft('search') },
      { id: 'files', label: tr('ファイルツリーを表示', 'Show file tree', '显示文件树'), run: () => this.showLeft('files') },
      { id: 'outline', label: tr('アウトラインを表示', 'Show outline', '显示大纲'), run: () => this.showLeft('outline') },
      { id: 'toggleLeft', label: tr('左ペインの表示切り替え', 'Toggle side bar', '切换侧边栏'), shortcut: 'Ctrl+Shift+E', run: () => this.toggleLeft() },
      {
        id: 'mode',
        label: tr('表示モードの切り替え（編集 / 分割 / プレビュー）', 'Switch view mode (edit / split / preview)', '切换显示模式（编辑 / 分栏 / 预览）'),
        shortcut: 'Ctrl+Shift+V',
        run: () => this.cycleMode(),
      },
      { id: 'terminal', label: tr('ターミナルの表示切り替え', 'Toggle terminal', '切换终端'), shortcut: 'Ctrl+`', run: () => this.toggleTerminal() },
      { id: 'newTerminal', label: tr('新しいターミナル', 'New terminal', '新建终端'), run: () => this.newTerminal() },
      { id: 'rescan', label: tr('フォルダを読み込み直す', 'Reload folder', '重新加载文件夹'), run: () => void this.rescan() },
      { id: 'reveal', label: tr('エクスプローラーで表示', 'Reveal in File Explorer', '在资源管理器中显示'), run: () => void api.revealInExplorer(doc?.path ?? '') },
      { id: 'print', label: tr('印刷 / PDF に保存', 'Print / Save as PDF', '打印 / 保存为 PDF'), run: () => void this.print() },
      {
        id: 'aiGuide',
        label: tr(
          'AI 向けの説明ファイル（CLAUDE.md / AGENTS.md）を作成',
          'Create guide files for AI (CLAUDE.md / AGENTS.md)',
          '创建面向 AI 的说明文件（CLAUDE.md / AGENTS.md）',
        ),
        run: () => void this.createAiGuide(),
      },
      { id: 'aiRefresh', label: tr('AI ツールを探し直す', 'Rescan for AI tools', '重新查找 AI 工具'), run: () => void this.refreshAiTools() },
      { id: 'update', label: tr('アップデートを確認', 'Check for updates', '检查更新'), run: () => void this.checkForUpdate(true) },
      { id: 'settings', label: tr('設定', 'Settings', '设置'), shortcut: 'Ctrl+,', run: () => (this.settingsOpen = true) },
    ];
    for (const s of this.shells) {
      list.push({
        id: `shell:${s.id}`,
        label: tr(`ターミナル: ${s.name} を開く`, `Terminal: Open ${s.name}`, `终端：打开 ${s.name}`),
        run: () => this.newTerminal({ shellId: s.id }),
      });
    }
    for (const t of this.aiTools.filter((t) => t.found)) {
      list.push({ id: `ai:${t.id}`, label: tr(`AI: ${t.name} を起動`, `AI: Launch ${t.name}`, `AI：启动 ${t.name}`), run: () => void this.launchTool(t) });
      if (doc) {
        for (const a of t.actions) {
          list.push({ id: `ai:${t.id}:${a.id}`, label: `AI: ${t.name} - ${a.label}`, run: () => void this.launchTool(t, a.id) });
        }
      }
    }
    return list;
  }

  showLeft(view: LeftView) {
    this.leftVisible = true;
    this.leftView = view;
    this.saveWorkspaceState();
  }

  toggleLeft() {
    this.leftVisible = !this.leftVisible;
    this.saveWorkspaceState();
  }
}

type WorkspaceState = {
  expanded?: string[];
  tabs?: { path: string; mode: Mode }[];
  active?: string | null;
  leftWidth?: number;
  leftVisible?: boolean;
  leftView?: LeftView;
  terminalHeight?: number;
  terminalVisible?: boolean;
};

export const store = new Store();
