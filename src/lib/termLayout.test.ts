import { describe, expect, it } from 'vitest';
import { keysOf, layout, leaf, ratioAt, remove, setRatio, splitAt } from './termLayout';

describe('termLayout', () => {
  it('分割と順番', () => {
    let t = splitAt(leaf(1), 1, 2, 'row');
    t = splitAt(t, 1, 3, 'column');
    expect(keysOf(t)).toEqual([1, 3, 2]);
  });

  it('位置の計算', () => {
    let t = splitAt(leaf(1), 1, 2, 'row');
    t = splitAt(t, 2, 3, 'column');
    const { rects, dividers } = layout(t);
    expect(rects.get(1)).toEqual({ x: 0, y: 0, w: 0.5, h: 1 });
    expect(rects.get(2)).toEqual({ x: 0.5, y: 0, w: 0.5, h: 0.5 });
    expect(rects.get(3)).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
    expect(dividers).toHaveLength(2);
    expect(dividers.find((d) => d.dir === 'row')?.rect).toEqual({ x: 0.5, y: 0, w: 0, h: 1 });
  });

  it('除くと残ったほうが広がる', () => {
    let t = splitAt(leaf(1), 1, 2, 'row');
    t = splitAt(t, 2, 3, 'column');
    const r = remove(t, 2)!;
    expect(keysOf(r)).toEqual([1, 3]);
    expect(layout(r).rects.get(3)).toEqual({ x: 0.5, y: 0, w: 0.5, h: 1 });
    expect(remove(remove(r, 1)!, 3)).toBeNull();
  });

  it('割合は範囲に収める', () => {
    let t = splitAt(leaf(1), 1, 2, 'row');
    t = splitAt(t, 2, 3, 'column');
    expect(ratioAt(setRatio(t, ['b'], 0.8), ['b'])).toBe(0.8);
    expect(ratioAt(setRatio(t, [], 0), [])).toBe(0.1);
    expect(ratioAt(setRatio(t, [], 2), [])).toBe(0.9);
  });
});
