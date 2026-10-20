import { describe, expect, it } from 'vitest';
import { PRESETS, fontStack, luminance, mix, presetFor, resolveTheme } from './themes';

describe('配色', () => {
  it('Windows の設定に合わせるとライト / ダークを選ぶ', () => {
    expect(presetFor('system', true)).toBe('dark');
    expect(presetFor('system', false)).toBe('light');
    expect(presetFor('nord', false)).toBe('nord');
    expect(presetFor('なし', false)).toBe('light');
  });

  it('ライト・ダークは以前と同じ色になる', () => {
    const light = resolveTheme('light', { base: 'light', colors: {} }, false);
    expect(light.dark).toBe(false);
    expect(light.vars['bg']).toBe('#ffffff');
    expect(light.vars['bg-hover']).toBe('#e8eaee');
    expect(light.vars['accent-fg']).toBe('#ffffff');
    const dark = resolveTheme('system', { base: 'light', colors: {} }, true);
    expect(dark.dark).toBe(true);
    expect(dark.vars['bg']).toBe('#1e1f22');
    expect(dark.vars['code-keyword']).toBe('#ff7b72');
  });

  it('すべてのプリセットで中間の色が作れる', () => {
    for (const id of Object.keys(PRESETS)) {
      const t = resolveTheme(id, { base: 'light', colors: {} }, false);
      expect(t.vars['bg-active']).toMatch(/^(#|rgba)/);
      expect(t.terminal.selectionBackground).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it('カスタムは元の配色に変えた色を重ね、背景の明るさで明暗を決める', () => {
    const t = resolveTheme('custom', { base: 'light', colors: { bg: '#101010', accent: '#ffd400', fg: 'ちがう' } }, false);
    expect(t.vars['bg']).toBe('#101010');
    expect(t.vars['fg']).toBe('#1f2328'); // 色でない値は無視する
    expect(t.dark).toBe(true);
    expect(t.vars['accent-fg']).toBe('#000000'); // 明るい強調色には黒い文字
    expect(t.vars['bg-hover']).not.toBe('#e8eaee'); // 色を変えたら中間の色も計算し直す
  });

  it('色を混ぜる・明るさを測る', () => {
    expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080');
    expect(mix('#fff', '#000', 0)).toBe('#ffffff');
    expect(luminance('#ffffff')).toBeCloseTo(1);
    expect(luminance('#000000')).toBe(0);
  });

  it('フォント名を font-family にする', () => {
    expect(fontStack('', 'sans-serif')).toBe('sans-serif');
    expect(fontStack('BIZ UDGothic', 'monospace')).toBe('"BIZ UDGothic", monospace');
    expect(fontStack('A, B', 'serif')).toBe('A, B');
  });
});
