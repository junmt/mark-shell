<script lang="ts">
  import { store, type MenuItem, type TermSession } from '../lib/store.svelte';
  import TerminalView from './TerminalView.svelte';
  import Icon from './Icon.svelte';
  import { tr } from '../lib/i18n.svelte';

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

  /** タブのツールチップ: 名前、AI が付けたタイトル、状態 */
  function tooltip(t: TermSession): string {
    return [t.title, t.osTitle, status(t)].filter(Boolean).join(' — ');
  }
</script>

<div class="panel">
  <div class="tabs">
    {#each store.terminals as t (t.key)}
      <div
        class="tab"
        class:active={store.activeTerminal === t.key}
        class:exited={t.exited}
        role="tab"
        tabindex="0"
        aria-selected={store.activeTerminal === t.key}
        title={tooltip(t)}
        onclick={() => (store.activeTerminal = t.key)}
        onkeydown={(e) => e.key === 'Enter' && (store.activeTerminal = t.key)}
      >
        <!-- 状態: 処理中は点滅、待機中は小さな点、終了は輪 -->
        <span class="status" class:busy={t.busy} class:exited={t.exited} aria-label={status(t)}></span>
        <span class="kind"><Icon name={t.ai ? 'sparkle' : 'terminal'} size={13} /></span>
        <span class="title">{t.title}</span>
        {#if t.ai && t.osTitle}<span class="sub">{t.osTitle}</span>{/if}
        {#if t.exited}<span class="state">{tr('（終了）', '(exited)', '（已退出）')}</span>{/if}
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
    <button class="icon-btn" title={tr('新しいターミナル', 'New terminal', '新建终端')} onclick={() => store.newTerminal()}><Icon name="plus" size={14} /></button>
    <button class="icon-btn" title={tr('シェルや AI を選んで開く', 'Choose a shell or AI tool to open', '选择要打开的 Shell 或 AI')} onclick={newMenu}>
      <Icon name="chevronDown" size={12} stroke={2} />
    </button>
    <div class="spacer"></div>
    <button class="icon-btn" title={tr('ターミナルを隠す (Ctrl+`)', 'Hide terminal (Ctrl+`)', '隐藏终端 (Ctrl+`)')} onclick={() => store.toggleTerminal()}>
      <Icon name="chevronDown" size={14} stroke={2} />
    </button>
  </div>
  <div class="views">
    {#each store.terminals as t (t.key)}
      <div class="view" class:hidden={store.activeTerminal !== t.key}>
        <TerminalView session={t} active={store.activeTerminal === t.key && store.terminalVisible} />
      </div>
    {/each}
    {#if store.terminals.length === 0}
      <div class="empty">
        <button class="btn" onclick={() => store.newTerminal()}>{tr('ターミナルを開く', 'Open terminal', '打开终端')}</button>
      </div>
    {/if}
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

  .tabs {
    display: flex;
    align-items: center;
    flex: none;
    height: 30px;
    padding: 0 4px;
    gap: 2px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px 0 10px;
    height: 24px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
    color: var(--fg-muted);
  }

  .tab.active {
    background: var(--bg-active);
    color: var(--fg);
  }

  .tab.exited .title,
  .tab.exited .sub {
    opacity: 0.6;
  }

  .tab {
    max-width: 420px;
  }

  .kind {
    display: flex;
    align-items: center;
    flex: none;
  }

  .title {
    flex: none;
  }

  /* AI ツールが付けた会話のタイトル */
  .sub {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .sub::before {
    content: '·';
    margin-right: 4px;
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

  .icon-btn {
    display: inline-flex;
    align-items: center;
  }

  .close {
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 10px;
    color: var(--fg-muted);
    border-radius: 3px;
    padding: 2px 4px;
  }

  .close:hover {
    background: var(--bg-hover);
  }

  .spacer {
    flex: 1;
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
    inset: 0;
  }

  .view.hidden {
    visibility: hidden;
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
</style>
