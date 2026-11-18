import { describe, expect, it } from 'vitest';
import {
  buildCalendarMonth,
  diffCalendarDays,
  formatClock,
  formatFullDate,
  formatFullDateTime,
  formatMonthTitle,
  formatRelativeDays,
  formatRelativeMonth,
  getWeekdayLabels,
} from './calendar';

describe('calendar', () => {
  // 2026-11-19 (Thu) 02:25:18
  const sample = new Date(2026, 10, 19, 2, 25, 18);

  it('formats clock string with date, weekday, and time across locales', () => {
    expect(formatClock(sample, 'ja')).toBe('2026/11/19 (木) 02:25');
    expect(formatClock(sample, 'en')).toBe('2026-11-19 (Thu) 02:25');
    expect(formatClock(sample, 'zh')).toBe('2026/11/19 (周四) 02:25');
  });

  it('formats full date and date-time', () => {
    expect(formatFullDate(2026, 10, 19, 'ja')).toBe('2026年11月19日（木曜日）');
    expect(formatFullDate(2026, 10, 19, 'en')).toBe('Thursday, November 19, 2026');
    expect(formatFullDate(2026, 10, 19, 'zh')).toBe('2026年11月19日（星期四）');
    expect(formatFullDateTime(sample, 'ja')).toBe('2026年11月19日（木曜日） 02:25:18');
  });

  it('formats month titles and relative month labels', () => {
    expect(formatMonthTitle(2026, 10, 'ja')).toBe('2026年 11月');
    expect(formatMonthTitle(2026, 11, 'en')).toBe('December 2026');
    // month overflow (e.g. month 12 -> Jan of next year)
    expect(formatMonthTitle(2026, 12, 'ja')).toBe('2027年 1月');

    expect(formatRelativeMonth(2026, 10, sample, 'ja')).toBe('今月');
    expect(formatRelativeMonth(2026, 11, sample, 'ja')).toBe('来月');
    expect(formatRelativeMonth(2027, 0, sample, 'ja')).toBe('再来月');
    expect(formatRelativeMonth(2026, 9, sample, 'ja')).toBe('先月');
  });

  it('calculates day differences and relative day labels', () => {
    expect(diffCalendarDays(2026, 10, 19, sample)).toBe(0);
    expect(diffCalendarDays(2026, 10, 26, sample)).toBe(7);
    expect(diffCalendarDays(2026, 11, 19, sample)).toBe(30);

    expect(formatRelativeDays(0, 'ja')).toBe('今日');
    expect(formatRelativeDays(1, 'ja')).toBe('明日（1日後）');
    expect(formatRelativeDays(-1, 'ja')).toBe('昨日（1日前）');
    expect(formatRelativeDays(7, 'ja')).toBe('1週間後（7日後）');
    expect(formatRelativeDays(14, 'ja')).toBe('2週間後（14日後）');
    expect(formatRelativeDays(10, 'ja')).toBe('10日後');
  });

  it('builds a 42-day (6-week) calendar grid with correct weekdays and today/selected flags', () => {
    // Nov 1, 2026 is a Sunday (weekday = 0)
    const nov = buildCalendarMonth(2026, 10, sample, { year: 2026, month: 10, day: 26 });
    expect(nov).toHaveLength(42);
    expect(nov[0]).toMatchObject({ year: 2026, month: 10, day: 1, weekday: 0, inMonth: true });
    const todayCell = nov.find((c) => c.isToday);
    expect(todayCell).toMatchObject({ year: 2026, month: 10, day: 19, weekday: 4, diffDays: 0 });
    const selectedCell = nov.find((c) => c.isSelected);
    expect(selectedCell).toMatchObject({ year: 2026, month: 10, day: 26, weekday: 4, diffDays: 7 });
    // Trailing days spill into December 2026
    expect(nov[30]).toMatchObject({ year: 2026, month: 11, day: 1, weekday: 2, inMonth: false });

    expect(getWeekdayLabels('ja')).toEqual(['日', '月', '火', '水', '木', '金', '土']);
  });
});
