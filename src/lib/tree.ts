// フラットなファイル一覧からツリーを組み立てる

import type { Entry } from './api';
import { naturalCompare } from './paths';

export type TreeNode = {
  name: string;
  path: string;
  dir: boolean;
  mtime: number;
  children: TreeNode[];
  /** 子孫に Markdown ファイルがあるか */
  hasFiles: boolean;
};

export function buildTree(
  entries: Iterable<Entry>,
  opts: { showEmptyDirs: boolean; sort: 'name' | 'mtime'; keepDirs: Set<string> },
): TreeNode {
  const root: TreeNode = { name: '', path: '', dir: true, mtime: 0, children: [], hasFiles: false };
  const dirs = new Map<string, TreeNode>([['', root]]);

  const ensureDir = (path: string): TreeNode => {
    const key = path.toLowerCase();
    let node = dirs.get(key);
    if (node) return node;
    const i = path.lastIndexOf('/');
    const parent = ensureDir(i < 0 ? '' : path.slice(0, i));
    node = { name: path.slice(i + 1), path, dir: true, mtime: 0, children: [], hasFiles: false };
    parent.children.push(node);
    dirs.set(key, node);
    return node;
  };

  for (const e of entries) {
    if (e.dir) {
      ensureDir(e.path).mtime = e.mtime;
      continue;
    }
    const i = e.path.lastIndexOf('/');
    const parent = ensureDir(i < 0 ? '' : e.path.slice(0, i));
    parent.children.push({ name: e.path.slice(i + 1), path: e.path, dir: false, mtime: e.mtime, children: [], hasFiles: true });
    // 祖先に「ファイルあり」を伝える
    let p: TreeNode | undefined = parent;
    let path = parent.path;
    while (p && !p.hasFiles) {
      p.hasFiles = true;
      const j = path.lastIndexOf('/');
      if (path === '') break;
      path = j < 0 ? '' : path.slice(0, j);
      p = dirs.get(path.toLowerCase());
    }
  }

  const keep = new Set([...opts.keepDirs].map((d) => d.toLowerCase()));
  const finish = (node: TreeNode) => {
    node.children = node.children.filter(
      (c) => !c.dir || c.hasFiles || opts.showEmptyDirs || keep.has(c.path.toLowerCase()) || hasKept(c.path, keep),
    );
    node.children.sort((a, b) => {
      if (a.dir !== b.dir) return a.dir ? -1 : 1;
      if (opts.sort === 'mtime' && !a.dir) return b.mtime - a.mtime || naturalCompare(a.name, b.name);
      return naturalCompare(a.name, b.name);
    });
    node.children.forEach((c) => c.dir && finish(c));
  };
  finish(root);
  return root;
}

function hasKept(path: string, keep: Set<string>): boolean {
  const prefix = path.toLowerCase() + '/';
  for (const k of keep) if (k.startsWith(prefix)) return true;
  return false;
}

export type FlatRow = { node: TreeNode; depth: number };

/** 開いているフォルダだけを展開した、表示用の行 */
export function flatten(root: TreeNode, expanded: Set<string>): FlatRow[] {
  const rows: FlatRow[] = [];
  const walk = (node: TreeNode, depth: number) => {
    for (const c of node.children) {
      rows.push({ node: c, depth });
      if (c.dir && expanded.has(c.path)) walk(c, depth + 1);
    }
  };
  walk(root, 0);
  return rows;
}
