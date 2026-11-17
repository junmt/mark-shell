<script lang="ts">
  import { store, type AiUsageState } from '../lib/store.svelte';
  import TerminalView from './TerminalView.svelte';
  import Icon from './Icon.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { contains, layout, ratioAt, type Divider, type Rect } from '../lib/termLayout';
  import type { ToolInfo } from '../lib/api';

  // ターミナルの切り替えは左ペインの一覧（TerminalList）で行う。
  // 分割したターミナルは、選んでいるターミナルのグループ（木）を左右・上下に並べて表示する。
  // 木が変わってもターミナルを作り直さないよう、全ターミナルを同じ親に置き、位置だけを変える

  let width = $state(0);
  let height = $state(0);

  const groupIndex = $derived(store.activeTerminal === null ? -1 : store.termLayouts.findIndex((l) => contains(l, store.activeTerminal!)));
  const placed = $derived.by(() => {
    const rects = new Map<number, Rect>();
    let dividers: Divider[] = [];
    store.termLayouts.forEach((l, i) => {
      const r = layout(l);
      for (const [k, rect] of r.rects) rects.set(k, rect);
      if (i === groupIndex) dividers = r.dividers;
    });
    return { rects, dividers };
  });
  /** 選んでいるグループのターミナル */
  const visibleKeys = $derived.by(() => {
    const g = store.termLayouts[groupIndex];
    const out = new Set<number>();
    if (g) for (const k of layout(g).rects.keys()) out.add(k);
    return out;
  });
  const splitShown = $derived(visibleKeys.size > 1);

  /** Usage コマンドが指定されている AI ツール */
  const usageItems = $derived(
    store.aiTools
      .map((tool) => ({ tool, cmd: store.usageCommandFor(tool), state: store.aiUsage.get(tool.id) }))
      .filter((x) => x.cmd.length > 0),
  );

  // ターミナル表示中、設定された間隔（既定 30 秒）で各 AI の Usage コマンドを実行する
  $effect(() => {
    const visible = store.terminalVisible;
    const interval = store.config.terminal.usageInterval;
    const sig = store.aiTools.map((t) => `${t.id}:${store.usageCommandFor(t)}`).join('\n');
    void sig;
    if (!visible) return;
    void store.refreshAiUsage();
    if (interval <= 0) return;
    const id = setInterval(() => {
      if (document.visibilityState !== 'hidden') void store.refreshAiUsage();
    }, interval * 1000);
    return () => clearInterval(id);
  });

  function oneLine(s: string): string {
    return s
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter(Boolean)
      .join(' · ');
  }

  function usageTooltip(tool: ToolInfo, cmd: string, state: AiUsageState | undefined): string {
    const lines = [`${tool.name} (${cmd})`];
    if (state?.error) lines.push(state.error);
    else if (state?.output) lines.push(state.output);
    return lines.join('\n');
  }

  function pct(v: number) {
    return `${v * 100}%`;
  }

  // ---- 境界のドラッグ ----
  let drag: { divider: Divider; start: number; ratio: number } | null = null;

  function down(e: PointerEvent, d: Divider) {
    const g = store.termLayouts[groupIndex];
    if (!g) return;
    drag = { divider: d, start: d.dir === 'row' ? e.clientX : e.clientY, ratio: ratioAt(g, d.path) };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const d = drag.divider;
    const size = d.dir === 'row' ? d.parent.w * width : d.parent.h * height;
    if (size <= 0) return;
    const delta = (d.dir === 'row' ? e.clientX : e.clientY) - drag.start;
    store.setTermRatio(groupIndex, d.path, drag.ratio + delta / size);
  }

  function up() {
    drag = null;
  }
</script>

<div class="panel">
  <div class="views" bind:clientWidth={width} bind:clientHeight={height}>
    {#each store.terminals as t (t.key)}
      {@const r = placed.rects.get(t.key) ?? { x: 0, y: 0, w: 1, h: 1 }}
      <div
        class="view"
        class:hidden={!visibleKeys.has(t.key)}
        class:focused={splitShown && store.activeTerminal === t.key}
        style:left={pct(r.x)}
        style:top={pct(r.y)}
        style:width={pct(r.w)}
        style:height={pct(r.h)}
        onpointerdown={() => (store.activeTerminal = t.key)}
        role="presentation"
      >
        <TerminalView session={t} active={store.activeTerminal === t.key && store.terminalVisible} />
      </div>
    {/each}
    {#each placed.dividers as d (d.path.join('') + d.dir)}
      <div
        class="divider {d.dir}"
        style:left={pct(d.rect.x)}
        style:top={pct(d.rect.y)}
        style:width={d.dir === 'row' ? null : pct(d.rect.w)}
        style:height={d.dir === 'column' ? null : pct(d.rect.h)}
        role="separator"
        aria-orientation={d.dir === 'row' ? 'vertical' : 'horizontal'}
        onpointerdown={(e) => down(e, d)}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
      ></div>
    {/each}
    {#if store.terminals.length === 0}
      <div class="empty">
        <button class="btn" onclick={() => store.newTerminal()}>{tr('ターミナルを開く', 'Open terminal', '打开终端')}</button>
      </div>
    {/if}
  </div>
  <div class="usage-bar" aria-label={tr('AI Usage', 'AI Usage', 'AI 用量')}>
    <div class="usage-list">
      {#if usageItems.length > 0}
        {#each usageItems as { tool, cmd, state } (tool.id)}
          <span class="usage-item" title={usageTooltip(tool, cmd, state)}>
            <span class="usage-icon"><Icon name="sparkle" size={12} /></span>
            <span class="usage-name">{tool.name}:</span>
            {#if state?.error}
              <span class="usage-error">{oneLine(state.error)}</span>
            {:else if state?.output}
              <span class="usage-val">{oneLine(state.output)}</span>
            {:else if state?.loading}
              <span class="usage-muted">…</span>
            {:else}
              <span class="usage-muted">—</span>
            {/if}
          </span>
        {/each}
      {:else}
        <span class="usage-item">
          <span class="usage-icon"><Icon name="sparkle" size={12} /></span>
          <span class="usage-name">AI Usage:</span>
          <button class="usage-link" onclick={() => (store.settingsOpen = true)}>
            {tr('設定でコマンドを指定…', 'Configure commands in Settings…', '在设置中指定命令…')}
          </button>
        </span>
      {/if}
    </div>
    <div class="usage-actions">
      {#if usageItems.length > 0}
        <button
          class="icon-btn"
          title={store.config.terminal.usageInterval > 0
            ? tr(
                `Usage を今すぐ更新（自動更新: ${store.config.terminal.usageInterval}秒ごと）`,
                `Refresh usage now (auto-refresh: every ${store.config.terminal.usageInterval}s)`,
                `立即刷新用量（自动刷新：每 ${store.config.terminal.usageInterval} 秒）`,
              )
            : tr('Usage を今すぐ更新（自動更新: オフ）', 'Refresh usage now (auto-refresh: off)', '立即刷新用量（自动刷新：关闭）')}
          onclick={() => void store.refreshAiUsage()}
          disabled={store.aiUsageRefreshing}
        >
          <Icon name="refresh" size={12} />
        </button>
      {/if}
      <button
        class="icon-btn"
        title={tr('AI Usage の設定', 'AI Usage settings', 'AI 用量设置')}
        onclick={() => (store.settingsOpen = true)}
      >
        <Icon name="settings" size={12} />
      </button>
    </div>
  </div>
</div>

<style>
  .panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-pane);
  }

  .views {
    flex: 1;
    min-height: 0;
    position: relative;
    /* ターミナルの周りの余白もターミナルの背景色にする */
    background: var(--terminal-bg, var(--bg-pane));
  }

  .view {
    position: absolute;
  }

  .view.hidden {
    visibility: hidden;
  }

  /* 分割しているときは、選んでいるターミナルの上端に強調色の線を出す */
  .view.focused::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
    z-index: 2;
  }

  /* 境界。見た目は 1px の線、つかめる幅は 7px */
  .divider {
    position: absolute;
    z-index: 5;
  }

  .divider::before {
    content: '';
    position: absolute;
    background: var(--border);
  }

  .divider.row {
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
  }

  .divider.row::before {
    left: 3px;
    top: 0;
    bottom: 0;
    width: 1px;
  }

  .divider.column {
    height: 7px;
    margin-top: -3px;
    cursor: row-resize;
  }

  .divider.column::before {
    top: 3px;
    left: 0;
    right: 0;
    height: 1px;
  }

  .divider:hover::before,
  .divider:active::before {
    background: var(--accent);
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }

  .usage-bar {
    flex: none;
    min-height: 24px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 2px 8px 2px 10px;
    font-size: 12px;
    color: var(--fg-muted);
    background: var(--bg-pane);
    border-top: 1px solid var(--border);
  }

  .usage-list {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    overflow-x: auto;
    white-space: nowrap;
  }

  .usage-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .usage-icon {
    display: inline-flex;
    align-items: center;
    color: var(--accent);
  }

  .usage-name {
    font-weight: 600;
    color: var(--fg);
  }

  .usage-val {
    font-family: var(--editor-mono);
    color: var(--fg);
  }

  .usage-error {
    color: #dc2626;
  }

  .usage-muted {
    color: var(--fg-muted);
  }

  .usage-link {
    border: none;
    background: transparent;
    padding: 0;
    font-size: 12px;
    color: var(--md-link);
    cursor: pointer;
  }

  .usage-link:hover {
    text-decoration: underline;
  }

  .usage-actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
  }
</style>
