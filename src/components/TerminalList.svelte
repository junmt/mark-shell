<script lang="ts">
  import { untrack } from 'svelte';
  import { store, type AiUsageState, type MenuItem, type TermSession } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  import Splitter from './Splitter.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { keysOf } from '../lib/termLayout';
  import type { ToolInfo } from '../lib/api';
  import { level, limitLabel, remainingPercent, resetText, staleText } from '../lib/usage';

  // 左ペインの下側に並べるターミナルの一覧と、その下の各 AI の Usage 表示

  const usageItems = $derived(
    store.usageTargets().map(({ tool, cmd }) => ({
      tool,
      cmd,
      state: store.aiUsage.get(tool.id),
    })),
  );

  // 設定された間隔（既定 30 秒）で各 AI の Usage コマンドを実行する。
  // 間隔と対象（ツールとコマンド）が変わったときだけ実行し直す。refreshAiUsage は中で aiUsage・aiUsageRefreshing・
  // activeDoc を読むので、untrack しないとそれらの変化（＝実行の完了）でこの effect が走り直し、
  // コマンドを間隔なしで延々と起動し続ける
  $effect(() => {
    const interval = store.config.terminal.usageInterval;
    const sig = store.usageTargets().map((x) => `${x.tool.id}:${x.cmd}`).join('\n');
    void sig;
    untrack(() => void store.refreshAiUsage());
    if (interval <= 0) return;
    const id = setInterval(() => {
      if (document.visibilityState !== 'hidden') void store.refreshAiUsage();
    }, interval * 1000);
    return () => clearInterval(id);
  });

  // 回復までの時間の表示を進める（UNIX 秒）
  let now = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const id = setInterval(() => (now = Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(id);
  });

  function usageTooltip(tool: ToolInfo, cmd: string, state: AiUsageState | undefined): string {
    const lines = [tool.name];
    for (const l of state?.limits ?? []) {
      const rem = Math.round(remainingPercent(l, now));
      const reset = resetText(l, now);
      lines.push(`${limitLabel(l)}: ${tr(`残り ${rem}%`, `${rem}% left`, `剩余 ${rem}%`)}${reset ? ` — ${reset}` : ''}`);
    }
    lines.push(`${tr('コマンド', 'Command', '命令')}: ${cmd}`);
    if (state?.error) lines.push(state.error);
    else if (state?.detail) lines.push(state.detail);
    else if (state?.output) lines.push(state.output);
    return lines.join('\n');
  }

  // AI Usage の高さ（境界をドラッグして変える。中身より低いとスクロールする）
  let usageEl: HTMLDivElement | undefined = $state();
  function resizeUsage(d: number) {
    const cur = store.usageHeight ?? usageEl?.clientHeight ?? 120;
    store.usageHeight = Math.min(Math.max(50, cur - d), 800);
  }

  function newMenu(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = store.shells.map((s) => ({ label: s.name, action: () => store.newTerminal({ shellId: s.id }) }));
    const tools = store.aiTools.filter((t) => t.found && t.kind === 'cli');
    if (tools.length) {
      items.push({ label: '', separator: true });
      for (const t of tools) items.push({ label: `✦ ${t.name}`, action: () => void store.launchTool(t) });
    }
    store.menu = { x: Math.min(rect.left, window.innerWidth - 260), y: rect.bottom + 2, items };
  }

  function status(t: TermSession): string {
    if (t.exited) return tr('終了', 'Exited', '已退出');
    if (t.busy) return tr('処理中', 'Working', '处理中');
    return tr('待機中', 'Idle', '空闲');
  }

  /** ツールチップ: 名前、AI が付けたタイトル、状態 */
  function tooltip(t: TermSession): string {
    return [t.title, t.osTitle, status(t)].filter(Boolean).join(' — ');
  }

  /** 分割したグループの中での位置（一覧で、同じグループのターミナルをつないで見せる） */
  const groupPos = $derived.by(() => {
    const pos = new Map<number, 'first' | 'middle' | 'last'>();
    for (const l of store.termLayouts) {
      const keys = keysOf(l);
      if (keys.length < 2) continue;
      keys.forEach((k, i) => pos.set(k, i === 0 ? 'first' : i === keys.length - 1 ? 'last' : 'middle'));
    }
    return pos;
  });

  function itemMenu(e: MouseEvent, t: TermSession) {
    store.openMenu(e, [
      { label: tr('左右に分割', 'Split right', '向右拆分'), shortcut: 'Ctrl+Shift+5', action: () => store.splitTerminal('row', t.key) },
      { label: tr('上下に分割', 'Split down', '向下拆分'), action: () => store.splitTerminal('column', t.key) },
      { label: '', separator: true },
      { label: tr('閉じる', 'Close', '关闭'), action: () => store.closeTerminal(t.key) },
    ]);
  }

  function select(t: TermSession) {
    store.activeTerminal = t.key;
    if (!store.terminalVisible) {
      store.terminalVisible = true;
      store.saveWorkspaceState();
    }
  }
</script>

<div class="list">
  <div class="terms" style:height="{store.terminalListHeight}px">
    <div class="header">
      <span class="label">{tr('ターミナル', 'Terminals', '终端')}</span>
      <div class="spacer"></div>
      <button class="icon-btn" title={tr('新しいターミナル', 'New terminal', '新建终端')} onclick={() => store.newTerminal()}><Icon name="plus" size={14} /></button>
      <button
        class="icon-btn"
        title={tr('ターミナルを左右に分割 (Ctrl+Shift+5)', 'Split terminal right (Ctrl+Shift+5)', '向右拆分终端 (Ctrl+Shift+5)')}
        onclick={() => store.splitTerminal('row')}
        disabled={store.terminals.length === 0}><Icon name="splitRight" size={14} /></button
      >
      <button
        class="icon-btn"
        title={tr('ターミナルを上下に分割', 'Split terminal down', '向下拆分终端')}
        onclick={() => store.splitTerminal('column')}
        disabled={store.terminals.length === 0}><Icon name="splitDown" size={14} /></button
      >
      <button class="icon-btn" title={tr('シェルや AI を選んで開く', 'Choose a shell or AI tool to open', '选择要打开的 Shell 或 AI')} onclick={newMenu}>
        <Icon name="chevronDown" size={12} stroke={2} />
      </button>
      <button
        class="icon-btn"
        class:flip={!store.terminalVisible}
        title={store.terminalVisible
          ? tr('ターミナルを隠す (Ctrl+`)', 'Hide terminal (Ctrl+`)', '隐藏终端 (Ctrl+`)')
          : tr('ターミナルを表示 (Ctrl+`)', 'Show terminal (Ctrl+`)', '显示终端 (Ctrl+`)')}
        onclick={() => store.toggleTerminal()}
      >
        <Icon name="chevronDown" size={14} stroke={2} />
      </button>
    </div>
    <div class="items" role="tablist" aria-orientation="vertical">
      {#each store.terminals as t (t.key)}
        <div
          class="item"
          class:active={store.activeTerminal === t.key}
          class:exited={t.exited}
          class:grouped={groupPos.has(t.key)}
          class:group-first={groupPos.get(t.key) === 'first'}
          class:group-last={groupPos.get(t.key) === 'last'}
          role="tab"
          tabindex="0"
          aria-selected={store.activeTerminal === t.key}
          title={tooltip(t)}
          onclick={() => select(t)}
          onkeydown={(e) => e.key === 'Enter' && select(t)}
          oncontextmenu={(e) => itemMenu(e, t)}
        >
          <!-- 状態: 処理中は点滅、待機中は小さな点、終了は輪 -->
          <span class="status" class:busy={t.busy} class:exited={t.exited} aria-label={status(t)}></span>
          <span class="kind"><Icon name={t.ai ? 'sparkle' : 'terminal'} size={13} /></span>
          <span class="text">
            <span class="title">{t.title}{#if t.exited}<span class="state">{tr('（終了）', '(exited)', '（已退出）')}</span>{/if}</span>
            {#if t.ai && t.osTitle}<span class="sub">{t.osTitle}</span>{/if}
          </span>
          <button
            class="close"
            title={tr('閉じる', 'Close', '关闭')}
            onclick={(e) => {
              e.stopPropagation();
              store.closeTerminal(t.key);
            }}>✕</button
          >
        </div>
      {/each}
      {#if store.terminals.length === 0}
        <button class="open" onclick={() => store.newTerminal()}>{tr('ターミナルを開く', 'Open terminal', '打开终端')}</button>
      {/if}
    </div>
  </div>

  <Splitter direction="horizontal" onresize={resizeUsage} ondone={() => store.saveWorkspaceState()} />

  <div class="usage" bind:this={usageEl} style:height={store.usageHeight === null ? undefined : `${store.usageHeight}px`} aria-label={tr('AI Usage', 'AI Usage', 'AI 用量')}>
    <div class="header usage-header">
      <span class="label">AI Usage</span>
      <div class="spacer"></div>
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
        <Icon name="refresh" size={13} />
      </button>
      <button
        class="icon-btn"
        title={tr('AI Usage の設定', 'AI Usage settings', 'AI 用量设置')}
        onclick={() => (store.settingsOpen = true)}
      >
        <Icon name="settings" size={13} />
      </button>
    </div>
    <div class="usage-items">
      {#each usageItems as { tool, cmd, state } (tool.id)}
        <div class="usage-block" title={usageTooltip(tool, cmd, state)}>
          <div class="usage-row">
            <span class="usage-tool">
              <span class="usage-icon"><Icon name="sparkle" size={12} /></span>
              <span class="usage-name">{tool.name}</span>
            </span>
            {#if state?.error}
              <span class="usage-val error">{state.error}</span>
            {:else if state?.output}
              <span class="usage-val">{state.output}</span>
            {:else if state?.loading}
              <span class="usage-val muted">…</span>
            {:else if !state?.limits.length}
              <span class="usage-val muted">—</span>
            {/if}
          </div>
          {#if state?.limits.length}
            <div class="limits">
              {#each state.limits as l, i (i)}
                {@const rem = remainingPercent(l, now)}
                <span class="limit-name">{limitLabel(l)}</span>
                <span
                  class="bar"
                  role="meter"
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(rem)}
                  aria-label={limitLabel(l)}
                >
                  <span class="fill {level(rem)}" style:width="{rem}%"></span>
                </span>
                <span class="limit-pct {level(rem)}">{tr(`残り ${Math.round(rem)}%`, `${Math.round(rem)}% left`, `剩余 ${Math.round(rem)}%`)}</span>
                {#if l.resetsAt !== null}
                  <span class="limit-reset">{resetText(l, now)}</span>
                {/if}
              {/each}
            </div>
            {@const stale = staleText(state.limitsUpdatedAt, now)}
            {#if stale}
              <div class="limit-note">{stale}</div>
            {/if}
          {:else if state?.limitsNote}
            <div class="limit-note">{state.limitsNote}</div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
  }

  .terms {
    min-height: 60px;
    display: flex;
    flex-direction: column;
  }

  .header {
    display: flex;
    align-items: center;
    flex: none;
    height: 28px;
    padding: 0 4px 0 10px;
    gap: 2px;
    border-bottom: 1px solid var(--border);
  }

  .label {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .spacer {
    flex: 1;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
  }

  .icon-btn.flip :global(svg) {
    transform: rotate(180deg);
  }

  .items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 4px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px 3px 8px;
    border-radius: 4px;
    cursor: pointer;
    color: var(--fg-muted);
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.active {
    background: var(--bg-active);
    color: var(--fg);
  }

  .item.exited .text {
    opacity: 0.6;
  }

  /* 分割した同じグループのターミナルは、左の線でつなぐ */
  .item.grouped {
    position: relative;
    margin-left: 6px;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }

  .item.grouped::before {
    content: '';
    position: absolute;
    left: -5px;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--border);
  }

  .item.group-first::before {
    top: 4px;
  }

  .item.group-last::before {
    bottom: 4px;
  }

  .kind {
    display: flex;
    align-items: center;
    flex: none;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* AI ツールが付けた会話のタイトル */
  .sub {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .state {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .status {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--fg-muted);
    opacity: 0.45;
  }

  .status.busy {
    background: var(--accent);
    opacity: 1;
    animation: pulse 1.1s ease-in-out infinite;
  }

  .status.exited {
    background: transparent;
    border: 1.5px solid var(--fg-muted);
    opacity: 0.6;
  }

  @keyframes pulse {
    50% {
      opacity: 0.25;
      transform: scale(0.75);
    }
  }

  .close {
    flex: none;
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 10px;
    color: var(--fg-muted);
    border-radius: 3px;
    padding: 2px 4px;
    visibility: hidden;
  }

  .item:hover .close,
  .item.active .close {
    visibility: visible;
  }

  .close:hover {
    background: var(--bg-active);
  }

  .open {
    margin: 8px auto;
    display: block;
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
  }

  .usage {
    flex: none;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .usage-header {
    height: 26px;
  }

  .usage-items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 3px 8px 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .usage-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    padding: 2px 2px;
  }

  .usage-block {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-bottom: 3px;
  }

  .usage-block + .usage-block {
    border-top: 1px dashed var(--border);
    padding-top: 5px;
  }

  /* 枠の名前 | バー | 残り%。回復までの時間はバーの下 */
  .limits {
    display: grid;
    grid-template-columns: auto minmax(40px, 1fr) auto;
    align-items: center;
    column-gap: 8px;
    row-gap: 1px;
    padding-left: 19px;
    font-size: 11px;
  }

  .limit-name {
    color: var(--fg-muted);
    white-space: nowrap;
  }

  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--bg-hover);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    border-radius: 4px;
    transition: width 0.3s;
  }

  .fill.ok {
    background: #2da44e;
  }

  .fill.warn {
    background: #d29922;
  }

  .fill.low {
    background: var(--danger);
  }

  .limit-pct {
    font-weight: 600;
    white-space: nowrap;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .limit-pct.warn {
    color: #d29922;
  }

  .limit-pct.low {
    color: var(--danger);
  }

  .limit-reset {
    grid-column: 2 / 4;
    margin-bottom: 3px;
    color: var(--fg-muted);
    font-size: 10.5px;
  }

  .limit-note {
    padding-left: 19px;
    font-size: 10.5px;
    color: var(--fg-muted);
  }

  .usage-tool {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex: none;
    color: var(--fg);
  }

  .usage-icon {
    display: inline-flex;
    align-items: center;
    color: var(--accent);
  }

  .usage-name {
    white-space: nowrap;
  }

  .usage-val {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--editor-mono);
    font-size: 11px;
    color: var(--fg-muted);
    text-align: right;
  }

  .usage-val.error {
    color: #dc2626;
  }

  .usage-val.muted {
    opacity: 0.7;
  }
</style>
