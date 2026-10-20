// ルートからの相対パス（区切りは `/`）を扱う関数

export function dirname(p: string): string {
  const i = p.lastIndexOf('/');
  return i < 0 ? '' : p.slice(0, i);
}

export function basename(p: string): string {
  const i = p.lastIndexOf('/');
  return i < 0 ? p : p.slice(i + 1);
}

export function extname(p: string): string {
  const name = basename(p);
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

export function stem(p: string): string {
  const name = basename(p);
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(0, i) : name;
}

export function join(...parts: string[]): string {
  return parts.filter((s) => s !== '').join('/');
}

/** `.` と `..` を解決する。ルートより上に出る場合は null */
export function normalize(p: string): string | null {
  const out: string[] = [];
  for (const part of p.split(/[\\/]/)) {
    if (part === '' || part === '.') continue;
    if (part === '..') {
      if (out.length === 0) return null;
      out.pop();
    } else {
      out.push(part);
    }
  }
  return out.join('/');
}

/** fromDir から to への相対パス */
export function relative(fromDir: string, to: string): string {
  const a = fromDir.split('/').filter(Boolean);
  const b = to.split('/').filter(Boolean);
  let i = 0;
  while (i < a.length && i < b.length && a[i].toLowerCase() === b[i].toLowerCase()) i++;
  return [...Array(a.length - i).fill('..'), ...b.slice(i)].join('/');
}

export function isInside(p: string, dir: string): boolean {
  if (dir === '') return true;
  const pl = p.toLowerCase();
  const dl = dir.toLowerCase();
  return pl === dl || pl.startsWith(dl + '/');
}

/** dir の下のパスを newDir の下に付け替える */
export function rebase(p: string, dir: string, newDir: string): string {
  if (p.toLowerCase() === dir.toLowerCase()) return newDir;
  return newDir + p.slice(dir.length);
}

const IMAGE_EXT = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif', 'ico']);
export function isImage(p: string): boolean {
  return IMAGE_EXT.has(extname(p));
}

/** Windows の絶対パスを相対パスに（ルートの外なら null） */
export function absToRel(root: string, abs: string): string | null {
  const r = root.replace(/[\\/]+$/, '').replace(/\//g, '\\').toLowerCase();
  const a = abs.replace(/\//g, '\\');
  if (a.toLowerCase() === r) return '';
  if (!a.toLowerCase().startsWith(r + '\\')) return null;
  return a.slice(r.length + 1).replace(/\\/g, '/');
}

export function toWindows(root: string, rel: string): string {
  const r = root.replace(/[\\/]+$/, '');
  return rel ? `${r}\\${rel.replace(/\//g, '\\')}` : r;
}

/** Markdown のリンク先として書く形（空白などを % エンコード） */
export function encodeLinkTarget(p: string): string {
  return p.replace(/%/g, '%25').replace(/ /g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29');
}

/** 名前の自然順（2.md < 10.md）での比較 */
export const naturalCompare = new Intl.Collator('ja', { numeric: true, sensitivity: 'base' }).compare;
