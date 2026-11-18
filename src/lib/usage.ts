// AI Usage の「あとどれくらい使えるか」の表示（残りの割合、色、回復までの時間）
import { i18n, tr } from './i18n.svelte';

/** レート制限の 1 枠（Rust の rate_limits::Limit） */
export type UsageLimit = { windowMinutes: number; usedPercent: number; resetsAt: number | null };

/** 枠の名前（5時間、1週間 など） */
export function windowLabel(minutes: number): string {
  if (minutes === 300) return tr('5時間', '5-hour', '5小时');
  if (minutes === 10080) return tr('1週間', 'Weekly', '每周');
  if (minutes >= 1440 && minutes % 1440 === 0) {
    const d = minutes / 1440;
    return tr(`${d}日`, `${d}-day`, `${d}天`);
  }
  if (minutes >= 60 && minutes % 60 === 0) {
    const h = minutes / 60;
    return tr(`${h}時間`, `${h}-hour`, `${h}小时`);
  }
  return minutes > 0 ? tr(`${minutes}分`, `${minutes}-min`, `${minutes}分钟`) : tr('上限', 'Limit', '上限');
}

/** 残りの割合（0〜100）。リセット時刻を過ぎていれば 100 */
export function remainingPercent(l: UsageLimit, nowSec: number): number {
  if (l.resetsAt !== null && l.resetsAt <= nowSec) return 100;
  return Math.max(0, Math.min(100, 100 - l.usedPercent));
}

/** 残りの多さ（色分け用） */
export function level(remaining: number): 'ok' | 'warn' | 'low' {
  if (remaining <= 20) return 'low';
  if (remaining <= 50) return 'warn';
  return 'ok';
}

/** 秒を「2時間14分」「3日5時間」などの長さにする */
export function duration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return tr(`${d}日${h}時間`, `${d}d ${h}h`, `${d}天${h}小时`);
  if (h > 0) return tr(`${h}時間${m}分`, `${h}h ${m}m`, `${h}小时${m}分钟`);
  if (m > 0) return tr(`${m}分`, `${m}m`, `${m}分钟`);
  return tr('1分未満', '<1m', '不到1分钟');
}

/** 時刻（11/21(金) 9:00 など） */
function clock(sec: number): string {
  const loc = i18n.locale === 'ja' ? 'ja-JP' : i18n.locale === 'zh' ? 'zh-CN' : 'en-US';
  return new Intl.DateTimeFormat(loc, { month: 'numeric', day: 'numeric', weekday: 'short', hour: 'numeric', minute: '2-digit' }).format(
    new Date(sec * 1000),
  );
}

/** 回復（リセット）までの説明。24 時間以内なら残り時間、それより先なら日時 */
export function resetText(l: UsageLimit, nowSec: number): string {
  if (l.resetsAt === null) return '';
  const left = l.resetsAt - nowSec;
  if (left <= 0) return tr('回復しました', 'Reset', '已恢复');
  if (left < 86400) return tr(`${duration(left)}後に回復`, `Resets in ${duration(left)}`, `${duration(left)}后恢复`);
  return tr(`${clock(l.resetsAt)} に回復`, `Resets ${clock(l.resetsAt)}`, `${clock(l.resetsAt)} 恢复`);
}

/** 値が古いときの注記（10 分より前に記録した値なら「25分前の値」など） */
export function staleText(updatedAt: number | null, nowSec: number): string {
  if (updatedAt === null) return '';
  const age = nowSec - updatedAt;
  if (age < 600) return '';
  return tr(`${duration(age)}前の値`, `as of ${duration(age)} ago`, `${duration(age)}前的数据`);
}
