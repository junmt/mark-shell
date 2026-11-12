import { describe, expect, it } from 'vitest';
import { normalizeColor, opaque, parseJsonc, projectColors, projectColorsFromText } from './vscodeColors';

describe('opaque', () => {
  it('透明度があれば下地と混ぜる', () => {
    expect(opaque('#ff0000', '#000000')).toBe('#ff0000');
    expect(opaque('#ff000080', '#000000')).toBe('#800000');
    expect(opaque('#ff000000', '#00ff00')).toBe('#00ff00');
    expect(opaque(undefined, '#000000')).toBeNull();
  });
});

describe('parseJsonc', () => {
  it('コメントと末尾のカンマを許す', () => {
    const text = `{
      // 行コメント
      "a": "http://example.com", /* ブロック
      コメント */
      "b": [1, 2,],
      "c": { "d": "x // y", },
    }`;
    expect(parseJsonc(text)).toEqual({ a: 'http://example.com', b: [1, 2], c: { d: 'x // y' } });
  });

  it('文字列の中のエスケープされた引用符', () => {
    expect(parseJsonc('{"a": "x\\"//y",}')).toEqual({ a: 'x"//y' });
  });

  it('カンマとかっこの間のコメント', () => {
    expect(parseJsonc('{"a": 1, // 最後\n}')).toEqual({ a: 1 });
  });
});

describe('normalizeColor', () => {
  it('いろいろな書き方', () => {
    expect(normalizeColor('#ABC')).toBe('#aabbcc');
    expect(normalizeColor('#abcd')).toBe('#aabbccdd');
    expect(normalizeColor('#112233')).toBe('#112233');
    expect(normalizeColor('#112233ff')).toBe('#112233');
    expect(normalizeColor('#11223380')).toBe('#11223380');
    expect(normalizeColor('red')).toBeUndefined();
    expect(normalizeColor(1)).toBeUndefined();
  });
});

describe('projectColors', () => {
  it('Peacock の書く設定', () => {
    const c = projectColorsFromText(`{
      "workbench.colorCustomizations": {
        "activityBar.background": "#2f7c47",
        "activityBar.foreground": "#e7e7e7",
        "statusBar.background": "#215732",
        "statusBar.foreground": "#e7e7e7",
        "statusBarItem.hoverBackground": "#2f7c47",
        "titleBar.activeBackground": "#215732",
        "titleBar.activeForeground": "#e7e7e7",
        "titleBar.inactiveBackground": "#21573299",
        "titleBar.inactiveForeground": "#e7e7e799",
      },
      "peacock.color": "#215732",
    }`);
    expect(c).toEqual({
      titleBg: '#215732',
      titleFg: '#e7e7e7',
      titleInactiveBg: '#21573299',
      titleInactiveFg: '#e7e7e799',
      toolbarBg: '#2f7c47',
      toolbarFg: '#e7e7e7',
      statusBg: '#215732',
      statusFg: '#e7e7e7',
      statusHoverBg: '#2f7c47',
    });
  });

  it('アクティビティバーがなければタイトルバーの色をツールバーに使う', () => {
    const c = projectColors({ 'workbench.colorCustomizations': { 'titleBar.activeBackground': '#123456' } });
    expect(c).toEqual({ titleBg: '#123456', titleFg: '#ffffff', toolbarBg: '#123456', toolbarFg: '#ffffff' });
  });

  it('明るい背景には暗い文字を補う', () => {
    const c = projectColors({ 'workbench.colorCustomizations': { 'statusBar.background': '#ffe066' } });
    expect(c).toEqual({ statusBg: '#ffe066', statusFg: '#1f2328' });
  });

  it('色の指定がない・読めない', () => {
    expect(projectColors({ 'editor.fontSize': 14 })).toBeNull();
    expect(projectColors({ 'workbench.colorCustomizations': { 'editor.background': '#000000' } })).toBeNull();
    expect(projectColorsFromText('{ 壊れた')).toBeNull();
  });
});
