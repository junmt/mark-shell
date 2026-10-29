// Git 連携の型・Rust 側のコマンドの呼び出し・画面に依存しない計算（docs/git.md）

import { invoke } from '@tauri-apps/api/core';
import { Text } from '@codemirror/state';
import { Chunk } from '@codemirror/merge';

export type RepoInfo = { path: string; name: string; prefix: string | null; rootInRepo: string | null };
export type GitInfo = { found: boolean; version: string | null; error: string | null; repos: RepoInfo[]; root: string };

export type FileStatus = {
  path: string;
  orig: string | null;
  x: string;
  y: string;
  conflict: boolean;
  untracked: boolean;
  submodule: boolean;
};
export type BranchStatus = { oid: string | null; head: string | null; upstream: string | null; ahead: number; behind: number };
export type RepoState = { kind: 'merge' | 'cherry-pick' | 'revert' | 'rebase' | null; remaining: number | null; mergeMsg: string | null; squashMsg: string | null };
export type StatusResult = {
  status: { branch: BranchStatus; files: FileStatus[]; truncated: boolean };
  state: RepoState;
  lastFetch: number | null;
};

export type Commit = { hash: string; parents: string[]; author: string; email: string; date: number; refs: string[]; subject: string };
export type ChangedFile = { status: string; path: string; orig: string | null };
export type CommitDetail = { commit: Commit; message: string; files: ChangedFile[] };
export type Compare = { incoming: Commit[]; outgoing: Commit[]; files: ChangedFile[] };

export type Branch = {
  name: string;
  hash: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  gone: boolean;
  current: boolean;
  subject: string;
  date: number;
};
export type RemoteBranch = { remote: string; name: string; hash: string; subject: string; date: number };
export type Tag = { name: string; hash: string };
export type Remote = { name: string; url: string };
export type Refs = { branches: Branch[]; remoteBranches: RemoteBranch[]; tags: Tag[]; remotes: Remote[] };
export type Stash = { index: number; message: string; date: number };

export type FileSide = {
  exists: boolean;
  binary: boolean;
  size: number;
  text: string | null;
  encoding: string;
  eol: string;
  image: string | null;
};

export type LogQuery = { refs?: string[]; all?: boolean; path?: string | null; skip?: number; limit: number };

/** 変更を伴う操作（Rust 側の git::Op） */
export type Op =
  | { type: 'init' }
  | { type: 'stage'; paths: string[] }
  | { type: 'stageAll' }
  | { type: 'unstage'; paths: string[] }
  | { type: 'unstageAll' }
  | { type: 'discard'; paths: string[] }
  | { type: 'discardUntracked'; paths: string[] }
  | { type: 'setIndex'; path: string; text: string; encoding: string; eol: string }
  | { type: 'commit'; message: string; amend: boolean; signoff: boolean }
  | { type: 'checkout'; name: string }
  | { type: 'checkoutDetached'; rev: string }
  | { type: 'checkoutTrack'; remoteRef: string; local: string }
  | { type: 'createBranch'; name: string; start: string | null; checkout: boolean }
  | { type: 'renameBranch'; from: string; to: string }
  | { type: 'deleteBranch'; name: string; force: boolean }
  | { type: 'deleteRemoteBranch'; remote: string; name: string }
  | { type: 'merge'; rev: string; mode: 'ff' | 'no-ff' | 'squash' }
  | { type: 'cherryPick'; revs: string[]; noCommit: boolean; recordOrigin: boolean; mainline: number | null }
  | { type: 'revert'; rev: string; mainline: number | null }
  | { type: 'sequencer'; kind: 'merge' | 'cherry-pick' | 'revert'; action: 'continue' | 'skip' | 'abort' }
  | { type: 'fetch'; remote: string | null; branch: string | null }
  | { type: 'pull'; mode: 'ff-only' | 'merge' | 'rebase' | null }
  | { type: 'push'; remote: string | null; branch: string | null; setUpstream: boolean; force: boolean }
  | { type: 'createTag'; name: string; rev: string; message: string | null }
  | { type: 'deleteTag'; name: string }
  | { type: 'pushTag'; remote: string; name: string }
  | { type: 'stashPush'; message: string | null; untracked: boolean }
  | { type: 'stashApply'; index: number }
  | { type: 'stashPop'; index: number }
  | { type: 'stashDrop'; index: number }
  | { type: 'addRemote'; name: string; url: string }
  | { type: 'setRemoteUrl'; name: string; url: string }
  | { type: 'removeRemote'; name: string }
  | { type: 'setUser'; name: string; email: string; global: boolean }
  | { type: 'appendGitignore'; line: string };

export const gitApi = {
  repos: (nested: string[]) => invoke<GitInfo>('git_repos', { nested }),
  status: (repo: string) => invoke<StatusResult>('git_status', { repo }),
  log: (repo: string, query: LogQuery) => invoke<Commit[]>('git_log', { repo, query: { refs: [], all: false, path: null, skip: 0, ...query } }),
  commitDetail: (repo: string, rev: string) => invoke<CommitDetail>('git_commit_detail', { repo, rev }),
  compare: (repo: string, base: string, target: string) => invoke<Compare>('git_compare', { repo, base, target }),
  lsTree: (repo: string, rev: string) => invoke<string[]>('git_ls_tree', { repo, rev }),
  cherry: (repo: string, revs: string[]) => invoke<string[]>('git_cherry', { repo, revs }),
  /** rev: null は作業ツリー、'' はステージ、':2' / ':3' は競合中のこちら / 取り込む側、それ以外はコミット */
  showFile: (repo: string, rev: string | null, path: string) => invoke<FileSide>('git_show_file', { repo, rev, path }),
  refs: (repo: string) => invoke<Refs>('git_refs', { repo }),
  stashes: (repo: string) => invoke<Stash[]>('git_stashes', { repo }),
  stashFiles: (repo: string, index: number) => invoke<ChangedFile[]>('git_stash_files', { repo, index }),
  configGet: (repo: string, key: string) => invoke<string | null>('git_config_get', { repo, key }),
  op: (repo: string, op: Op, opId?: number) => invoke<string>('git_op', { repo, op, opId: opId ?? null }),
  cancel: (opId: number) => invoke<void>('git_cancel', { opId }),
  watch: (repos: string[]) => invoke<void>('git_watch', { repos }),
  loadState: (repo: string) => invoke<unknown>('git_load_state', { repo }),
  saveState: (repo: string, value: unknown) => invoke<void>('git_save_state', { repo, value }),
};

// ================= パス =================

/** リポジトリの中のパス → ルートからの相対パス。ルートの外なら null */
export function repoToRoot(repo: RepoInfo, path: string): string | null {
  if (repo.prefix !== null) return repo.prefix ? `${repo.prefix}/${path}` : path;
  if (repo.rootInRepo !== null) {
    if (!repo.rootInRepo) return path;
    const p = repo.rootInRepo + '/';
    return path.toLowerCase().startsWith(p.toLowerCase()) ? path.slice(p.length) : null;
  }
  return null;
}

/** ルートからの相対パス → リポジトリの中のパス。そのリポジトリの外なら null */
export function rootToRepo(repo: RepoInfo, rel: string): string | null {
  if (repo.prefix !== null) {
    if (!repo.prefix) return rel;
    const p = repo.prefix + '/';
    return rel.toLowerCase().startsWith(p.toLowerCase()) ? rel.slice(p.length) : null;
  }
  if (repo.rootInRepo !== null) return repo.rootInRepo ? `${repo.rootInRepo}/${rel}` : rel;
  return null;
}

// ================= 変更の一覧 =================

export type Section = 'staged' | 'unstaged' | 'untracked' | 'conflict';

/** status の結果をセクションに分ける。1 つのファイルがステージ済みと変更の両方に出ることもある */
export function groupStatus(files: FileStatus[]): Record<Section, FileStatus[]> {
  const g: Record<Section, FileStatus[]> = { staged: [], unstaged: [], untracked: [], conflict: [] };
  for (const f of files) {
    if (f.conflict) g.conflict.push(f);
    else if (f.untracked) g.untracked.push(f);
    else {
      if (f.x !== '.') g.staged.push(f);
      if (f.y !== '.') g.unstaged.push(f);
    }
  }
  for (const k of Object.keys(g) as Section[]) g[k].sort((a, b) => a.path.localeCompare(b.path, 'ja'));
  return g;
}

/** ファイルツリーに付ける記号: M 変更、A 追加、D 削除、U 未追跡、! 競合 */
export function treeMark(f: FileStatus): 'M' | 'A' | 'D' | 'U' | '!' | 'R' {
  if (f.conflict) return '!';
  if (f.untracked) return 'U';
  const c = f.y !== '.' ? f.y : f.x;
  if (c === 'A') return 'A';
  if (c === 'D') return 'D';
  if (c === 'R' || c === 'C') return 'R';
  return 'M';
}

/** 一覧に出す状態の文字 */
export function sectionCode(f: FileStatus, section: Section): string {
  if (section === 'conflict') return '!';
  if (section === 'untracked') return 'U';
  const c = section === 'staged' ? f.x : f.y;
  return c === '.' ? 'M' : c === 'T' ? 'M' : c;
}

// ================= 履歴のグラフ =================

export type GraphRow = {
  /** このコミットの列 */
  col: number;
  /** 上半分の線: [上の列, 下（中央）の列] */
  top: [number, number][];
  /** 下半分の線: [中央の列, 下の列] */
  bottom: [number, number][];
  /** この行で使う列の数 */
  width: number;
};

/** コミットの並び（新しい順）から、各行の線を計算する。列は詰めずに使い回す */
export function layoutGraph(commits: Commit[]): GraphRow[] {
  let lanes: (string | null)[] = [];
  const rows: GraphRow[] = [];
  for (const c of commits) {
    let col = lanes.indexOf(c.hash);
    if (col < 0) {
      col = lanes.indexOf(null);
      if (col < 0) col = lanes.length;
      lanes[col] = null;
    }
    const top: [number, number][] = [];
    lanes.forEach((h, i) => {
      if (h === null) return;
      top.push([i, h === c.hash ? col : i]);
    });
    const next = lanes.map((h) => (h === c.hash ? null : h));
    const bottom: [number, number][] = [];
    next.forEach((h, i) => {
      if (h !== null) bottom.push([i, i]);
    });
    c.parents.forEach((p, k) => {
      let j = next.indexOf(p);
      if (j < 0) {
        if (k === 0 && next[col] === null) j = col;
        else {
          j = next.indexOf(null);
          if (j < 0) j = next.length;
        }
        next[j] = p;
      }
      bottom.push([col, j]);
    });
    while (next.length && next[next.length - 1] === null) next.pop();
    const width = Math.max(col + 1, lanes.length, next.length);
    rows.push({ col, top, bottom, width });
    lanes = next;
  }
  return rows;
}

// ================= ハンク・行ごとのステージ =================
// 文字列は '\n' で分けた行の配列として扱う（Text.of(s.split('\n')) と join('\n') が対になる）

type Lines = { a: string[]; b: string[]; A: Text; B: Text };

function linesOf(a: string, b: string): Lines {
  const al = a.split('\n');
  const bl = b.split('\n');
  return { a: al, b: bl, A: Text.of(al), B: Text.of(bl) };
}

/** まとまりの位置（文字）→ 行の範囲 [start, end)（0 始まり） */
function span(doc: Text, from: number, to: number): [number, number] {
  const s = doc.lineAt(Math.min(from, doc.length)).number - 1;
  if (to <= from) return [s, s];
  const e = to > doc.length ? doc.lines : doc.lineAt(to).number - 1;
  return [s, e];
}

/**
 * a（変更前）のうち、1 つのまとまりだけを b（変更後）の内容にした文字列を返す。
 * ステージ: a = ステージ、b = 作業ツリー → 新しいステージ
 */
export function applyChunk(a: string, b: string, chunk: Chunk): string {
  const L = linesOf(a, b);
  const [sA, eA] = span(L.A, chunk.fromA, chunk.toA);
  const [sB, eB] = span(L.B, chunk.fromB, chunk.toB);
  return [...L.a.slice(0, sA), ...L.b.slice(sB, eB), ...L.a.slice(eA)].join('\n');
}

/**
 * b（変更後）のうち、1 つのまとまりだけを a（変更前）の内容に戻した文字列を返す。
 * 破棄: a = ステージ、b = 作業ツリー → 新しい作業ツリー。ステージを外す: a = HEAD、b = ステージ → 新しいステージ
 */
export function revertChunk(a: string, b: string, chunk: Chunk): string {
  const L = linesOf(a, b);
  const [sA, eA] = span(L.A, chunk.fromA, chunk.toA);
  const [sB, eB] = span(L.B, chunk.fromB, chunk.toB);
  return [...L.b.slice(0, sB), ...L.a.slice(sA, eA), ...L.b.slice(eB)].join('\n');
}

/**
 * 選んだ行（b の文字位置 from〜to を含む行）だけをステージした文字列を返す。
 * 範囲にまとまり全体が入るものはそのまま、一部だけのものは「削除はそのまま残し、選んだ追加の行だけ足す」
 */
export function applyLines(a: string, b: string, chunks: readonly Chunk[], from: number, to: number): string {
  const L = linesOf(a, b);
  const selS = L.B.lineAt(Math.min(from, L.B.length)).number - 1;
  const selE = L.B.lineAt(Math.min(to, L.B.length)).number; // 含まない
  const out: string[] = [];
  let posA = 0;
  for (const ch of chunks) {
    const [sA, eA] = span(L.A, ch.fromA, ch.toA);
    const [sB, eB] = span(L.B, ch.fromB, ch.toB);
    const whole = sB === eB ? sB >= selS && sB <= selE : sB >= selS && eB <= selE;
    const s = Math.max(sB, selS);
    const e = Math.min(eB, selE);
    if (!whole && s >= e) continue;
    out.push(...L.a.slice(posA, sA));
    if (whole) out.push(...L.b.slice(sB, eB));
    else out.push(...L.a.slice(sA, eA), ...L.b.slice(s, e));
    posA = eA;
  }
  out.push(...L.a.slice(posA));
  return out.join('\n');
}

/** 変更前と変更後のまとまり */
export function chunksOf(a: string, b: string): readonly Chunk[] {
  return Chunk.build(Text.of(a.split('\n')), Text.of(b.split('\n')));
}

// ================= 表示 =================

/** 変わったファイルをツリーで並べるときの 1 行。フォルダはそのフォルダの中のファイル数を持つ */
export type PathTreeRow<T> =
  | { kind: 'dir'; path: string; name: string; depth: number; count: number }
  | { kind: 'file'; item: T; name: string; depth: number };

type PathNode<T> = { dirs: Map<string, PathNode<T>>; files: T[]; count: number };

/**
 * パスの一覧をフォルダのツリーにして、表示する順の行にする。フォルダが先、それぞれ名前順。
 * フォルダ 1 つだけを含むフォルダは `a/b` のように 1 行にまとめる。閉じたフォルダ（collapsed が true）の中は出さない
 */
export function pathTreeRows<T extends { path: string }>(items: T[], collapsed: (dir: string) => boolean): PathTreeRow<T>[] {
  const node = (): PathNode<T> => ({ dirs: new Map(), files: [], count: 0 });
  const root = node();
  for (const it of items) {
    const parts = it.path.replace(/\/+$/, '').split('/');
    let n = root;
    n.count++;
    for (const p of parts.slice(0, -1)) {
      let c = n.dirs.get(p);
      if (!c) n.dirs.set(p, (c = node()));
      c.count++;
      n = c;
    }
    n.files.push(it);
  }
  const cmp = (a: string, b: string) => a.localeCompare(b, 'ja');
  const nameOf = (p: string) => p.replace(/\/+$/, '').split('/').pop() ?? p;
  const out: PathTreeRow<T>[] = [];
  const walk = (n: PathNode<T>, prefix: string, depth: number) => {
    for (const [first, child] of [...n.dirs].sort((a, b) => cmp(a[0], b[0]))) {
      let name = first;
      let c = child;
      while (c.files.length === 0 && c.dirs.size === 1) {
        const [[k, v]] = c.dirs;
        name += '/' + k;
        c = v;
      }
      const path = prefix ? `${prefix}/${name}` : name;
      out.push({ kind: 'dir', path, name, depth, count: c.count });
      if (!collapsed(path)) walk(c, path, depth + 1);
    }
    for (const f of n.files.map((item) => ({ item, name: nameOf(item.path) })).sort((a, b) => cmp(a.name, b.name))) {
      out.push({ kind: 'file', item: f.item, name: f.name, depth });
    }
  };
  walk(root, '', 0);
  return out;
}

export function shortHash(h: string): string {
  return h.slice(0, 7);
}

export function relativeTime(unixSec: number, locale: 'ja' | 'en' | 'zh'): string {
  const diff = Date.now() / 1000 - unixSec;
  const fmt = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (diff < 60) return fmt.format(-Math.round(diff), 'second');
  if (diff < 3600) return fmt.format(-Math.round(diff / 60), 'minute');
  if (diff < 86400) return fmt.format(-Math.round(diff / 3600), 'hour');
  if (diff < 86400 * 30) return fmt.format(-Math.round(diff / 86400), 'day');
  if (diff < 86400 * 365) return fmt.format(-Math.round(diff / 86400 / 30), 'month');
  return fmt.format(-Math.round(diff / 86400 / 365), 'year');
}

/** ブランチ名に使えない名前か（git check-ref-format の主なもの） */
export function invalidBranchName(name: string): boolean {
  return (
    !name ||
    /[\s~^:?*[\\\x00-\x1f\x7f]/.test(name) ||
    name.includes('..') ||
    name.includes('@{') ||
    name.startsWith('-') ||
    name.startsWith('/') ||
    name.endsWith('/') ||
    name.endsWith('.') ||
    name.endsWith('.lock') ||
    name.includes('//')
  );
}
