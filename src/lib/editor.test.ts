import { describe, expect, it } from 'vitest';
import { displayWidth, formatTableText, toggleTaskLine } from './editor';

describe('toggleTaskLine', () => {
  it('チェックの切り替えとタスク化', () => {
    expect(toggleTaskLine('- [ ] 買い物')).toBe('- [x] 買い物');
    expect(toggleTaskLine('  1. [x] 済み')).toBe('  1. [ ] 済み');
    expect(toggleTaskLine('- 項目')).toBe('- [ ] 項目');
    expect(toggleTaskLine('  ただの行')).toBe('  - [ ] ただの行');
  });
});

describe('formatTableText', () => {
  it('全角を幅 2 として列をそろえ、配置を保つ', () => {
    expect(displayWidth('日本a')).toBe(5);
    const out = formatTableText(['|名前|値|', '|:-|-:|', '|りんご|1|', '|b|100|']);
    expect(out).toEqual([
      '| 名前   |  値 |',
      '| :----- | --: |',
      '| りんご |   1 |',
      '| b      | 100 |',
    ]);
  });

  it('表でなければ null', () => {
    expect(formatTableText(['a | b', 'c | d'])).toBeNull();
  });
});
