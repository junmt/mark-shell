// ターミナルの分割。ターミナルの一覧の 1 項目（グループ）を、左右・上下に分けた木で表す。
// 木が変わってもターミナル（xterm.js と PTY）を作り直さないよう、画面では全ターミナルを同じ親に置き、
// ここで計算した位置（割合）に重ねて表示する

/** row: 左右に並べる、column: 上下に並べる */
export type SplitDir = 'row' | 'column';

export type TermLayout =
  | { kind: 'leaf'; key: number }
  | { kind: 'split'; dir: SplitDir; /** a の大きさの割合 */ ratio: number; a: TermLayout; b: TermLayout };

/** 位置と大きさ（親に対する 0〜1 の割合） */
export type Rect = { x: number; y: number; w: number; h: number };

/** 境界。path は木の中の分割の位置（a / b の並び） */
export type Divider = { path: ('a' | 'b')[]; dir: SplitDir; rect: Rect; parent: Rect };

export const MIN_RATIO = 0.1;

export function leaf(key: number): TermLayout {
  return { kind: 'leaf', key };
}

/** 木に含まれるターミナル（左上から順に） */
export function keysOf(node: TermLayout): number[] {
  return node.kind === 'leaf' ? [node.key] : [...keysOf(node.a), ...keysOf(node.b)];
}

export function contains(node: TermLayout, key: number): boolean {
  return node.kind === 'leaf' ? node.key === key : contains(node.a, key) || contains(node.b, key);
}

/** key の場所を、key と added の 2 つに分ける（added は右か下） */
export function splitAt(node: TermLayout, key: number, added: number, dir: SplitDir): TermLayout {
  if (node.kind === 'leaf') {
    return node.key === key ? { kind: 'split', dir, ratio: 0.5, a: node, b: leaf(added) } : node;
  }
  return { ...node, a: splitAt(node.a, key, added, dir), b: splitAt(node.b, key, added, dir) };
}

/** key を除く。残りがなければ null（分割は残ったほうに置き換わる） */
export function remove(node: TermLayout, key: number): TermLayout | null {
  if (node.kind === 'leaf') return node.key === key ? null : node;
  const a = remove(node.a, key);
  const b = remove(node.b, key);
  if (!a) return b;
  if (!b) return a;
  return { ...node, a, b };
}

/** path の分割の割合を変える */
export function setRatio(node: TermLayout, path: ('a' | 'b')[], ratio: number): TermLayout {
  if (node.kind === 'leaf') return node;
  if (path.length === 0) return { ...node, ratio: Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, ratio)) };
  const [head, ...rest] = path;
  return head === 'a' ? { ...node, a: setRatio(node.a, rest, ratio) } : { ...node, b: setRatio(node.b, rest, ratio) };
}

/** 各ターミナルの位置と、境界の位置を計算する */
export function layout(node: TermLayout): { rects: Map<number, Rect>; dividers: Divider[] } {
  const rects = new Map<number, Rect>();
  const dividers: Divider[] = [];
  const walk = (n: TermLayout, r: Rect, path: ('a' | 'b')[]) => {
    if (n.kind === 'leaf') {
      rects.set(n.key, r);
      return;
    }
    if (n.dir === 'row') {
      const w = r.w * n.ratio;
      walk(n.a, { ...r, w }, [...path, 'a']);
      walk(n.b, { ...r, x: r.x + w, w: r.w - w }, [...path, 'b']);
      dividers.push({ path, dir: n.dir, rect: { x: r.x + w, y: r.y, w: 0, h: r.h }, parent: r });
    } else {
      const h = r.h * n.ratio;
      walk(n.a, { ...r, h }, [...path, 'a']);
      walk(n.b, { ...r, y: r.y + h, h: r.h - h }, [...path, 'b']);
      dividers.push({ path, dir: n.dir, rect: { x: r.x, y: r.y + h, w: r.w, h: 0 }, parent: r });
    }
  };
  walk(node, { x: 0, y: 0, w: 1, h: 1 }, []);
  return { rects, dividers };
}

/** path の分割の割合 */
export function ratioAt(node: TermLayout, path: ('a' | 'b')[]): number {
  let n = node;
  for (const p of path) {
    if (n.kind === 'leaf') return 0.5;
    n = n[p];
  }
  return n.kind === 'split' ? n.ratio : 0.5;
}
