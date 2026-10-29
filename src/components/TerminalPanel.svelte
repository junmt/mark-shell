<script lang="ts">
  import { store } from '../lib/store.svelte';
  import TerminalView from './TerminalView.svelte';
  import { tr } from '../lib/i18n.svelte';

  // ターミナルの切り替えは左ペインの一覧（TerminalList）で行う
</script>

<div class="panel">
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
