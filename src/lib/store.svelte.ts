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
        this.toast(`ファイルが多すぎるため ${e.payload.files.toLocaleString()} 件で探索を止めました。中のフォルダを開くなど、より狭い範囲を開いてください。`, 'error');
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

    await getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === 'drop') void this.onDrop(e.payload.paths);
    });
    await getCurrentWindow().onCloseRequested(async () => {
      await this.saveAll();
      this.saveWorkspaceStateNow();
    });

    void api.listShells().then((s) => (this.shells = s));
    void this.refreshAiTools();

    // 起動時に指定がなければ、前回開いていたフォルダを開く（なくなっていればようこそ画面）
    const launch = await api.getLaunchRoot();
    this.recent = await api.getRecent();
    const target = launch ?? this.recent[0];
    if (target) await this.openRoot(target);

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
      if (manual) this.toast(`アップデートを確認できません: ${errorMessage(e)}`, 'error');
      return;
    }
    if (!update) {
      if (manual) this.toast(`最新のバージョン（${await getVersion()}）です`);
      return;
    }
    if (!(await api.isInstalled())) {
      // 単体実行版の exe は自分を置き換えられないので、ダウンロードしてもらう
      const ok = await this.confirm(
        'アップデート',
        `新しいバージョン ${update.version} があります（いまは ${update.currentVersion}）。インストールせずに使う版なので、自動では更新できません。リリースのページを開きますか？`,
        'ページを開く',
        'あとで',
      );
      if (ok) await openUrl(RELEASES_URL);
      else this.updateDeclined = true;
      return;
    }
    const running = this.terminals.some((t) => !t.exited);
    const ok = await this.confirm(
      'アップデート',
      `新しいバージョン ${update.version} があります（いまは ${update.currentVersion}）。更新して再起動しますか？編集中のファイルは保存します。` +
        (running ? 'ターミナル（AI ツールを含む）は終了します。' : ''),
      '更新して再起動',
      'あとで',
    );
    if (!ok) {
      this.updateDeclined = true;
      return;
    }
    await this.saveAll();
    this.saveWorkspaceStateNow();
    this.toast(`バージョン ${update.version} をダウンロードしています…`);
    try {
      // Windows ではインストーラーが動くとアプリは終了し、入れ替え後に起動し直す
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      this.toast(`アップデートできませんでした: ${errorMessage(e)}`, 'error');
    }
  }

  // ================= ルートディレクトリ =================

  async chooseRoot() {
    const dir = await openDialog({ directory: true, multiple: false, title: 'フォルダを開く' });
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
    if (/^\\\\(\?\\UNC\\)?wsl(\.localhost|\$)\\/i.test(path)) this.toast('WSL に接続しています…');
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
    if (res.wslError) this.toast(`WSL のエージェントを使えないため、変更の自動反映などが動きません: ${res.wslError}`, 'error');
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
    const ok = await this.confirm(
      '編集内容の復元',
      `前回、保存されないまま終了したファイルが ${pending.length} 件あります。\n${pending.map((p) => '・' + p.path).join('\n')}\n\n編集内容を復元しますか？（復元後に保存すると反映されます）`,
      '復元する',
      '破棄する',
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
        this.toast(`${doc.path} は削除されました。保存すると作り直します。`);
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
      const copy = join(dirname(doc.path), `${basename(doc.path).replace(/(\.[^.]+)?$/, ` (競合 ${stamp})$1`)}`);
      void api
        .createFile(copy, mine)
        .then((p) => this.toast(`自分の変更を ${p} に保存しました`))
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
        const ok = await this.confirm('ファイルを開く', `${msg.slice(NOT_TEXT_PREFIX.length)}。既定のアプリで開きますか？`, '既定のアプリで開く');
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
        const discard = await this.confirm('保存できません', `${doc.path} を保存できませんでした。変更を破棄して閉じますか？`, '破棄して閉じる', 'キャンセル', true);
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
      this.toast(`${doc.path} は読み取り専用です`, 'error');
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
            '文字コード',
            `${doc.path} は ${doc.encoding} で保存されていますが、この文字コードでは表せない文字が含まれています。UTF-8 に変えて保存しますか？`,
            'UTF-8 で保存',
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
    const name = await this.prompt('新しいファイル', `${dir || this.rootName} に作るファイルの名前`, '無題.md', true);
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
    const name = await this.prompt('新しいフォルダ', `${dir || this.rootName} に作るフォルダの名前`, '');
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
    const name = await this.prompt('名前の変更', path, basename(path), true);
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
      this.toast('フォルダをその中に移動することはできません', 'error');
      return;
    }
    await this.saveAll();
    let rewrite = false;
    try {
      const count = await api.countLinkUpdates(from, to);
      if (count > 0) {
        const r = await this.ask({
          title: 'リンクの書き換え',
          message: `${from} へのリンクなど、${count} 件のリンクが影響を受けます。新しい場所に合わせて書き換えますか？`,
          buttons: [
            { label: 'キャンセル', value: 'cancel' },
            { label: '書き換えない', value: 'no' },
            { label: '書き換える', value: 'yes', primary: true },
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
      if (res.links > 0) this.toast(`${res.files.length} 個のファイルで ${res.links} 件のリンクを書き換えました`);
      this.saveWorkspaceState();
    } catch (e) {
      this.toast(errorMessage(e), 'error');
    }
  }

  async remove(path: string) {
    const entry = this.entries.get(path);
    const ok = await this.confirm(
      '削除',
      `${path}${entry?.dir ? ' フォルダとその中身' : ''} をごみ箱に移動しますか？`,
      'ごみ箱に移動',
      'キャンセル',
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
    this.toast(`コピーしました: ${text}`);
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
            title: 'フォルダを開く',
            message: p,
            buttons: [
              { label: 'キャンセル', value: 'cancel' },
              { label: '新しいウィンドウで開く', value: 'new' },
              { label: 'このウィンドウで開く', value: 'here', primary: true },
            ],
          });
          if (r?.button === 'here') await this.openRoot(p);
          else if (r?.button === 'new') await api.openNewWindow(p);
        }
      } else {
        this.toast('開いているフォルダの外のファイルは開けません。フォルダごと開いてください。', 'error');
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
    const ok = await this.confirm(
      'すべて置換',
      `${files.length} 個のファイルで約 ${count} 件を「${replacement}」に置き換えます。よろしいですか？`,
      '置換する',
      'キャンセル',
      true,
    );
    if (!ok) return;
    await this.saveAll();
    try {
      const r = await api.replaceInFiles($state.snapshot(this.searchOptions), replacement, files);
      this.toast(`${r.files.length} 個のファイルで ${r.replacements} 件を置き換えました`);
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
      this.toast('先にフォルダを開いてください', 'error');
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
    const title = opts.title ?? (opts.toolId ? this.aiTools.find((t) => t.id === opts.toolId)?.name : shell?.name) ?? 'ターミナル';
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
        const ok = await this.confirm(tool.name, `${tool.name} が見つかりません。インストール方法のページをブラウザで開きますか？`, '開く');
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
    this.diff = { title: `${doc.path} の変更（AI セッション中）`, before, after: doc.text };
  }

  async revertAiChange(doc: Doc) {
    const before = this.aiBefore.get(doc.path);
    if (before === undefined) return;
    this.setDocText(doc, before);
    await this.save(doc);
    this.dismissAiChange(doc.path);
  }

  async createAiGuide() {
    const body = `# このフォルダについて

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
    for (const name of ['CLAUDE.md', 'AGENTS.md']) {
      if (await api.pathExists(name)) {
        this.toast(`${name} は既にあるので変更しませんでした`);
        continue;
      }
      await api.createFile(name, body);
      this.toast(`${name} を作りました`);
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
      const create = await this.confirm('リンク先がありません', `${target} はありません。作りますか？`, '作る');
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
    const ok = await this.confirm('外部リンク', `ブラウザで開きますか？\n${url}`, '開く');
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
    const label = { editor: 'エディタ', preview: 'プレビュー', terminal: 'ターミナル' }[target];
    this.zoomHint = `${label}の文字: ${size}px`;
    if (this.zoomTimer) clearTimeout(this.zoomTimer);
    this.zoomTimer = setTimeout(() => (this.zoomHint = null), 1200);
    if (this.zoomSaveTimer) clearTimeout(this.zoomSaveTimer);
    this.zoomSaveTimer = setTimeout(() => {
      void api.setConfig($state.snapshot(this.config)).catch((e) => this.toast(errorMessage(e), 'error'));
    }, 500);
  }

  async updateConfig(patch: Partial<Config>) {
    const before = this.config;
    this.config = { ...this.config, ...patch, terminal: { ...this.config.terminal, ...(patch.terminal ?? {}) } };
    try {
      await api.setConfig($state.snapshot(this.config));
    } catch (e) {
      this.toast(errorMessage(e), 'error');
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

  async confirm(title: string, message: string, ok = 'OK', cancel = 'キャンセル', danger = false): Promise<boolean> {
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
        { label: 'キャンセル', value: 'cancel' },
        { label: 'OK', value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return null;
    const v = r.value.trim();
    if (!v) return null;
    if (/[\\:*?"<>|]/.test(v) || v.includes('..')) {
      this.toast('名前に \\ : * ? " < > | や .. は使えません', 'error');
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
      { id: 'open', label: 'フォルダを開く', shortcut: 'Ctrl+O', run: () => void this.chooseRoot() },
      { id: 'openRecent', label: '最近開いたフォルダを開く', shortcut: 'Ctrl+R', run: () => void this.chooseRecentRoot() },
      { id: 'newWindow', label: '新しいウィンドウ', run: () => void api.openNewWindow() },
      { id: 'newFile', label: '新しいファイル', shortcut: 'Ctrl+N', run: () => void this.newFile() },
      { id: 'newFolder', label: '新しいフォルダ', run: () => void this.newFolder() },
      { id: 'save', label: '保存', shortcut: 'Ctrl+S', run: () => doc && void this.save(doc, true) },
      { id: 'saveAll', label: 'すべて保存', run: () => void this.saveAll() },
      { id: 'close', label: 'タブを閉じる', shortcut: 'Ctrl+W', run: () => doc && void this.closeTab(doc.path) },
      { id: 'quickOpen', label: 'ファイルを開く（名前で検索）', shortcut: 'Ctrl+P', run: () => (this.palette = 'files') },
      { id: 'search', label: '全文検索', shortcut: 'Ctrl+Shift+F', run: () => this.showLeft('search') },
      { id: 'files', label: 'ファイルツリーを表示', run: () => this.showLeft('files') },
      { id: 'outline', label: 'アウトラインを表示', run: () => this.showLeft('outline') },
      { id: 'toggleLeft', label: '左ペインの表示切り替え', shortcut: 'Ctrl+Shift+E', run: () => this.toggleLeft() },
      { id: 'mode', label: '表示モードの切り替え（編集 / 分割 / プレビュー）', shortcut: 'Ctrl+Shift+V', run: () => this.cycleMode() },
      { id: 'terminal', label: 'ターミナルの表示切り替え', shortcut: 'Ctrl+`', run: () => this.toggleTerminal() },
      { id: 'newTerminal', label: '新しいターミナル', run: () => this.newTerminal() },
      { id: 'rescan', label: 'フォルダを読み込み直す', run: () => void this.rescan() },
      { id: 'reveal', label: 'エクスプローラーで表示', run: () => void api.revealInExplorer(doc?.path ?? '') },
      { id: 'print', label: '印刷 / PDF に保存', run: () => void this.print() },
      { id: 'aiGuide', label: 'AI 向けの説明ファイル（CLAUDE.md / AGENTS.md）を作成', run: () => void this.createAiGuide() },
      { id: 'aiRefresh', label: 'AI ツールを探し直す', run: () => void this.refreshAiTools() },
      { id: 'update', label: 'アップデートを確認', run: () => void this.checkForUpdate(true) },
      { id: 'settings', label: '設定', shortcut: 'Ctrl+,', run: () => (this.settingsOpen = true) },
    ];
    for (const s of this.shells) {
      list.push({ id: `shell:${s.id}`, label: `ターミナル: ${s.name} を開く`, run: () => this.newTerminal({ shellId: s.id }) });
    }
    for (const t of this.aiTools.filter((t) => t.found)) {
      list.push({ id: `ai:${t.id}`, label: `AI: ${t.name} を起動`, run: () => void this.launchTool(t) });
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
