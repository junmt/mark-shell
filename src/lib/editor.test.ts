import { describe, expect, it } from 'vitest';
import { detectLanguage, displayWidth, formatTableText, languageNames, toggleTaskLine } from './editor';

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

describe('detectLanguage', () => {
  it('拡張子から選ぶ', () => {
    expect(detectLanguage('src/a.ts')?.name).toBe('TypeScript');
    expect(detectLanguage('index.php')?.name).toBe('PHP');
    expect(detectLanguage('Dockerfile')?.name).toBe('Dockerfile');
  });

  it('拡張子がなければ先頭の行から選ぶ', () => {
    expect(detectLanguage('bin/run', '#!/usr/bin/env python3\nprint(1)')?.name).toBe('Python');
    expect(detectLanguage('script', '#!/bin/bash\necho hi')?.name).toBe('Shell');
    expect(detectLanguage('page', '<?php echo 1;')?.name).toBe('PHP');
    expect(detectLanguage('notes.log', 'hello')).toBeNull();
  });

  it('選べる言語の一覧に主な言語がある', () => {
    const names = languageNames();
    for (const n of ['TypeScript', 'PHP', 'Python', 'JSON', 'Markdown']) expect(names).toContain(n);
  });
});
