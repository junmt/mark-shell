<script lang="ts">
  import { store, type MenuItem, type TermSession } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  import { tr } from '../lib/i18n.svelte';

  // 左ペインの下側に並べるターミナルの一覧（ターミナル本体は右ペインの下側）

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

  function select(t: TermSession) {
    store.activeTerminal = t.key;
    if (!store.terminalVisible) {
      store.terminalVisible = true;
      store.saveWorkspaceState();
    }
  }
</script>

<div class="list">
  <div class="header">
    <span class="label">{tr('ターミナル', 'Terminals', '终端')}</span>
    <div class="spacer"></div>
    <button class="icon-btn" title={tr('新しいターミナル', 'New terminal', '新建终端')} onclick={() => store.newTerminal()}><Icon name="plus" size={14} /></button>
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
        role="tab"
        tabindex="0"
        aria-selected={store.activeTerminal === t.key}
        title={tooltip(t)}
        onclick={() => select(t)}
        onkeydown={(e) => e.key === 'Enter' && select(t)}
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

<style>
  .list {
    flex: 1;
    min-height: 0;
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
</style>
