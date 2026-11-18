<script lang="ts">
  import { onMount } from 'svelte';
  import { i18n, tr } from '../lib/i18n.svelte';
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
    type CalendarDay,
  } from '../lib/calendar';
  import Icon from './Icon.svelte';

  const initialNow = new Date();
  let now = $state(initialNow);
  let open = $state(false);
  let viewYear = $state(initialNow.getFullYear());
  let viewMonth = $state(initialNow.getMonth());
  let selected = $state<{ year: number; month: number; day: number }>({
    year: initialNow.getFullYear(),
    month: initialNow.getMonth(),
    day: initialNow.getDate(),
  });
  let hovered = $state<CalendarDay | null>(null);

  let btnEl: HTMLButtonElement | undefined = $state();
  let popEl: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 8, y: 38 });

  onMount(() => {
    const timer = setInterval(() => {
      now = new Date();
    }, 1000);
    return () => clearInterval(timer);
  });

  function toggle() {
    open = !open;
    if (open) {
      now = new Date();
      goToday();
    }
  }

  function goToday() {
    viewYear = now.getFullYear();
    viewMonth = now.getMonth();
    selected = {
      year: now.getFullYear(),
      month: now.getMonth(),
      day: now.getDate(),
    };
    hovered = null;
  }

  function shiftMonth(delta: number) {
    const d = new Date(viewYear, viewMonth + delta, 1);
    viewYear = d.getFullYear();
    viewMonth = d.getMonth();
  }

  function selectDay(cell: CalendarDay) {
    selected = { year: cell.year, month: cell.month, day: cell.day };
  }

  // ポップオーバーが画面端からはみ出さないように位置を調整する
  $effect(() => {
    if (!open || !btnEl) return;
    const btnRect = btnEl.getBoundingClientRect();
    const popWidth = popEl?.offsetWidth ?? 500;
    const popHeight = popEl?.offsetHeight ?? 330;
    const x = Math.max(8, Math.min(btnRect.right - popWidth, window.innerWidth - popWidth - 8));
    const y =
      btnRect.bottom + 4 + popHeight > window.innerHeight
        ? Math.max(8, btnRect.top - popHeight - 4)
        : btnRect.bottom + 4;
    pos = { x, y };
  });

  function onWindowPointerDown(e: PointerEvent) {
    if (!open) return;
    const target = e.target instanceof Node ? e.target : null;
    if (target && (btnEl?.contains(target) || popEl?.contains(target))) return;
    open = false;
  }

  function onWindowKeyDown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    }
  }

  const nextMonthDate = $derived(new Date(viewYear, viewMonth + 1, 1));
  const months = $derived([
    {
      year: viewYear,
      month: viewMonth,
      title: formatMonthTitle(viewYear, viewMonth, i18n.locale),
      rel: formatRelativeMonth(viewYear, viewMonth, now, i18n.locale),
      days: buildCalendarMonth(viewYear, viewMonth, now, selected),
    },
    {
      year: nextMonthDate.getFullYear(),
      month: nextMonthDate.getMonth(),
      title: formatMonthTitle(nextMonthDate.getFullYear(), nextMonthDate.getMonth(), i18n.locale),
      rel: formatRelativeMonth(nextMonthDate.getFullYear(), nextMonthDate.getMonth(), now, i18n.locale),
      days: buildCalendarMonth(nextMonthDate.getFullYear(), nextMonthDate.getMonth(), now, selected),
    },
  ]);

  const weekdays = $derived(getWeekdayLabels(i18n.locale));

  const inspected = $derived.by(() => {
    if (hovered) {
      return {
        year: hovered.year,
        month: hovered.month,
        day: hovered.day,
        diffDays: hovered.diffDays,
      };
    }
    return {
      year: selected.year,
      month: selected.month,
      day: selected.day,
      diffDays: diffCalendarDays(selected.year, selected.month, selected.day, now),
    };
  });
</script>

<svelte:window onpointerdowncapture={onWindowPointerDown} onkeydowncapture={onWindowKeyDown} onblur={() => (open = false)} />

<button
  bind:this={btnEl}
  class="icon-btn clock-btn"
  class:on={open}
  onclick={toggle}
  aria-expanded={open}
  title={tr(
    `${formatFullDateTime(now, 'ja')}（クリックでカレンダーを表示）`,
    `${formatFullDateTime(now, 'en')} (Click to show calendar)`,
    `${formatFullDateTime(now, 'zh')}（点击显示日历）`,
  )}
>
  <Icon name="clock" size={14} />
  <span class="clock-label">{formatClock(now, i18n.locale)}</span>
</button>

{#if open}
  <div
    bind:this={popEl}
    class="calendar-popover"
    role="dialog"
    aria-label={tr('カレンダー', 'Calendar', '日历')}
    style:left="{pos.x}px"
    style:top="{pos.y}px"
  >
    <div class="cal-top">
      <div class="now-summary">
        <Icon name="calendar" size={15} />
        <span class="now-text">{formatFullDateTime(now, i18n.locale)}</span>
      </div>
      <div class="cal-nav">
        <button
          class="icon-btn nav-btn"
          onclick={() => shiftMonth(-1)}
          title={tr('前の月', 'Previous month', '上个月')}
          aria-label={tr('前の月', 'Previous month', '上个月')}
        >
          <Icon name="chevronLeft" size={15} />
        </button>
        <button class="today-btn" onclick={goToday}>
          {tr('今日', 'Today', '今天')}
        </button>
        <button
          class="icon-btn nav-btn"
          onclick={() => shiftMonth(1)}
          title={tr('次の月', 'Next month', '下个月')}
          aria-label={tr('次の月', 'Next month', '下个月')}
        >
          <Icon name="chevronRight" size={15} />
        </button>
      </div>
    </div>

    <div class="cal-months">
      {#each months as m (`${m.year}-${m.month}`)}
        <div class="cal-month">
          <div class="month-header">
            <span class="month-title">{m.title}</span>
            <span class="month-rel" class:current={m.year === now.getFullYear() && m.month === now.getMonth()}>
              {m.rel}
            </span>
          </div>
          <div class="weekdays">
            {#each weekdays as wd, idx (idx)}
              <span class="wd" class:sun={idx === 0} class:sat={idx === 6}>{wd}</span>
            {/each}
          </div>
          <div class="days-grid" role="grid">
            {#each m.days as cell (`${cell.year}-${cell.month}-${cell.day}`)}
              <button
                type="button"
                class="day-cell"
                class:outside={!cell.inMonth}
                class:today={cell.isToday}
                class:selected={cell.isSelected}
                class:sun={cell.weekday === 0}
                class:sat={cell.weekday === 6}
                onclick={() => selectDay(cell)}
                onmouseenter={() => (hovered = cell)}
                onmouseleave={() => (hovered = null)}
                title={`${formatFullDate(cell.year, cell.month, cell.day, i18n.locale)} (${formatRelativeDays(cell.diffDays, i18n.locale)})`}
              >
                {cell.day}
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>

    <div class="cal-footer">
      <span class="inspected-date">{formatFullDate(inspected.year, inspected.month, inspected.day, i18n.locale)}</span>
      <span class="inspected-rel" class:is-today={inspected.diffDays === 0}>
        {formatRelativeDays(inspected.diffDays, i18n.locale)}
      </span>
    </div>
  </div>
{/if}

<style>
  .clock-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    padding: 4px 8px;
    color: var(--toolbar-fg, var(--fg-muted));
  }

  .clock-btn:hover {
    color: var(--toolbar-fg, var(--fg));
    background: var(--toolbar-hover, var(--bg-hover));
  }

  .clock-btn.on {
    color: var(--toolbar-fg, var(--accent));
    background: var(--toolbar-active, var(--bg-active));
  }

  .clock-label {
    font-size: 12.5px;
    letter-spacing: 0.01em;
  }

  .calendar-popover {
    position: fixed;
    z-index: 95;
    background: var(--bg-alt);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-variant-numeric: tabular-nums;
    max-width: calc(100vw - 16px);
  }

  .cal-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  .now-summary {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 13px;
  }

  .cal-nav {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .nav-btn {
    padding: 4px;
    color: var(--fg-muted);
  }

  .nav-btn:hover {
    color: var(--fg);
    background: var(--bg-hover);
  }

  .today-btn {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--fg);
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 4px;
    cursor: pointer;
  }

  .today-btn:hover {
    background: var(--bg-hover);
  }

  .cal-months {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
  }

  .cal-month {
    width: 224px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .month-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4px;
  }

  .month-title {
    font-weight: 700;
    font-size: 13.5px;
  }

  .month-rel {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 10px;
    background: var(--bg-pane);
    color: var(--fg-muted);
    border: 1px solid var(--border);
  }

  .month-rel.current {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    font-weight: 600;
  }

  .weekdays {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    text-align: center;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--fg-muted);
    padding-bottom: 2px;
  }

  .wd.sun {
    color: var(--danger);
  }

  .wd.sat {
    color: var(--accent);
  }

  .days-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
  }

  .day-cell {
    height: 28px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--fg);
    border-radius: 5px;
    font-size: 12.5px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .day-cell.sun {
    color: var(--danger);
  }

  .day-cell.sat {
    color: var(--accent);
  }

  .day-cell.outside {
    opacity: 0.38;
  }

  .day-cell:hover {
    background: var(--bg-hover);
    opacity: 1;
  }

  .day-cell.selected {
    border-color: var(--accent);
    background: var(--bg-active);
    font-weight: 600;
  }

  .day-cell.today {
    background: var(--accent);
    color: var(--accent-fg);
    font-weight: 700;
  }

  .day-cell.today:hover {
    filter: brightness(1.08);
  }

  .cal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
  }

  .inspected-date {
    font-weight: 600;
  }

  .inspected-rel {
    font-size: 12px;
    color: var(--fg-muted);
    background: var(--bg-pane);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .inspected-rel.is-today {
    color: var(--accent);
    font-weight: 600;
  }
</style>
