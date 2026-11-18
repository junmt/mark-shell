import { beforeEach, describe, expect, it } from 'vitest';
import { i18n } from './i18n.svelte';
import { duration, level, remainingPercent, resetText, staleText, windowLabel } from './usage';

describe('usage', () => {
  beforeEach(() => {
    i18n.locale = 'ja';
  });

  it('枠の名前', () => {
    expect(windowLabel(300)).toBe('5時間');
    expect(windowLabel(10080)).toBe('1週間');
    expect(windowLabel(1440)).toBe('1日');
    expect(windowLabel(120)).toBe('2時間');
    i18n.locale = 'en';
    expect(windowLabel(300)).toBe('5-hour');
  });

  it('残りの割合と色', () => {
    expect(remainingPercent({ windowMinutes: 300, usedPercent: 23.5, resetsAt: 2000 }, 1000)).toBe(76.5);
    // 使いすぎても 0 未満にしない
    expect(remainingPercent({ windowMinutes: 300, usedPercent: 120, resetsAt: null }, 1000)).toBe(0);
    // リセット時刻を過ぎたら全回復
    expect(remainingPercent({ windowMinutes: 300, usedPercent: 90, resetsAt: 500 }, 1000)).toBe(100);
    expect(level(76)).toBe('ok');
    expect(level(50)).toBe('warn');
    expect(level(20)).toBe('low');
  });

  it('回復までの時間', () => {
    expect(duration(2 * 3600 + 14 * 60)).toBe('2時間14分');
    expect(duration(3 * 86400 + 5 * 3600)).toBe('3日5時間');
    expect(duration(30)).toBe('1分未満');
    const l = { windowMinutes: 300, usedPercent: 10, resetsAt: 10000 };
    expect(resetText(l, 10000 - 8040)).toBe('2時間14分後に回復');
    expect(resetText(l, 10001)).toBe('回復しました');
    // 24 時間より先は日時で出す
    expect(resetText({ ...l, resetsAt: 10000 + 3 * 86400 }, 10000)).toMatch(/に回復$/);
    expect(resetText({ ...l, resetsAt: null }, 0)).toBe('');
  });

  it('古い値の注記', () => {
    expect(staleText(1000, 1300)).toBe('');
    expect(staleText(1000, 1000 + 25 * 60)).toBe('25分前の値');
    expect(staleText(null, 1000)).toBe('');
  });
});
