import { describe, expect, it } from 'vitest';
import { applyChunk, applyLines, chunksOf, groupStatus, invalidBranchName, layoutGraph, pathTreeRows, repoToRoot, revertChunk, rootToRepo, type Commit, type FileStatus } from './git';

const c = (hash: string, parents: string[]): Commit => ({ hash, parents, author: '', email: '', date: 0, refs: [], subject: '' });

describe('layoutGraph', () => {
  it('一直線の履歴は 1 列', () => {
    const rows = layoutGraph([c('c', ['b']), c('b', ['a']), c('a', [])]);
    expect(rows.map((r) => r.col)).toEqual([0, 0, 0]);
    expect(rows[2].bottom).toEqual([]);
  });

  it('マージは 2 列に分かれて合流する', () => {
    // m は a と b のマージ、b は a から
    const rows = layoutGraph([c('m', ['a', 'b']), c('b', ['a']), c('a', [])]);
    expect(rows[0].col).toBe(0);
    expect(rows[0].bottom).toEqual([
      [0, 0],
      [0, 1],
    ]);
    expect(rows[1].col).toBe(1);
    // b の下半分で a の列に合流する
    expect(rows[1].bottom).toContainEqual([1, 0]);
    expect(rows[2].top).toEqual([[0, 0]]);
  });
});

describe('ハンクの計算', () => {
  const index = 'a\nb\nc\nd\ne\n';
  const work = 'a\nB\nc\nd\nE\nf\n';
  const chunks = chunksOf(index, work);

  it('まとまりは 2 つ', () => {
    expect(chunks.length).toBe(2);
  });

  it('1 つめだけステージする', () => {
    expect(applyChunk(index, work, chunks[0])).toBe('a\nB\nc\nd\ne\n');
  });

  it('2 つめだけステージする', () => {
    expect(applyChunk(index, work, chunks[1])).toBe('a\nb\nc\nd\nE\nf\n');
  });

  it('1 つめだけ破棄する', () => {
    expect(revertChunk(index, work, chunks[0])).toBe('a\nb\nc\nd\nE\nf\n');
  });

  it('末尾に改行のないファイル', () => {
    const a = 'x\ny';
    const b = 'x\ny\nz';
    const ch = chunksOf(a, b);
    expect(applyChunk(a, b, ch[0])).toBe(b);
    expect(revertChunk(a, b, ch[0])).toBe(a);
  });

  it('追加だけのまとまりの一部の行をステージする', () => {
    const a = 'a\nz\n';
    const b = 'a\n1\n2\n3\nz\n';
    const ch = chunksOf(a, b);
    const from = b.indexOf('2');
    expect(applyLines(a, b, ch, from, from)).toBe('a\n2\nz\n');
  });

  it('範囲がまとまり全体を含めばそのままステージする', () => {
    expect(applyLines(index, work, chunks, 0, work.indexOf('c'))).toBe('a\nB\nc\nd\ne\n');
  });
});

describe('パス', () => {
  it('ルートの中のリポジトリ', () => {
    const r = { path: 'C:/n/sub', name: 'sub', prefix: 'sub', rootInRepo: null };
    expect(repoToRoot(r, 'a.md')).toBe('sub/a.md');
    expect(rootToRepo(r, 'sub/a.md')).toBe('a.md');
    expect(rootToRepo(r, 'other/a.md')).toBeNull();
  });

  it('ルートより上のリポジトリ', () => {
    const r = { path: 'C:/w', name: 'w', prefix: null, rootInRepo: 'notes' };
    expect(repoToRoot(r, 'notes/a.md')).toBe('a.md');
    expect(repoToRoot(r, 'src/x.ts')).toBeNull();
    expect(rootToRepo(r, 'a.md')).toBe('notes/a.md');
  });
});

describe('groupStatus', () => {
  it('ステージ済みと変更の両方に出る', () => {
    const f = (o: Partial<FileStatus>): FileStatus => ({ path: 'a', orig: null, x: '.', y: '.', conflict: false, untracked: false, submodule: false, ...o });
    const g = groupStatus([f({ path: 'a', x: 'M', y: 'M' }), f({ path: 'u', untracked: true }), f({ path: 'c', conflict: true })]);
    expect(g.staged.map((x) => x.path)).toEqual(['a']);
    expect(g.unstaged.map((x) => x.path)).toEqual(['a']);
    expect(g.untracked.length).toBe(1);
    expect(g.conflict.length).toBe(1);
  });
});

it('ブランチ名', () => {
  expect(invalidBranchName('feature/x')).toBe(false);
  expect(invalidBranchName('a b')).toBe(true);
  expect(invalidBranchName('a..b')).toBe(true);
  expect(invalidBranchName('x.lock')).toBe(true);
});

describe('pathTreeRows', () => {
  const f = (path: string) => ({ path });
  const show = (rows: ReturnType<typeof pathTreeRows<{ path: string }>>) =>
    rows.map((r) => `${'  '.repeat(r.depth)}${r.kind === 'dir' ? `${r.name}/ (${r.count})` : r.name}`);

  it('フォルダを先に、1 つだけのフォルダはまとめる', () => {
    const rows = pathTreeRows([f('z.md'), f('src/lib/a.ts'), f('src/lib/b.ts'), f('docs/x/y/c.md'), f('src/d.ts')], () => false);
    expect(show(rows)).toEqual(['docs/x/y/ (1)', '  c.md', 'src/ (3)', '  lib/ (2)', '    a.ts', '    b.ts', '  d.ts', 'z.md']);
  });

  it('閉じたフォルダの中は出さない', () => {
    const rows = pathTreeRows([f('src/lib/a.ts'), f('src/d.ts')], (d) => d === 'src/lib');
    expect(show(rows)).toEqual(['src/ (2)', '  lib/ (1)', '  d.ts']);
  });
});
