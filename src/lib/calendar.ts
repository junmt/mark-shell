// 時計とカレンダーの日付・曜日・相対日数の計算

import type { Locale } from './i18n.svelte';

export type CalendarDay = {
  year: number;
  month: number; // 0-11
  day: number; // 1-31
  weekday: number; // 0 (Sun) - 6 (Sat)
  inMonth: boolean;
  isToday: boolean;
  isSelected: boolean;
  diffDays: number;
};

const WEEKDAYS_SHORT: Record<Locale, readonly string[]> = {
  ja: ['日', '月', '火', '水', '木', '金', '土'],
  en: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  zh: ['日', '一', '二', '三', '四', '五', '六'],
};

const WEEKDAYS_LONG: Record<Locale, readonly string[]> = {
  ja: ['日曜日', '月曜日', '火曜日', '水曜日', '木曜日', '金曜日', '土曜日'],
  en: ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'],
  zh: ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六'],
};

const MONTHS_EN = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const;

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

/** 曜日見出し（日〜土）を返す */
export function getWeekdayLabels(locale: Locale): readonly string[] {
  return WEEKDAYS_SHORT[locale];
}

/** ツールバーの時計表示（日付・曜日・時刻） */
export function formatClock(date: Date, locale: Locale): string {
  const y = date.getFullYear();
  const m = pad2(date.getMonth() + 1);
  const d = pad2(date.getDate());
  const w = WEEKDAYS_SHORT[locale][date.getDay()];
  const hh = pad2(date.getHours());
  const mm = pad2(date.getMinutes());
  if (locale === 'en') return `${y}-${m}-${d} (${w}) ${hh}:${mm}`;
  if (locale === 'zh') return `${y}/${m}/${d} (周${w}) ${hh}:${mm}`;
  return `${y}/${m}/${d} (${w}) ${hh}:${mm}`;
}

/** カレンダー上部の現在日時（秒まで含む） */
export function formatFullDateTime(date: Date, locale: Locale): string {
  const hh = pad2(date.getHours());
  const mm = pad2(date.getMinutes());
  const ss = pad2(date.getSeconds());
  return `${formatFullDate(date.getFullYear(), date.getMonth(), date.getDate(), locale)} ${hh}:${mm}:${ss}`;
}

/** 年・月・日（0-indexed month）を曜日付きの文字列にする */
export function formatFullDate(year: number, month: number, day: number, locale: Locale): string {
  const d = new Date(year, month, day);
  const w = WEEKDAYS_LONG[locale][d.getDay()];
  if (locale === 'en') return `${w}, ${MONTHS_EN[month]} ${day}, ${year}`;
  if (locale === 'zh') return `${year}年${month + 1}月${day}日（${w}）`;
  return `${year}年${month + 1}月${day}日（${w}）`;
}

/** カレンダーの月見出し（例: 2026年 11月） */
export function formatMonthTitle(year: number, month: number, locale: Locale): string {
  const norm = new Date(year, month, 1);
  const y = norm.getFullYear();
  const m = norm.getMonth();
  if (locale === 'en') return `${MONTHS_EN[m]} ${y}`;
  return `${y}年 ${m + 1}月`;
}

/** 今月から見た相対的な月のラベル（今月・来月・再来月・先月など） */
export function formatRelativeMonth(year: number, month: number, today: Date, locale: Locale): string {
  const norm = new Date(year, month, 1);
  const diff = (norm.getFullYear() - today.getFullYear()) * 12 + (norm.getMonth() - today.getMonth());
  if (diff === 0) return locale === 'ja' ? '今月' : locale === 'zh' ? '本月' : 'This month';
  if (diff === 1) return locale === 'ja' ? '来月' : locale === 'zh' ? '下个月' : 'Next month';
  if (diff === 2) return locale === 'ja' ? '再来月' : locale === 'zh' ? '2个月后' : 'In 2 months';
  if (diff === -1) return locale === 'ja' ? '先月' : locale === 'zh' ? '上个月' : 'Last month';
  if (diff > 0) return locale === 'ja' ? `${diff}か月後` : locale === 'zh' ? `${diff}个月后` : `In ${diff} months`;
  const abs = Math.abs(diff);
  return locale === 'ja' ? `${abs}か月前` : locale === 'zh' ? `${abs}个月前` : `${abs} months ago`;
}

/** 今日から見た相対日数の説明（今日・明日・来週・N日後など） */
export function formatRelativeDays(diffDays: number, locale: Locale): string {
  if (diffDays === 0) return locale === 'ja' ? '今日' : locale === 'zh' ? '今天' : 'Today';
  if (diffDays === 1) return locale === 'ja' ? '明日（1日後）' : locale === 'zh' ? '明天（1天后）' : 'Tomorrow (in 1 day)';
  if (diffDays === -1) return locale === 'ja' ? '昨日（1日前）' : locale === 'zh' ? '昨天（1天前）' : 'Yesterday (1 day ago)';
  if (diffDays > 0) {
    if (diffDays % 7 === 0) {
      const w = diffDays / 7;
      if (locale === 'ja') return w === 1 ? '1週間後（7日後）' : `${w}週間後（${diffDays}日後）`;
      if (locale === 'zh') return `${w}周后（${diffDays}天后）`;
      return w === 1 ? 'In 1 week (7 days)' : `In ${w} weeks (${diffDays} days)`;
    }
    return locale === 'ja' ? `${diffDays}日後` : locale === 'zh' ? `${diffDays}天后` : `In ${diffDays} days`;
  }
  const abs = Math.abs(diffDays);
  if (abs % 7 === 0) {
    const w = abs / 7;
    if (locale === 'ja') return w === 1 ? '1週間前（7日前）' : `${w}週間前（${abs}日前）`;
    if (locale === 'zh') return `${w}周前（${abs}天前）`;
    return w === 1 ? '1 week ago (7 days)' : `${w} weeks ago (${abs} days)`;
  }
  return locale === 'ja' ? `${abs}日前` : locale === 'zh' ? `${abs}天前` : `${abs} days ago`;
}

/** 2つの日付（時刻を無視したローカル日付）の差（日数） */
export function diffCalendarDays(year: number, month: number, day: number, today: Date): number {
  const targetUtc = Date.UTC(year, month, day);
  const todayUtc = Date.UTC(today.getFullYear(), today.getMonth(), today.getDate());
  return Math.round((targetUtc - todayUtc) / 86_400_000);
}

/** 指定した年・月（0-11）の 6 週間分（42 マス）のカレンダー配列を作る */
export function buildCalendarMonth(
  year: number,
  month: number,
  today: Date,
  selected?: { year: number; month: number; day: number } | null,
): CalendarDay[] {
  const firstOfMonth = new Date(year, month, 1);
  const normYear = firstOfMonth.getFullYear();
  const normMonth = firstOfMonth.getMonth();
  const startOffset = firstOfMonth.getDay(); // 0 (Sun) - 6 (Sat)

  const todayY = today.getFullYear();
  const todayM = today.getMonth();
  const todayD = today.getDate();

  const days: CalendarDay[] = [];
  for (let i = 0; i < 42; i++) {
    const cellDate = new Date(normYear, normMonth, 1 - startOffset + i);
    const y = cellDate.getFullYear();
    const m = cellDate.getMonth();
    const d = cellDate.getDate();
    days.push({
      year: y,
      month: m,
      day: d,
      weekday: cellDate.getDay(),
      inMonth: y === normYear && m === normMonth,
      isToday: y === todayY && m === todayM && d === todayD,
      isSelected: !!selected && y === selected.year && m === selected.month && d === selected.day,
      diffDays: diffCalendarDays(y, m, d, today),
    });
  }
  return days;
}
