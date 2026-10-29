// Git 連携の状態と操作（docs/git.md）。左ペインの Git タブ、右ペインの差分タブ、ファイルツリーの印、
// エディタの変更の帯、ステータスバーのブランチ表示がここを読む

import { SvelteSet } from 'svelte/reactivity';
import { listen } from '@tauri-apps/api/event';
import { errorMessage } from './api';
import { i18n, tr } from './i18n.svelte';
import { basename, dirname, extname } from './paths';
import { store, type Doc } from './store.svelte';
import {
  gitApi,
  groupStatus,
  invalidBranchName,
  repoToRoot,
  rootToRepo,
  shortHash,
  treeMark,
  type ChangedFile,
  type Commit,
  type CommitDetail,
  type FileStatus,
  type GitInfo,
  type Op,
  type Refs,
  type RepoInfo,
  type Section,
  type Stash,
  type StatusResult,
} from './git';

// ================= 右ペインのタブ =================

/** 差分の片側。rev: null は作業ツリー、'' はステージ、それ以外はコミット（`abc^` なども可） */
export type DiffSide = { rev: string | null; label: string };

/**
 * 差分タブの中身。
 * - stage: 'worktree' はステージ ↔ 作業ツリー（右を編集でき、まとまりごとにステージ・破棄）
 * - stage: 'index' は HEAD ↔ ステージ（まとまりごとにステージを外す）
 * - stage: 'none' は読み取り専用（コミットの差分など）
 */
export type DiffSpec = { repo: string; path: string; orig: string | null; left: DiffSide; right: DiffSide; stage: 'worktree' | 'index' | 'none' };

export type GitTabKind = 'diff' | 'rev' | 'tree' | 'compare';

export class GitTab {
  key: string;
  kind: GitTabKind;
  title = $state('');
  pinned = $state(false);
  /** 中身を読み直す合図（ステージなどで変わったとき） */
  version = $state(0);
  diff: DiffSpec | null = null;
  /** この時点の内容（読み取り専用） */
  rev: { repo: string; rev: string; path: string } | null = null;
  /** この時点のファイルの一覧 */
  tree: { repo: string; rev: string; label: string } | null = null;
  /** 2 つのブランチ・コミットの比較 */
  compare: { repo: string; base: string; target: string } | null = null;

  constructor(key: string, kind: GitTabKind, title: string) {
    this.key = key;
    this.kind = kind;
    this.title = title;
  }
}

// ================= 入力欄やチェックボックスのあるダイアログ =================

export type FormField =
  | { kind: 'text'; id: string; label: string; value: string; placeholder?: string; multiline?: boolean }
  | { kind: 'check'; id: string; label: string; value: boolean }
  | { kind: 'radio'; id: string; label?: string; value: string; options: { value: string; label: string }[] };

export type FormRequest = {
  title: string;
  message?: string;
  /** 一覧（取り込まれるコミットなど） */
  items?: { text: string; muted?: boolean; mark?: string }[];
  /** git の出力など、等幅で見せる文字列 */
  output?: string;
  fields?: FormField[];
  buttons: { label: string; value: string; primary?: boolean; danger?: boolean }[];
  resolve: (r: { button: string; values: Record<string, string | boolean> } | null) => void;
};

type DraftState = { message?: string; history?: string[] };

/** 長い時間かかったら自動の更新を止める（ミリ秒） */
const SLOW_STATUS = 5000;
const LOG_PAGE = 200;

class GitStore {
  // ---- 検出 ----
  info = $state<GitInfo | null>(null);
  repoPath = $state<string | null>(null);
  /** 開いているファイルを含むリポジトリに自動で切り替えるか（ユーザーが選んだら止める） */
  private repoChosen = false;

  // ---- 状態 ----
  status = $state<StatusResult | null>(null);
  refs = $state<Refs | null>(null);
  stashes = $state<Stash[]>([]);
  error = $state<string | null>(null);
  /** 状態の取得に時間がかかるので自動の更新を止めている */
  slow = $state(false);
  loading = $state(false);

  // ---- 履歴 ----
  log = $state<Commit[]>([]);
  logDone = $state(false);
  /** 表示するもの: 'HEAD'、'all'、またはブランチ名など */
  logRef = $state('HEAD');
  /** ファイルの履歴（リポジトリの中のパス） */
  historyPath = $state<string | null>(null);
  logFilter = $state('');
  /** 現在のブランチにないコミット（リモートのブランチの履歴を見ているとき強調する） */
  notInHead = new SvelteSet<string>();
  selectedCommits = $state<string[]>([]);
  expandedCommit = $state<string | null>(null);
  commitDetail = $state<CommitDetail | null>(null);
  private logSig = '';

  // ---- コミット ----
  message = $state('');
  history: string[] = [];
  private historyIndex = -1;
  /** 連続したチェリーピックの総数（進み具合の表示用） */
  sequenceTotal = $state(0);

  // ---- 実行中の操作 ----
  busy = $state<string | null>(null);
  progress = $state('');
  opId = $state<number | null>(null);
  private opSeq = 0;

  // ---- 画面 ----
  collapsed = new SvelteSet<string>(['branches', 'remotes', 'stashes', 'tags']);
  flat = $state(true);
  form = $state<FormRequest | null>(null);
  /** ファイルツリーの印（ルートからの相対パス → 記号）と、中に変更のあるフォルダ */
  marks = $state(new Map<string, string>());
  dirMarks = $state(new Set<string>());

  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  private draftTimer: ReturnType<typeof setTimeout> | null = null;
  private fetchTimer: ReturnType<typeof setInterval> | null = null;
  private refreshing: Promise<void> | null = null;
  private refreshAgain = false;

  get repo(): RepoInfo | null {
    return this.info?.repos.find((r) => r.path === this.repoPath) ?? null;
  }

  get enabled(): boolean {
    return store.config.gitEnabled !== false;
  }

  groups = $derived(groupStatus(this.status?.status.files ?? []));
  changeCount = $derived(this.status ? this.status.status.files.length : 0);
  branch = $derived(this.status?.status.branch ?? null);
  repoState = $derived(this.status?.state ?? null);

  // ================= 起動・検出 =================

  async init() {
    await listen<{ op: number | null; line: string }>('git:progress', (e) => {
      if (e.payload.op !== null && e.payload.op === this.opId) this.progress = e.payload.line;
    });
    await listen('git:changed', () => this.scheduleRefresh());
    window.addEventListener('focus', () => this.scheduleRefresh(0));
  }

  /** フォルダを開いた・探索が終わったときに、リポジトリを探す */
  async detect() {
    this.stopAutoFetch();
    if (!store.root || !this.enabled) {
      this.reset();
      return;
    }
    // ルートの下の .git を持つフォルダ（探索の結果から）
    const nested: string[] = [];
    for (const e of store.entries.values()) {
      if (basename(e.path) === '.git' && e.path !== '.git') nested.push(dirname(e.path));
    }
    let info: GitInfo;
    try {
      info = await gitApi.repos(nested);
    } catch (e) {
      this.reset();
      this.error = errorMessage(e);
      return;
    }
    this.info = info;
    if (!info.repos.some((r) => r.path === this.repoPath)) {
      this.repoPath = info.repos[0]?.path ?? null;
      this.repoChosen = false;
    }
    const saved = store.savedGitRepo;
    if (saved && info.repos.some((r) => r.path === saved)) {
      this.repoPath = saved;
      this.repoChosen = true;
    }
    void gitApi.watch(info.repos.map((r) => r.path)).catch(() => {});
    await this.selectRepo(this.repoPath, false);
    this.startAutoFetch();
  }

  reset() {
    this.info = null;
    this.repoPath = null;
    this.status = null;
    this.refs = null;
    this.stashes = [];
    this.log = [];
    this.logSig = '';
    this.error = null;
    this.marks = new Map();
    this.dirMarks = new Set();
    this.historyPath = null;
    this.logRef = 'HEAD';
    this.selectedCommits = [];
    this.expandedCommit = null;
  }

  async selectRepo(path: string | null, byUser = true) {
    if (byUser) this.repoChosen = true;
    if (this.repoPath !== path) {
      this.saveDraftNow();
      this.repoPath = path;
      this.status = null;
      this.refs = null;
      this.log = [];
      this.logSig = '';
      this.historyPath = null;
      this.logRef = 'HEAD';
      this.selectedCommits = [];
      this.expandedCommit = null;
    }
    if (byUser) store.saveWorkspaceState();
    await this.loadDraft();
    await this.refresh();
  }

  /** 開いているファイルを含むリポジトリ（複数あるとき） */
  repoForRoot(rel: string): RepoInfo | null {
    const repos = this.info?.repos ?? [];
    let best: RepoInfo | null = null;
    for (const r of repos) {
      if (rootToRepo(r, rel) === null) continue;
      if (!best || (r.prefix ?? '').length > (best.prefix ?? '').length) best = r;
    }
    return best;
  }

  /** 開いているファイルが別のリポジトリにあれば切り替える（ユーザーが選んでいなければ） */
  followActiveFile(path: string | null) {
    if (!path || this.repoChosen || (this.info?.repos.length ?? 0) < 2) return;
    const r = this.repoForRoot(path);
    if (r && r.path !== this.repoPath) void this.selectRepo(r.path, false);
  }

  // ================= 更新 =================

  scheduleRefresh(delay = 300) {
    if (!this.repoPath || this.slow) return;
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(() => void this.refresh(), delay);
  }

  /** ファイルの変更の通知から。作業ツリーか .git の中が変わったら更新する */
  onFsChange(paths: string[]) {
    const repo = this.repo;
    if (!repo || !this.enabled) return;
    if (paths.some((p) => rootToRepo(repo, p) !== null || p.split('/').includes('.git'))) this.scheduleRefresh();
  }

  async refresh() {
    if (this.refreshing) {
      this.refreshAgain = true;
      return this.refreshing;
    }
    this.refreshing = this.doRefresh().finally(() => {
      this.refreshing = null;
      if (this.refreshAgain) {
        this.refreshAgain = false;
        void this.refresh();
      }
    });
    return this.refreshing;
  }

  private async doRefresh() {
    const repo = this.repoPath;
    if (!repo) return;
    this.loading = true;
    const started = performance.now();
    try {
      const [status, refs, stashes] = await Promise.all([gitApi.status(repo), gitApi.refs(repo), gitApi.stashes(repo)]);
      if (repo !== this.repoPath) return;
      this.slow = performance.now() - started > SLOW_STATUS;
      this.status = status;
      this.refs = refs;
      this.stashes = stashes;
      this.error = null;
      this.updateMarks();
      this.bumpTabs();
      // コミットやブランチが変わったときだけ履歴を読み直す
      const sig = [status.status.branch.oid, ...refs.branches.map((b) => b.hash), ...refs.remoteBranches.map((b) => b.hash), ...refs.tags.map((t) => t.hash)].join(',');
      if (sig !== this.logSig) {
        this.logSig = sig;
        await this.loadLog(true);
      }
      void this.updateBase(store.activeDoc);
    } catch (e) {
      if (repo === this.repoPath) this.error = errorMessage(e);
    } finally {
      this.loading = false;
    }
  }

  private updateMarks() {
    const repo = this.repo;
    const marks = new Map<string, string>();
    const dirs = new Set<string>();
    if (repo && store.config.gitTreeColors !== false) {
      for (const f of this.status?.status.files ?? []) {
        const rel = repoToRoot(repo, f.path.replace(/\/$/, ''));
        if (rel === null) continue;
        marks.set(rel, treeMark(f));
        let d = dirname(rel);
        while (d) {
          dirs.add(d);
          d = dirname(d);
        }
      }
    }
    this.marks = marks;
    this.dirMarks = dirs;
  }

  // ================= 履歴 =================

  async loadLog(reset: boolean) {
    const repo = this.repoPath;
    if (!repo) return;
    const skip = reset ? 0 : this.log.length;
    const all = this.logRef === 'all';
    const refs = all || this.logRef === 'HEAD' ? [] : [this.logRef];
    try {
      const limit = this.logFilter ? 2000 : LOG_PAGE;
      const list = await gitApi.log(repo, { refs, all, path: this.historyPath, skip, limit });
      if (repo !== this.repoPath) return;
      this.log = reset ? list : [...this.log, ...list];
      this.logDone = list.length < limit;
      if (reset) await this.loadNotInHead();
    } catch (e) {
      if (reset) this.log = [];
      this.logDone = true;
      store.toast(errorMessage(e), 'error');
    }
  }

  private async loadNotInHead() {
    this.notInHead.clear();
    const repo = this.repoPath;
    if (!repo || this.logRef === 'HEAD' || this.logRef === 'all' || !this.branch?.oid) return;
    try {
      const list = await gitApi.log(repo, { refs: [`HEAD..${this.logRef}`], limit: 1000 });
      for (const c of list) this.notInHead.add(c.hash);
    } catch {
      // 印が付かないだけ
    }
  }

  async showHistory(ref: string) {
    this.logRef = ref;
    this.historyPath = null;
    this.collapsed.delete('history');
    this.selectedCommits = [];
    this.expandedCommit = null;
    store.showLeft('git');
    await this.loadLog(true);
  }

  /** このファイルの履歴（ルートからの相対パス） */
  async showFileHistory(rel: string) {
    const repo = this.repoForRoot(rel);
    if (!repo) {
      store.toast(tr('このファイルは Git で管理されていません', 'This file is not in a Git repository', '此文件不在 Git 仓库中'), 'error');
      return;
    }
    if (repo.path !== this.repoPath) await this.selectRepo(repo.path);
    this.historyPath = rootToRepo(repo, rel);
    this.logRef = 'HEAD';
    this.collapsed.delete('history');
    this.expandedCommit = null;
    this.selectedCommits = [];
    store.showLeft('git');
    await this.loadLog(true);
  }

  async clearFileHistory() {
    this.historyPath = null;
    await this.loadLog(true);
  }

  async toggleCommit(hash: string) {
    if (this.expandedCommit === hash) {
      this.expandedCommit = null;
      this.commitDetail = null;
      return;
    }
    this.expandedCommit = hash;
    this.commitDetail = null;
    try {
      const d = await gitApi.commitDetail(this.repoPath!, hash);
      if (this.expandedCommit === hash) this.commitDetail = d;
    } catch (e) {
      store.toast(errorMessage(e), 'error');
    }
  }

  selectCommit(hash: string, e: MouseEvent) {
    if (e.ctrlKey) {
      this.selectedCommits = this.selectedCommits.includes(hash) ? this.selectedCommits.filter((h) => h !== hash) : [...this.selectedCommits, hash];
    } else if (e.shiftKey && this.selectedCommits.length) {
      const hashes = this.log.map((c) => c.hash);
      const a = hashes.indexOf(this.selectedCommits[this.selectedCommits.length - 1]);
      const b = hashes.indexOf(hash);
      const [s, t] = a < b ? [a, b] : [b, a];
      this.selectedCommits = hashes.slice(s, t + 1);
    } else {
      this.selectedCommits = [hash];
      void this.toggleCommit(hash);
    }
  }

  // ================= 実行 =================

  /** 操作を実行し、終わったら状態を読み直す。失敗したら git の出力を見せる */
  private async run(label: string, op: Op, opts: { remote?: boolean; quiet?: boolean } = {}): Promise<{ ok: boolean; output: string }> {
    const repo = this.repoPath;
    if (!repo) return { ok: false, output: '' };
    const id = ++this.opSeq;
    this.busy = label;
    this.progress = '';
    this.opId = opts.remote ? id : null;
    try {
      const output = await gitApi.op(repo, op, opts.remote ? id : undefined);
      return { ok: true, output };
    } catch (e) {
      const output = errorMessage(e);
      if (!opts.quiet) await this.showError(label, output, op, !!opts.remote);
      return { ok: false, output };
    } finally {
      this.busy = null;
      this.opId = null;
      this.progress = '';
      await this.refresh();
    }
  }

  private async showError(label: string, output: string, op: Op, remote: boolean) {
    // 競合で止まったときは、競合のセクションで解決してもらう
    await this.refresh();
    if (this.repoState?.kind && /conflict|CONFLICT/.test(output)) {
      store.toast(tr('競合しました。「競合」の一覧から解決してください', 'There are conflicts. Resolve them from the Conflicts list', '存在冲突。请在“冲突”列表中解决'), 'error');
      this.collapsed.delete('changes');
      return;
    }
    if (/Cancelled|取り消しました|已取消/.test(output)) return;
    const buttons = [{ label: tr('閉じる', 'Close', '关闭'), value: 'close', primary: true }];
    const command = commandLine(op);
    if (remote && command) buttons.unshift({ label: tr('ターミナルで実行', 'Run in terminal', '在终端中运行'), value: 'terminal', primary: false });
    const r = await this.ask({
      title: tr(`${label}できませんでした`, `${label} failed`, `${label}失败`),
      message: remote
        ? tr(
            '認証が必要な場合は「ターミナルで実行」で同じコマンドを実行できます。',
            'If authentication is required, use "Run in terminal" to run the same command.',
            '如需身份验证，可通过“在终端中运行”执行相同命令。',
          )
        : undefined,
      output,
      buttons,
    });
    if (r?.button === 'terminal' && command) this.runInTerminal(command);
  }

  runInTerminal(command: string) {
    const repo = this.repo;
    const cwd = repo?.prefix ?? null;
    store.newTerminal({ cwd: cwd ?? '', title: 'git', input: command });
  }

  cancel() {
    if (this.opId !== null) void gitApi.cancel(this.opId);
  }

  ask(req: Omit<FormRequest, 'resolve'>): Promise<{ button: string; values: Record<string, string | boolean> } | null> {
    return new Promise((resolve) => {
      this.form = { ...req, resolve };
    });
  }

  // ================= 変更とステージ =================

  /** 変更の一覧のファイル → ルートからの相対パス（ルートの外なら null） */
  rootPath(path: string): string | null {
    return this.repo ? repoToRoot(this.repo, path) : null;
  }

  async stage(files: FileStatus[]) {
    if (files.length) await this.run(tr('ステージ', 'Stage', '暂存'), { type: 'stage', paths: files.map((f) => f.path) });
  }

  async unstage(files: FileStatus[]) {
    if (files.length) await this.run(tr('ステージの取り消し', 'Unstage', '取消暂存'), { type: 'unstage', paths: files.map((f) => f.path) });
  }

  async stageAll() {
    await this.run(tr('ステージ', 'Stage', '暂存'), { type: 'stageAll' });
  }

  async unstageAll() {
    await this.run(tr('ステージの取り消し', 'Unstage', '取消暂存'), { type: 'unstageAll' });
  }

  /** 作業ツリーの変更を破棄する。未追跡のファイルはごみ箱へ移す */
  async discard(files: FileStatus[]) {
    if (!files.length) return;
    const names = files.slice(0, 10).map((f) => '・' + f.path).join('\n') + (files.length > 10 ? `\n… (${files.length})` : '');
    const ok = await store.confirm(
      tr('変更の破棄', 'Discard changes', '放弃更改'),
      tr(
        `次のファイルの変更を破棄します。元に戻せません（未追跡のファイルはごみ箱へ移します）。\n${names}`,
        `Discard changes to these files? This can't be undone (untracked files are moved to the Recycle Bin).\n${names}`,
        `将放弃以下文件的更改，且无法撤销（未跟踪的文件将移到回收站）。\n${names}`,
      ),
      tr('破棄する', 'Discard', '放弃'),
      undefined,
      true,
    );
    if (!ok) return;
    const untracked = files.filter((f) => f.untracked).map((f) => f.path);
    const tracked = files.filter((f) => !f.untracked).map((f) => f.path);
    if (tracked.length) await this.run(tr('変更の破棄', 'Discard', '放弃更改'), { type: 'discard', paths: tracked });
    if (untracked.length) await this.run(tr('変更の破棄', 'Discard', '放弃更改'), { type: 'discardUntracked', paths: untracked });
  }

  async setIndex(path: string, text: string, encoding: string, eol: string) {
    return this.run(tr('ステージ', 'Stage', '暂存'), { type: 'setIndex', path, text, encoding: encoding || 'utf-8', eol: eol || 'lf' });
  }

  /** ファイル（ルートからの相対パス）のステージの中身を text にする（エディタの変更の帯から） */
  async stageText(rel: string, text: string, encoding: string, eol: string) {
    const repo = this.repoForRoot(rel);
    if (!repo) return;
    if (repo.path !== this.repoPath) await this.selectRepo(repo.path);
    await this.setIndex(rootToRepo(repo, rel)!, text, encoding, eol);
  }

  async addToGitignore(rel: string) {
    const repo = this.repoForRoot(rel);
    if (!repo) return;
    if (repo.path !== this.repoPath) await this.selectRepo(repo.path);
    const path = rootToRepo(repo, rel)!;
    const isDir = store.entries.get(rel)?.dir;
    const ext = extname(path);
    const options = [{ value: '/' + path + (isDir ? '/' : ''), label: tr(`このファイル（/${path}）`, `This file (/${path})`, `此文件（/${path}）`) }];
    if (isDir) options[0].label = tr(`このフォルダ（/${path}/）`, `This folder (/${path}/)`, `此文件夹（/${path}/）`);
    if (!isDir) options.push({ value: basename(path), label: tr(`同じ名前のファイル（${basename(path)}）`, `Files with this name (${basename(path)})`, `同名文件（${basename(path)}）`) });
    if (!isDir && ext) options.push({ value: '*' + ext, label: tr(`同じ拡張子のファイル（*${ext}）`, `Files with this extension (*${ext})`, `同扩展名的文件（*${ext}）`) });
    if (!isDir && dirname(path)) options.push({ value: '/' + dirname(path) + '/', label: tr(`このフォルダ（/${dirname(path)}/）`, `The folder (/${dirname(path)}/)`, `此文件夹（/${dirname(path)}/）`) });
    const r = await this.ask({
      title: tr('.gitignore に追加', 'Add to .gitignore', '添加到 .gitignore'),
      fields: [{ kind: 'radio', id: 'line', value: options[0].value, options }],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('追加', 'Add', '添加'), value: 'ok', primary: true },
      ],
    });
    if (r?.button === 'ok') await this.run('.gitignore', { type: 'appendGitignore', line: String(r.values.line) });
  }

  // ================= コミット =================

  /** コミットする。opts.push はコミットしてプッシュ */
  async commit(opts: { amend?: boolean; push?: boolean; signoff?: boolean } = {}) {
    const st = this.status;
    if (!st || this.busy) return;
    const message = this.message.trim();
    if (!message && !opts.amend) {
      store.toast(tr('コミットメッセージを入力してください', 'Enter a commit message', '请输入提交信息'), 'error');
      return;
    }
    if (this.groups.conflict.length) {
      store.toast(tr('競合を解決してからコミットしてください', 'Resolve the conflicts before committing', '请先解决冲突再提交'), 'error');
      return;
    }
    if (!(await this.ensureUser())) return;
    if (this.groups.staged.length === 0 && !opts.amend) {
      if (this.groups.unstaged.length === 0 && this.groups.untracked.length === 0) {
        if (this.repoState?.kind !== 'merge') {
          store.toast(tr('コミットする変更がありません', 'There are no changes to commit', '没有可提交的更改'), 'error');
          return;
        }
      } else if (store.config.gitCommitAll !== 'always') {
        const r = await this.ask({
          title: tr('コミット', 'Commit', '提交'),
          message: tr(
            'ステージ済みの変更がありません。すべての変更をステージしてコミットしますか？',
            'There are no staged changes. Stage all changes and commit?',
            '没有已暂存的更改。要暂存所有更改并提交吗？',
          ),
          fields: [{ kind: 'check', id: 'always', label: tr('次からは確認しない', "Don't ask again", '以后不再询问'), value: false }],
          buttons: [
            { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
            { label: tr('すべてステージしてコミット', 'Stage all and commit', '全部暂存并提交'), value: 'ok', primary: true },
          ],
        });
        if (r?.button !== 'ok') return;
        if (r.values.always) await store.updateConfig({ gitCommitAll: 'always' });
      }
      if (!(await this.run(tr('ステージ', 'Stage', '暂存'), { type: 'stageAll' })).ok) return;
    }
    if (opts.amend && this.branch?.upstream && this.branch.ahead === 0) {
      const ok = await store.confirm(
        tr('修正コミット', 'Amend commit', '修改提交'),
        tr(
          '直前のコミットはプッシュ済みです。修正するとリモートと食い違い、強制プッシュが必要になります。続けますか？',
          'The last commit has already been pushed. Amending it will diverge from the remote and require a force push. Continue?',
          '上一次提交已推送。修改后将与远程不一致，需要强制推送。要继续吗？',
        ),
        tr('修正する', 'Amend', '修改'),
        undefined,
        true,
      );
      if (!ok) return;
    }
    let msg = message;
    if (opts.amend && !msg) {
      const d = await gitApi.commitDetail(this.repoPath!, 'HEAD').catch(() => null);
      msg = d?.message ?? '';
    }
    const r = await this.run(tr('コミット', 'Commit', '提交'), { type: 'commit', message: msg, amend: !!opts.amend, signoff: !!opts.signoff });
    if (!r.ok) return;
    this.pushHistory(message);
    this.message = '';
    this.sequenceTotal = 0;
    this.saveDraft();
    if (opts.push) await this.push();
  }

  /** user.name / user.email が設定されていなければ入力してもらう */
  private async ensureUser(): Promise<boolean> {
    const repo = this.repoPath!;
    const [name, email] = await Promise.all([gitApi.configGet(repo, 'user.name'), gitApi.configGet(repo, 'user.email')]).catch(() => [null, null]);
    if (name && email) return true;
    const r = await this.ask({
      title: tr('作者の名前とメールアドレス', 'Author name and email', '作者姓名和邮箱'),
      message: tr(
        'コミットに記録する名前とメールアドレスが設定されていません。',
        'The name and email recorded in commits are not set.',
        '尚未设置提交中记录的姓名和邮箱。',
      ),
      fields: [
        { kind: 'text', id: 'name', label: tr('名前', 'Name', '姓名'), value: name ?? '' },
        { kind: 'text', id: 'email', label: tr('メールアドレス', 'Email', '邮箱'), value: email ?? '' },
        {
          kind: 'radio',
          id: 'scope',
          value: 'global',
          options: [
            { value: 'global', label: tr('すべてのリポジトリで使う（--global）', 'Use for all repositories (--global)', '用于所有仓库（--global）') },
            { value: 'local', label: tr('このリポジトリだけで使う', 'Use for this repository only', '仅用于此仓库') },
          ],
        },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('保存', 'Save', '保存'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return false;
    const n = String(r.values.name).trim();
    const e = String(r.values.email).trim();
    if (!n || !e) return false;
    return (await this.run(tr('設定', 'Configure', '设置'), { type: 'setUser', name: n, email: e, global: r.values.scope === 'global' })).ok;
  }

  private pushHistory(message: string) {
    if (!message) return;
    this.history = [message, ...this.history.filter((m) => m !== message)].slice(0, 10);
    this.historyIndex = -1;
  }

  /** メッセージ欄の ↑ / ↓ で直前のメッセージを選ぶ。欄が空か、履歴を選んでいる間だけ */
  browseHistory(dir: 1 | -1): boolean {
    if (this.message && this.historyIndex < 0) return false;
    const next = this.historyIndex + dir;
    if (next < -1 || next >= this.history.length) return true;
    this.historyIndex = next;
    this.message = next < 0 ? '' : this.history[next];
    return true;
  }

  onMessageInput() {
    this.historyIndex = -1;
    this.saveDraft();
  }

  private async loadDraft() {
    const repo = this.repoPath;
    if (!repo) return;
    const s = ((await gitApi.loadState(repo).catch(() => null)) ?? {}) as DraftState;
    if (repo !== this.repoPath) return;
    this.message = s.message ?? '';
    this.history = s.history ?? [];
    this.historyIndex = -1;
  }

  saveDraft() {
    if (this.draftTimer) clearTimeout(this.draftTimer);
    this.draftTimer = setTimeout(() => this.saveDraftNow(), 800);
  }

  saveDraftNow() {
    if (this.draftTimer) clearTimeout(this.draftTimer);
    if (!this.repoPath) return;
    void gitApi.saveState(this.repoPath, { message: this.message, history: this.history } satisfies DraftState).catch(() => {});
  }

  // ================= ブランチ =================

  async checkout(name: string) {
    await this.withStashRetry(tr('ブランチの切り替え', 'Switch branch', '切换分支'), { type: 'checkout', name });
  }

  /** リモートのブランチをチェックアウト（同じ名前のローカルブランチを作る。あればそれに切り替える） */
  async checkoutRemote(remote: string, name: string) {
    if (this.refs?.branches.some((b) => b.name === name)) return this.checkout(name);
    await this.withStashRetry(tr('ブランチの切り替え', 'Switch branch', '切换分支'), { type: 'checkoutTrack', remoteRef: `${remote}/${name}`, local: name });
  }

  async checkoutDetached(rev: string) {
    const ok = await store.confirm(
      tr('コミットのチェックアウト', 'Check out commit', '检出提交'),
      tr(
        `${shortHash(rev)} をチェックアウトします。どのブランチにもいない状態（デタッチ）になります。ここでコミットするときは、先にブランチを作ってください。`,
        `Check out ${shortHash(rev)}? You'll be in a detached HEAD state (not on any branch). Create a branch before committing here.`,
        `将检出 ${shortHash(rev)}，处于不在任何分支上的状态（分离 HEAD）。如要在此提交，请先创建分支。`,
      ),
      tr('チェックアウト', 'Check out', '检出'),
    );
    if (ok) await this.withStashRetry(tr('チェックアウト', 'Check out', '检出'), { type: 'checkoutDetached', rev });
  }

  /** 作業ツリーの変更が妨げて失敗したら、スタッシュしてからやり直すか尋ねる */
  private async withStashRetry(label: string, op: Op): Promise<boolean> {
    const r = await this.run(label, op, { quiet: true });
    if (r.ok) return true;
    if (/would be overwritten|commit your changes or stash them|Please commit or stash|local changes/i.test(r.output)) {
      const a = await this.ask({
        title: label,
        message: tr(
          '作業ツリーの変更があるため実行できません。変更をスタッシュしてから実行しますか？（スタッシュは「スタッシュ」の一覧から取り出せます）',
          'Local changes prevent this. Stash the changes and continue? (You can restore them from the Stashes list.)',
          '由于存在本地更改，无法执行。要先贮藏更改再执行吗？（可从“贮藏”列表中恢复）',
        ),
        output: r.output,
        buttons: [
          { label: tr('やめる', 'Cancel', '取消'), value: 'cancel' },
          { label: tr('スタッシュして実行', 'Stash and continue', '贮藏并继续'), value: 'ok', primary: true },
        ],
      });
      if (a?.button !== 'ok') return false;
      const s = await this.run(tr('スタッシュ', 'Stash', '贮藏'), { type: 'stashPush', message: `mark-shell: ${label}`, untracked: true });
      if (!s.ok) return false;
      return (await this.run(label, op)).ok;
    }
    await this.showError(label, r.output, op, false);
    return false;
  }

  async createBranch(start: string | null = null) {
    const r = await this.ask({
      title: tr('ブランチの作成', 'Create branch', '创建分支'),
      message: start
        ? tr(`${start.length === 40 ? shortHash(start) : start} から作ります`, `From ${start.length === 40 ? shortHash(start) : start}`, `从 ${start.length === 40 ? shortHash(start) : start} 创建`)
        : tr('現在のコミットから作ります', 'From the current commit', '从当前提交创建'),
      fields: [
        { kind: 'text', id: 'name', label: tr('ブランチ名', 'Branch name', '分支名'), value: '', placeholder: 'feature/...' },
        { kind: 'check', id: 'checkout', label: tr('作ったブランチに切り替える', 'Switch to the new branch', '切换到新分支'), value: true },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('作成', 'Create', '创建'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return;
    const name = String(r.values.name).trim();
    if (invalidBranchName(name)) {
      store.toast(tr(`ブランチ名に使えない名前です: ${name}`, `Invalid branch name: ${name}`, `无效的分支名：${name}`), 'error');
      return;
    }
    await this.run(tr('ブランチの作成', 'Create branch', '创建分支'), { type: 'createBranch', name, start, checkout: !!r.values.checkout });
  }

  async renameBranch(from: string) {
    const to = await store.prompt(tr('ブランチ名の変更', 'Rename branch', '重命名分支'), tr('新しい名前', 'New name', '新名称'), from);
    if (!to || to === from) return;
    if (invalidBranchName(to)) {
      store.toast(tr(`ブランチ名に使えない名前です: ${to}`, `Invalid branch name: ${to}`, `无效的分支名：${to}`), 'error');
      return;
    }
    await this.run(tr('ブランチ名の変更', 'Rename branch', '重命名分支'), { type: 'renameBranch', from, to });
  }

  async deleteBranch(name: string) {
    const ok = await store.confirm(
      tr('ブランチの削除', 'Delete branch', '删除分支'),
      tr(`ブランチ ${name} を削除しますか？`, `Delete branch ${name}?`, `要删除分支 ${name} 吗？`),
      tr('削除', 'Delete', '删除'),
      undefined,
      true,
    );
    if (!ok) return;
    const r = await this.run(tr('ブランチの削除', 'Delete branch', '删除分支'), { type: 'deleteBranch', name, force: false }, { quiet: true });
    if (r.ok) return;
    if (/not fully merged/.test(r.output)) {
      const force = await store.confirm(
        tr('ブランチの削除', 'Delete branch', '删除分支'),
        tr(
          `${name} にはマージされていないコミットがあります。削除するとそのコミットは失われます。それでも削除しますか？`,
          `${name} has commits that are not merged. Deleting it will lose them. Delete anyway?`,
          `${name} 包含尚未合并的提交，删除后这些提交将丢失。仍要删除吗？`,
        ),
        tr('強制的に削除', 'Force delete', '强制删除'),
        undefined,
        true,
      );
      if (force) await this.run(tr('ブランチの削除', 'Delete branch', '删除分支'), { type: 'deleteBranch', name, force: true });
    } else {
      await this.showError(tr('ブランチの削除', 'Delete branch', '删除分支'), r.output, { type: 'deleteBranch', name, force: false }, false);
    }
  }

  async deleteRemoteBranch(remote: string, name: string) {
    const ok = await store.confirm(
      tr('リモートのブランチの削除', 'Delete remote branch', '删除远程分支'),
      tr(
        `リモート ${remote} のブランチ ${name} を削除します。ほかの人も使っている場合は影響します。削除しますか？`,
        `Delete branch ${name} on remote ${remote}? This affects anyone else using it.`,
        `将删除远程 ${remote} 上的分支 ${name}，会影响其他使用者。要删除吗？`,
      ),
      tr('削除', 'Delete', '删除'),
      undefined,
      true,
    );
    if (ok) await this.run(tr('リモートのブランチの削除', 'Delete remote branch', '删除远程分支'), { type: 'deleteRemoteBranch', remote, name }, { remote: true });
  }

  // ================= マージ =================

  /** ブランチ・タグ・コミットを現在のブランチにマージする */
  async merge(rev: string, label = rev) {
    const repo = this.repoPath;
    if (!repo) return;
    const head = this.branch?.head ?? tr('（デタッチ）', '(detached)', '（分离）');
    const commits = await gitApi.log(repo, { refs: [`HEAD..${rev}`], limit: 101 }).catch(() => [] as Commit[]);
    if (commits.length === 0 && this.branch?.oid) {
      store.toast(tr(`${label} はすでに取り込まれています`, `${label} is already merged`, `${label} 已合并`));
      return;
    }
    const canFf = this.branch?.oid ? await gitApi.log(repo, { refs: [`${rev}..HEAD`], limit: 1 }).then((l) => l.length === 0).catch(() => false) : true;
    const n = commits.length > 100 ? '100+' : String(commits.length);
    const r = await this.ask({
      title: tr('マージ', 'Merge', '合并'),
      message: tr(
        `${label} を ${head} にマージします。取り込まれるコミット: ${n} 件`,
        `Merge ${label} into ${head}. Commits to bring in: ${n}`,
        `将 ${label} 合并到 ${head}。将引入的提交：${n} 个`,
      ),
      items: commits.slice(0, 100).map((c) => ({ text: c.subject, mark: shortHash(c.hash) })),
      fields: [
        {
          kind: 'radio',
          id: 'mode',
          value: 'ff',
          options: [
            {
              value: 'ff',
              label: canFf
                ? tr('早送り（マージコミットを作らない）', 'Fast-forward (no merge commit)', '快进（不创建合并提交）')
                : tr('マージコミットを作る（早送りはできません）', 'Create a merge commit (fast-forward not possible)', '创建合并提交（无法快进）'),
            },
            { value: 'no-ff', label: tr('常にマージコミットを作る（--no-ff）', 'Always create a merge commit (--no-ff)', '始终创建合并提交（--no-ff）') },
            {
              value: 'squash',
              label: tr('1 つのコミットにまとめる（--squash。コミットはメッセージ欄から）', 'Squash into one commit (--squash; commit from the message box)', '压缩为一个提交（--squash，从提交信息框提交）'),
            },
          ],
        },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('マージ', 'Merge', '合并'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return;
    const mode = r.values.mode as 'ff' | 'no-ff' | 'squash';
    const ok = await this.withStashRetry(tr('マージ', 'Merge', '合并'), { type: 'merge', rev, mode });
    if (ok && mode === 'squash') {
      this.message = this.repoState?.squashMsg?.split('\n')[0] === 'Squashed commit of the following:' ? `Merge ${label} (squash)` : this.message || `Merge ${label} (squash)`;
      this.saveDraft();
      store.toast(tr('変更をステージしました。メッセージを書いてコミットしてください', 'Changes are staged. Write a message and commit', '更改已暂存。请填写信息并提交'));
    }
    if (this.repoState?.kind === 'merge' && this.repoState.mergeMsg && !this.message) this.message = this.repoState.mergeMsg.split('\n#')[0].trim();
  }

  // ================= チェリーピック・revert =================

  async cherryPick(hashes: string[]) {
    const repo = this.repoPath;
    if (!repo || !hashes.length) return;
    // 古い順に適用する
    const known = new Map(this.log.map((c, i) => [c.hash, { c, i }]));
    const commits: Commit[] = [];
    for (const h of hashes) {
      const k = known.get(h)?.c ?? (await gitApi.commitDetail(repo, h).then((d) => d.commit).catch(() => null));
      if (k) commits.push(k);
    }
    commits.sort((a, b) => (known.get(b.hash)?.i ?? 0) - (known.get(a.hash)?.i ?? 0) || a.date - b.date);
    const applied = new Set(await gitApi.cherry(repo, commits.map((c) => c.hash)).catch(() => [] as string[]));
    const merges = commits.filter((c) => c.parents.length > 1);
    const fields: FormField[] = commits.map((c) => ({
      kind: 'check' as const,
      id: 'c:' + c.hash,
      label: `${shortHash(c.hash)}  ${c.subject}${applied.has(c.hash) ? tr('  （適用済み）', '  (already applied)', '  （已应用）') : ''}`,
      value: !applied.has(c.hash),
    }));
    fields.push(
      {
        kind: 'radio',
        id: 'commit',
        value: 'commit',
        options: [
          { value: 'commit', label: tr('コミットする', 'Commit', '提交') },
          { value: 'no-commit', label: tr('変更だけ取り込む（--no-commit。メッセージ欄からコミット）', 'Apply changes only (--no-commit; commit from the message box)', '仅应用更改（--no-commit，从提交信息框提交）') },
        ],
      },
      { kind: 'check', id: 'x', label: tr('元のコミットを記録する（-x）', 'Record the original commit (-x)', '记录原始提交（-x）'), value: false },
    );
    if (merges.length) {
      fields.push({
        kind: 'radio',
        id: 'mainline',
        label: tr('マージコミットの基準にする親', 'Parent to use for merge commits', '合并提交的基准父提交'),
        value: '1',
        options: [
          { value: '1', label: tr('1 つめの親（取り込んだ側のブランチ）', 'First parent (the branch merged into)', '第一个父提交（被合并的分支）') },
          { value: '2', label: tr('2 つめの親（取り込まれた側）', 'Second parent (the merged branch)', '第二个父提交（合并进来的分支）') },
        ],
      });
    }
    const head = this.branch?.head ?? tr('（デタッチ）', '(detached)', '（分离）');
    const r = await this.ask({
      title: tr('チェリーピック', 'Cherry-pick', '拣选'),
      message: tr(`次のコミットを ${head} に複製します（古い順）`, `Copy these commits onto ${head} (oldest first)`, `将以下提交复制到 ${head}（按从旧到新）`),
      fields,
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('チェリーピック', 'Cherry-pick', '拣选'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return;
    const revs = commits.filter((c) => r.values['c:' + c.hash]).map((c) => c.hash);
    if (!revs.length) return;
    const noCommit = r.values.commit === 'no-commit';
    this.sequenceTotal = revs.length;
    const mainline = revs.some((h) => merges.some((m) => m.hash === h)) ? Number(r.values.mainline ?? 1) : null;
    const ok = await this.withStashRetry(tr('チェリーピック', 'Cherry-pick', '拣选'), { type: 'cherryPick', revs, noCommit, recordOrigin: !!r.values.x, mainline });
    if (ok) {
      this.sequenceTotal = 0;
      if (noCommit && !this.message) {
        this.message = commits.find((c) => c.hash === revs[0])?.subject ?? '';
        this.saveDraft();
      }
      store.toast(tr(`${revs.length} 件のコミットを適用しました`, `Applied ${revs.length} commit(s)`, `已应用 ${revs.length} 个提交`));
    }
  }

  async revert(c: Commit) {
    let mainline: number | null = null;
    if (c.parents.length > 1) {
      const r = await this.ask({
        title: tr('コミットを打ち消す', 'Revert commit', '还原提交'),
        message: tr('マージコミットです。どちらの親を基準に打ち消しますか？', 'This is a merge commit. Which parent should be the mainline?', '这是合并提交。要以哪个父提交为基准还原？'),
        fields: [
          {
            kind: 'radio',
            id: 'm',
            value: '1',
            options: [
              { value: '1', label: tr('1 つめの親', 'First parent', '第一个父提交') },
              { value: '2', label: tr('2 つめの親', 'Second parent', '第二个父提交') },
            ],
          },
        ],
        buttons: [
          { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
          { label: tr('打ち消す', 'Revert', '还原'), value: 'ok', primary: true },
        ],
      });
      if (r?.button !== 'ok') return;
      mainline = Number(r.values.m);
    } else {
      const ok = await store.confirm(
        tr('コミットを打ち消す', 'Revert commit', '还原提交'),
        tr(`${shortHash(c.hash)}「${c.subject}」を打ち消すコミットを作ります。`, `Create a commit that reverts ${shortHash(c.hash)} "${c.subject}"?`, `将创建还原 ${shortHash(c.hash)}“${c.subject}”的提交。`),
        tr('打ち消す', 'Revert', '还原'),
      );
      if (!ok) return;
    }
    await this.withStashRetry(tr('打ち消し', 'Revert', '还原'), { type: 'revert', rev: c.hash, mainline });
  }

  /** マージ・チェリーピック・revert の続き / スキップ / 中止 */
  async sequencer(action: 'continue' | 'skip' | 'abort') {
    const kind = this.repoState?.kind;
    if (!kind || kind === 'rebase') return;
    if (action === 'abort') {
      const ok = await store.confirm(
        tr('中止', 'Abort', '中止'),
        tr('途中の変更を捨てて、始める前の状態に戻します。よろしいですか？', 'Discard the in-progress changes and return to the state before you started?', '将丢弃进行中的更改并恢复到开始前的状态。确定吗？'),
        tr('中止する', 'Abort', '中止'),
        undefined,
        true,
      );
      if (!ok) return;
    }
    if (action === 'continue' && this.groups.conflict.length) {
      store.toast(tr('解決していない競合があります', 'There are unresolved conflicts', '仍有未解决的冲突'), 'error');
      return;
    }
    if (kind === 'merge' && action === 'continue') {
      if (!this.message && this.repoState?.mergeMsg) this.message = this.repoState.mergeMsg.split('\n#')[0].trim();
      await this.commit();
      return;
    }
    const r = await this.run(tr('続き', 'Continue', '继续'), { type: 'sequencer', kind, action });
    if (r.ok && action !== 'continue') this.sequenceTotal = action === 'abort' ? 0 : this.sequenceTotal;
    if (r.ok && !this.repoState?.kind) this.sequenceTotal = 0;
  }

  /** 競合のファイルを解決済みにする */
  async markResolved(files: FileStatus[]) {
    await this.stage(files);
  }

  // ================= リモート =================

  async fetch(remote: string | null = null, branch: string | null = null) {
    const before = this.branch?.behind ?? 0;
    const r = await this.run(tr('フェッチ', 'Fetch', '获取'), { type: 'fetch', remote, branch }, { remote: true });
    if (!r.ok) return;
    const behind = this.branch?.behind ?? 0;
    const name = this.branch?.upstream;
    if (name && behind > 0) {
      store.toast(tr(`${name} に取り込んでいないコミットが ${behind} 件あります`, `${name} has ${behind} commit(s) you haven't pulled`, `${name} 有 ${behind} 个尚未拉取的提交`));
    } else if (behind === before) {
      store.toast(tr('フェッチしました', 'Fetched', '已获取'));
    }
  }

  async pull() {
    const repo = this.repoPath;
    if (!repo) return;
    if (!this.branch?.upstream) {
      store.toast(tr('このブランチには上流のブランチがありません。先にプッシュしてください', 'This branch has no upstream. Push it first', '此分支没有上游分支。请先推送'), 'error');
      return;
    }
    // 方法はユーザーの設定（pull.rebase / pull.ff）に従う。未設定なら早送りのみ
    const [rebase, ff] = await Promise.all([gitApi.configGet(repo, 'pull.rebase'), gitApi.configGet(repo, 'pull.ff')]).catch(() => [null, null]);
    const mode = rebase !== null || ff !== null ? null : ('ff-only' as const);
    const label = tr('プル', 'Pull', '拉取');
    const r = await this.run(label, { type: 'pull', mode }, { remote: true, quiet: true });
    if (r.ok) return;
    if (/Not possible to fast-forward|diverg|fatal: Need to specify how to reconcile/i.test(r.output)) {
      const a = await this.ask({
        title: label,
        message: tr(
          'ローカルとリモートの両方に新しいコミットがあるため、早送りできません。取り込み方を選んでください。',
          'Both local and remote have new commits, so a fast-forward is not possible. Choose how to integrate them.',
          '本地和远程都有新的提交，无法快进。请选择合并方式。',
        ),
        buttons: [
          { label: tr('やめる', 'Cancel', '取消'), value: 'cancel' },
          { label: tr('リベース', 'Rebase', '变基'), value: 'rebase' },
          { label: tr('マージ', 'Merge', '合并'), value: 'merge', primary: true },
        ],
      });
      if (a?.button === 'merge' || a?.button === 'rebase') await this.run(label, { type: 'pull', mode: a.button }, { remote: true });
      return;
    }
    if (/would be overwritten|local changes/i.test(r.output)) {
      await this.withStashRetry(label, { type: 'pull', mode });
      return;
    }
    await this.showError(label, r.output, { type: 'pull', mode }, true);
  }

  async push(force = false) {
    const b = this.branch;
    if (!b?.head) {
      store.toast(tr('ブランチにいないため、プッシュできません', "You're not on a branch, so you can't push", '当前不在分支上，无法推送'), 'error');
      return;
    }
    const label = force ? tr('強制プッシュ', 'Force push', '强制推送') : tr('プッシュ', 'Push', '推送');
    let op: Op = { type: 'push', remote: null, branch: null, setUpstream: false, force };
    if (!b.upstream) {
      const remotes = this.refs?.remotes ?? [];
      if (!remotes.length) {
        store.toast(tr('リモートがありません。「リモート」から追加してください', 'There are no remotes. Add one from Remotes', '没有远程仓库。请在“远程”中添加'), 'error');
        return;
      }
      let remote = remotes[0].name;
      {
        const r = await this.ask({
          title: label,
          message: tr(
            `ブランチ ${b.head} には上流のブランチがありません。プッシュ先のリモートを選んでください（上流として設定します）。`,
            `Branch ${b.head} has no upstream. Choose a remote to push to (it will be set as upstream).`,
            `分支 ${b.head} 没有上游分支。请选择要推送到的远程（将设为上游）。`,
          ),
          fields: [{ kind: 'radio', id: 'remote', value: remote, options: remotes.map((x) => ({ value: x.name, label: `${x.name}  ${x.url}` })) }],
          buttons: [
            { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
            { label, value: 'ok', primary: true },
          ],
        });
        if (r?.button !== 'ok') return;
        remote = String(r.values.remote);
      }
      op = { type: 'push', remote, branch: b.head, setUpstream: true, force };
    } else if (force) {
      const ok = await store.confirm(
        label,
        tr(
          `${b.upstream} をローカルの ${b.head} で上書きします（--force-with-lease。ほかの人が新しくプッシュしていたら止まります）。リモートのコミットが失われることがあります。実行しますか？`,
          `Overwrite ${b.upstream} with your local ${b.head}? (--force-with-lease stops if someone else pushed.) Commits on the remote may be lost.`,
          `将用本地 ${b.head} 覆盖 ${b.upstream}（--force-with-lease，若他人有新推送则会停止）。远程的提交可能会丢失。要执行吗？`,
        ),
        label,
        undefined,
        true,
      );
      if (!ok) return;
    }
    const r = await this.run(label, op, { remote: true, quiet: true });
    if (r.ok) return;
    if (/rejected|fetch first|non-fast-forward/i.test(r.output)) {
      const a = await this.ask({
        title: tr('プッシュできませんでした', 'Push was rejected', '推送被拒绝'),
        message: tr(
          'リモートに、ローカルにないコミットがあります。プルして取り込んでから、もう一度プッシュしてください。',
          'The remote has commits you do not have locally. Pull them, then push again.',
          '远程有本地没有的提交。请先拉取，再重新推送。',
        ),
        output: r.output,
        buttons: [
          { label: tr('閉じる', 'Close', '关闭'), value: 'close' },
          { label: tr('プル', 'Pull', '拉取'), value: 'pull', primary: true },
        ],
      });
      if (a?.button === 'pull') await this.pull();
      return;
    }
    await this.showError(label, r.output, op, true);
  }

  async addRemote() {
    const r = await this.ask({
      title: tr('リモートの追加', 'Add remote', '添加远程'),
      fields: [
        { kind: 'text', id: 'name', label: tr('名前', 'Name', '名称'), value: this.refs?.remotes.length ? '' : 'origin' },
        { kind: 'text', id: 'url', label: 'URL', value: '', placeholder: 'https://github.com/...' },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('追加', 'Add', '添加'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return;
    const name = String(r.values.name).trim();
    const url = String(r.values.url).trim();
    if (name && url) await this.run(tr('リモートの追加', 'Add remote', '添加远程'), { type: 'addRemote', name, url });
  }

  async editRemoteUrl(name: string, url: string) {
    const next = await this.ask({
      title: tr('リモートの URL', 'Remote URL', '远程 URL'),
      fields: [{ kind: 'text', id: 'url', label: name, value: url }],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('変更', 'Change', '更改'), value: 'ok', primary: true },
      ],
    });
    const u = String(next?.values.url ?? '').trim();
    if (next?.button === 'ok' && u && u !== url) await this.run(tr('リモートの URL', 'Remote URL', '远程 URL'), { type: 'setRemoteUrl', name, url: u });
  }

  async removeRemote(name: string) {
    const ok = await store.confirm(
      tr('リモートの削除', 'Remove remote', '删除远程'),
      tr(`リモート ${name} の設定を削除しますか？（リモートのリポジトリ自体は消えません）`, `Remove remote ${name}? (The remote repository itself is not deleted.)`, `要删除远程 ${name} 的设置吗？（远程仓库本身不会被删除）`),
      tr('削除', 'Remove', '删除'),
      undefined,
      true,
    );
    if (ok) await this.run(tr('リモートの削除', 'Remove remote', '删除远程'), { type: 'removeRemote', name });
  }

  // ================= タグ =================

  async createTag(rev: string) {
    const r = await this.ask({
      title: tr('タグを付ける', 'Create tag', '创建标签'),
      message: rev.length === 40 ? shortHash(rev) : rev,
      fields: [
        { kind: 'text', id: 'name', label: tr('タグ名', 'Tag name', '标签名'), value: '', placeholder: 'v1.0.0' },
        { kind: 'text', id: 'message', label: tr('メッセージ（入れると注釈付きタグ）', 'Message (makes an annotated tag)', '信息（填写后为附注标签）'), value: '' },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('作成', 'Create', '创建'), value: 'ok', primary: true },
      ],
    });
    const name = String(r?.values.name ?? '').trim();
    if (r?.button !== 'ok' || !name) return;
    if (invalidBranchName(name)) {
      store.toast(tr(`タグ名に使えない名前です: ${name}`, `Invalid tag name: ${name}`, `无效的标签名：${name}`), 'error');
      return;
    }
    const message = String(r.values.message ?? '').trim() || null;
    await this.run(tr('タグの作成', 'Create tag', '创建标签'), { type: 'createTag', name, rev, message });
  }

  async deleteTag(name: string) {
    const ok = await store.confirm(tr('タグの削除', 'Delete tag', '删除标签'), tr(`タグ ${name} を削除しますか？`, `Delete tag ${name}?`, `要删除标签 ${name} 吗？`), tr('削除', 'Delete', '删除'), undefined, true);
    if (ok) await this.run(tr('タグの削除', 'Delete tag', '删除标签'), { type: 'deleteTag', name });
  }

  async pushTag(name: string) {
    const remotes = this.refs?.remotes ?? [];
    if (!remotes.length) return;
    let remote = remotes[0].name;
    if (remotes.length > 1) {
      const r = await this.ask({
        title: tr('タグのプッシュ', 'Push tag', '推送标签'),
        fields: [{ kind: 'radio', id: 'remote', value: remote, options: remotes.map((x) => ({ value: x.name, label: x.name })) }],
        buttons: [
          { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
          { label: tr('プッシュ', 'Push', '推送'), value: 'ok', primary: true },
        ],
      });
      if (r?.button !== 'ok') return;
      remote = String(r.values.remote);
    }
    await this.run(tr('タグのプッシュ', 'Push tag', '推送标签'), { type: 'pushTag', remote, name }, { remote: true });
  }

  // ================= スタッシュ =================

  async stashPush() {
    const r = await this.ask({
      title: tr('スタッシュ', 'Stash', '贮藏'),
      message: tr('作業ツリーとステージの変更を一時的にしまいます。', 'Temporarily put away changes in the working tree and the index.', '暂时收起工作区和暂存区的更改。'),
      fields: [
        { kind: 'text', id: 'message', label: tr('メッセージ（省略可）', 'Message (optional)', '信息（可选）'), value: '' },
        { kind: 'check', id: 'untracked', label: tr('未追跡のファイルも含める', 'Include untracked files', '包含未跟踪的文件'), value: true },
      ],
      buttons: [
        { label: tr('キャンセル', 'Cancel', '取消'), value: 'cancel' },
        { label: tr('スタッシュ', 'Stash', '贮藏'), value: 'ok', primary: true },
      ],
    });
    if (r?.button !== 'ok') return;
    await this.run(tr('スタッシュ', 'Stash', '贮藏'), { type: 'stashPush', message: String(r.values.message) || null, untracked: !!r.values.untracked });
  }

  async stashApply(index: number, pop: boolean) {
    const label = pop ? tr('スタッシュの取り出し', 'Pop stash', '弹出贮藏') : tr('スタッシュの適用', 'Apply stash', '应用贮藏');
    await this.run(label, pop ? { type: 'stashPop', index } : { type: 'stashApply', index });
  }

  async stashDrop(index: number) {
    const ok = await store.confirm(
      tr('スタッシュの削除', 'Drop stash', '删除贮藏'),
      tr(`stash@{${index}} を削除しますか？元に戻せません。`, `Drop stash@{${index}}? This can't be undone.`, `要删除 stash@{${index}} 吗？此操作无法撤销。`),
      tr('削除', 'Drop', '删除'),
      undefined,
      true,
    );
    if (ok) await this.run(tr('スタッシュの削除', 'Drop stash', '删除贮藏'), { type: 'stashDrop', index });
  }

  async initRepo() {
    const root = this.info?.root;
    if (!root) return;
    const ok = await store.confirm(
      tr('リポジトリを作成', 'Create repository', '创建仓库'),
      tr(`${root} に Git のリポジトリを作ります（.git フォルダができます）。`, `Create a Git repository in ${root}? (A .git folder will be created.)`, `将在 ${root} 创建 Git 仓库（会生成 .git 文件夹）。`),
      tr('作成', 'Create', '创建'),
    );
    if (!ok) return;
    try {
      await gitApi.op(root, { type: 'init' });
    } catch (e) {
      store.toast(errorMessage(e), 'error');
      return;
    }
    await this.detect();
  }

  // ================= 右ペインのタブ =================

  private openTab(key: string, kind: GitTabKind, title: string, setup: (t: GitTab) => void, pin: boolean) {
    const existing = store.views.find((v) => v.key === key);
    if (existing) {
      if (pin) existing.pinned = true;
      existing.version++;
      store.activate(key);
      return;
    }
    const t = new GitTab(key, kind, title);
    setup(t);
    t.pinned = pin;
    // 仮タブは次に開くもので置き換える（ファイルの仮タブと同じ）
    const preview = store.views.findIndex((v) => !v.pinned);
    if (preview >= 0 && !pin) store.views.splice(preview, 1, t);
    else store.views.push(t);
    store.activate(key);
  }

  openDiff(spec: DiffSpec, pin = false) {
    const key = `git:diff:${spec.repo}:${spec.left.rev}:${spec.right.rev}:${spec.path}`;
    const which = spec.stage === 'worktree' ? tr('作業ツリー', 'Working Tree', '工作区') : spec.stage === 'index' ? tr('ステージ', 'Index', '暂存区') : spec.right.label;
    this.openTab(key, 'diff', `${basename(spec.path)} (${which})`, (t) => (t.diff = spec), pin);
  }

  /** 変更の一覧のファイルの差分を開く */
  openChange(f: FileStatus, section: Section, pin = false) {
    const repo = this.repoPath;
    if (!repo) return;
    if (section === 'staged') {
      this.openDiff({ repo, path: f.path, orig: f.orig, left: { rev: 'HEAD', label: 'HEAD' }, right: { rev: '', label: tr('ステージ', 'Index', '暂存区') }, stage: 'index' }, pin);
    } else if (section === 'conflict') {
      this.openDiff(
        { repo, path: f.path, orig: null, left: { rev: ':2', label: tr('現在の変更', 'Current', '当前更改') }, right: { rev: ':3', label: tr('取り込む変更', 'Incoming', '传入的更改') }, stage: 'none' },
        pin,
      );
    } else {
      this.openDiff(
        {
          repo,
          path: f.path,
          orig: null,
          left: { rev: f.untracked ? 'HEAD' : '', label: tr('ステージ', 'Index', '暂存区') },
          right: { rev: null, label: tr('作業ツリー', 'Working Tree', '工作区') },
          stage: 'worktree',
        },
        pin,
      );
    }
  }

  /** ファイル（ルートからの相対パス）の作業ツリーの変更を表示する */
  openWorkingDiff(rel: string) {
    const repo = this.repoForRoot(rel);
    if (!repo) return;
    const path = rootToRepo(repo, rel)!;
    const f = this.status?.status.files.find((x) => x.path === path);
    if (repo.path !== this.repoPath || !f) {
      this.openDiff({
        repo: repo.path,
        path,
        orig: null,
        left: { rev: '', label: tr('ステージ', 'Index', '暂存区') },
        right: { rev: null, label: tr('作業ツリー', 'Working Tree', '工作区') },
        stage: 'worktree',
      });
      return;
    }
    if (f.conflict) this.openChange(f, 'conflict');
    else if (f.y === '.' && f.x !== '.') this.openChange(f, 'staged');
    else this.openChange(f, f.untracked ? 'untracked' : 'unstaged');
  }

  /** コミットでのファイルの差分（1 つめの親との差分） */
  openCommitFile(c: Commit, f: ChangedFile, pin = false) {
    const repo = this.repoPath;
    if (!repo) return;
    const parent = c.parents[0] ?? null;
    this.openDiff(
      {
        repo,
        path: f.path,
        orig: f.orig,
        left: { rev: parent ?? '__none__', label: parent ? shortHash(parent) : tr('（なし）', '(none)', '（无）') },
        right: { rev: c.hash, label: shortHash(c.hash) },
        stage: 'none',
      },
      pin,
    );
  }

  openCompareFile(base: string, target: string, f: ChangedFile) {
    const repo = this.repoPath;
    if (!repo) return;
    this.openDiff({ repo, path: f.path, orig: f.orig, left: { rev: base, label: label(base) }, right: { rev: target, label: label(target) }, stage: 'none' });
  }

  openStashFile(index: number, f: ChangedFile) {
    const repo = this.repoPath;
    if (!repo) return;
    const s = `stash@{${index}}`;
    this.openDiff({ repo, path: f.path, orig: f.orig, left: { rev: `${s}^1`, label: `${s}^1` }, right: { rev: s, label: s }, stage: 'none' });
  }

  /** この時点の内容（読み取り専用） */
  openRevision(rev: string, path: string, repo = this.repoPath) {
    if (!repo) return;
    const name = rev.length === 40 ? shortHash(rev) : rev;
    this.openTab(`git:rev:${repo}:${rev}:${path}`, 'rev', `${basename(path)} @ ${name}`, (t) => (t.rev = { repo, rev, path }), false);
  }

  /** この時点のファイルの一覧 */
  openTree(rev: string, title = rev.length === 40 ? shortHash(rev) : rev) {
    const repo = this.repoPath;
    if (!repo) return;
    this.openTab(`git:tree:${repo}:${rev}`, 'tree', `${title} ${tr('のファイル', 'files', '的文件')}`, (t) => (t.tree = { repo, rev, label: title }), true);
  }

  /** 比較（base にないコミット = 取り込むもの、target にないコミット = 送るもの） */
  openCompare(base: string, target: string) {
    const repo = this.repoPath;
    if (!repo) return;
    this.openTab(`git:compare:${repo}:${base}:${target}`, 'compare', `${label(base)} ↔ ${label(target)}`, (t) => (t.compare = { repo, base, target }), true);
  }

  /** ステージや破棄のあと、開いている差分タブを読み直す */
  bumpTabs() {
    for (const v of store.views) if (v.kind === 'diff' || v.kind === 'compare') v.version++;
  }

  // ================= エディタの変更の帯 =================

  /** 開いているファイルのステージの内容（変更の帯の基準）を読む */
  async updateBase(doc: Doc | null) {
    if (!doc) return;
    const repo = this.repoForRoot(doc.path);
    if (!repo || store.config.gitGutter === false || !this.enabled) {
      doc.gitBase = null;
      return;
    }
    const path = rootToRepo(repo, doc.path)!;
    try {
      const side = await gitApi.showFile(repo.path, '', path);
      doc.gitBase = side.exists && side.text !== null ? side.text : side.exists ? null : '';
    } catch {
      doc.gitBase = null;
    }
  }

  // ================= 自動フェッチ =================

  startAutoFetch() {
    this.stopAutoFetch();
    if (!store.config.gitAutoFetch || !this.repoPath) return;
    const minutes = Math.max(1, store.config.gitAutoFetchMinutes || 5);
    this.fetchTimer = setInterval(() => {
      if (document.hasFocus() && !this.busy && this.refs?.remotes.length) {
        void this.run(tr('フェッチ', 'Fetch', '获取'), { type: 'fetch', remote: null, branch: null }, { remote: true, quiet: true });
      }
    }, minutes * 60_000);
  }

  stopAutoFetch() {
    if (this.fetchTimer) clearInterval(this.fetchTimer);
    this.fetchTimer = null;
  }

  // ================= 表示用 =================

  relTime(sec: number): string {
    return relativeTime(sec);
  }

  /** ブランチの選択肢（パレット用） */
  branchChoices(): { name: string; detail: string; remote: { remote: string; name: string } | null }[] {
    const out: { name: string; detail: string; remote: { remote: string; name: string } | null }[] = [];
    for (const b of this.refs?.branches ?? []) out.push({ name: b.name, detail: b.current ? tr('現在のブランチ', 'current', '当前分支') : b.subject, remote: null });
    for (const r of this.refs?.remoteBranches ?? []) out.push({ name: `${r.remote}/${r.name}`, detail: r.subject, remote: { remote: r.remote, name: r.name } });
    return out;
  }
}

function label(rev: string): string {
  return /^[0-9a-f]{40}$/.test(rev) ? shortHash(rev) : rev;
}

function relativeTime(sec: number): string {
  const diff = Date.now() / 1000 - sec;
  const fmt = new Intl.RelativeTimeFormat(i18n.locale, { numeric: 'auto' });
  if (diff < 60) return fmt.format(-Math.max(0, Math.round(diff)), 'second');
  if (diff < 3600) return fmt.format(-Math.round(diff / 60), 'minute');
  if (diff < 86400) return fmt.format(-Math.round(diff / 3600), 'hour');
  if (diff < 86400 * 30) return fmt.format(-Math.round(diff / 86400), 'day');
  if (diff < 86400 * 365) return fmt.format(-Math.round(diff / 86400 / 30), 'month');
  return fmt.format(-Math.round(diff / 86400 / 365), 'year');
}

/** 「ターミナルで実行」で入れるコマンド */
function commandLine(op: Op): string | null {
  const q = (s: string) => (/^[\w./@:+-]+$/.test(s) ? s : `"${s.replace(/"/g, '\\"')}"`);
  switch (op.type) {
    case 'fetch':
      return op.remote ? `git fetch --prune ${q(op.remote)}${op.branch ? ' ' + q(op.branch) : ''}` : 'git fetch --prune --tags --all';
    case 'pull':
      return op.mode === 'merge' ? 'git pull --no-rebase' : op.mode === 'rebase' ? 'git pull --rebase' : op.mode === 'ff-only' ? 'git pull --ff-only' : 'git pull';
    case 'push':
      return `git push${op.force ? ' --force-with-lease' : ''}${op.setUpstream ? ' -u' : ''}${op.remote ? ' ' + q(op.remote) : ''}${op.branch ? ' ' + q(op.branch) : ''}`;
    case 'pushTag':
      return `git push ${q(op.remote)} ${q('refs/tags/' + op.name)}`;
    case 'deleteRemoteBranch':
      return `git push ${q(op.remote)} --delete ${q(op.name)}`;
    default:
      return null;
  }
}

export const git = new GitStore();
