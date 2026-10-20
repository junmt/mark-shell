import { describe, expect, it } from 'vitest';
import { absToRel, encodeLinkTarget, isInside, normalize, rebase, relative, stem } from './paths';
import { buildTree, flatten } from './tree';

describe('paths', () => {
  it('normalize', () => {
    expect(normalize('a/./b/../c')).toBe('a/c');
    expect(normalize('a\\b')).toBe('a/b');
    expect(normalize('../x')).toBeNull();
  });

  it('relative', () => {
    expect(relative('a/b', 'a/c/d.md')).toBe('../c/d.md');
    expect(relative('', 'x.md')).toBe('x.md');
  });

  it('isInside / rebase', () => {
    expect(isInside('a/b.md', 'a')).toBe(true);
    expect(isInside('ab/b.md', 'a')).toBe(false);
    expect(rebase('old/x/y.md', 'old', 'new')).toBe('new/x/y.md');
  });

  it('absToRel', () => {
    expect(absToRel('C:\\Notes', 'C:\\Notes\\a\\b.png')).toBe('a/b.png');
    expect(absToRel('C:\\Notes\\', 'c:/notes/a.md')).toBe('a.md');
    expect(absToRel('C:\\Notes', 'C:\\Other\\a.md')).toBeNull();
  });

  it('stem / encode', () => {
    expect(stem('a/設計メモ.md')).toBe('設計メモ');
    expect(encodeLinkTarget('my notes/a (1).md')).toBe('my%20notes/a%20%281%29.md');
  });
});

describe('tree', () => {
  const e = (path: string, dir = false, mtime = 0) => ({ path, dir, mtime, size: 0 });

  it('Markdown を含まないフォルダは隠し、フォルダを先に自然順で並べる', () => {
    const root = buildTree(
      [e('b', true), e('b/10.md'), e('b/2.md'), e('empty', true), e('a.md'), e('c', true), e('c/d', true), e('c/d/x.md')],
      { showEmptyDirs: false, sort: 'name', keepDirs: new Set() },
    );
    expect(root.children.map((c) => c.name)).toEqual(['b', 'c', 'a.md']);
    expect(root.children[0].children.map((c) => c.name)).toEqual(['2.md', '10.md']);
    const rows = flatten(root, new Set(['c']));
    expect(rows.map((r) => r.node.path)).toEqual(['b', 'c', 'c/d', 'a.md']);
  });

  it('作ったばかりの空フォルダは表示する', () => {
    const root = buildTree([e('new', true)], { showEmptyDirs: false, sort: 'name', keepDirs: new Set(['new']) });
    expect(root.children.map((c) => c.name)).toEqual(['new']);
  });
});
