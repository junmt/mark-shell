import { describe, expect, it } from 'vitest';
import { isMod, keyLabel } from './platform';

describe('keyLabel', () => {
  it('Windows ではそのまま', () => {
    expect(keyLabel('保存 (Ctrl+S)', false)).toBe('保存 (Ctrl+S)');
  });

  it('Mac では Ctrl+ を Cmd+ に', () => {
    expect(keyLabel('コマンドパレット (Ctrl+Shift+P)', true)).toBe('コマンドパレット (Cmd+Shift+P)');
    expect(keyLabel('Ctrl+ホイール', true)).toBe('Cmd+ホイール');
  });

  it('Mac でも Ctrl のままのもの', () => {
    expect(keyLabel('ターミナル (Ctrl+`)', true)).toBe('ターミナル (Ctrl+`)');
    expect(keyLabel('Ctrl+Shift+5', true)).toBe('Ctrl+Shift+5');
    expect(keyLabel('Ctrl+Tab', true)).toBe('Ctrl+Tab');
  });
});

describe('isMod', () => {
  it('Windows は Ctrl、Mac は Cmd', () => {
    expect(isMod({ ctrlKey: true, metaKey: false }, false)).toBe(true);
    expect(isMod({ ctrlKey: false, metaKey: true }, false)).toBe(false);
    expect(isMod({ ctrlKey: false, metaKey: true }, true)).toBe(true);
    expect(isMod({ ctrlKey: true, metaKey: false }, true)).toBe(false);
  });
});
