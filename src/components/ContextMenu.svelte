<script lang="ts">
  import { store, type MenuItem } from '../lib/store.svelte';

  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ x: 0, y: 0 });

  // 画面からはみ出さない位置に出す
  $effect(() => {
    const m = store.menu;
    if (!m || !el) return;
    const r = el.getBoundingClientRect();
    pos = {
      x: Math.min(m.x, window.innerWidth - r.width - 4),
      y: m.y + r.height > window.innerHeight ? Math.max(4, m.y - r.height) : m.y,
    };
  });

  function run(item: MenuItem) {
    if (item.disabled || item.separator) return;
    store.menu = null;
    item.action?.();
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && store.menu && (store.menu = null)} onblur={() => (store.menu = null)} />

{#if store.menu}
  <div class="context-menu" role="menu" bind:this={el} style:left="{pos.x}px" style:top="{pos.y}px">
    {#each store.menu.items as item, i (i)}
      {#if item.separator}
        <div class="sep"></div>
      {:else}
        <button class="item" role="menuitem" disabled={item.disabled} onclick={() => run(item)}>
          <span class="label">{item.label}</span>
          {#if item.shortcut}<span class="shortcut">{item.shortcut}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .context-menu {
    position: fixed;
    z-index: 90;
    min-width: 200px;
    max-width: 480px;
    max-height: 80vh;
    overflow: auto;
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.2);
  }

  .item {
    display: flex;
    width: 100%;
    gap: 24px;
    border: none;
    background: transparent;
    text-align: left;
    padding: 5px 10px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
  }

  .item:hover:not(:disabled) {
    background: var(--bg-active);
  }

  .item:disabled {
    color: var(--fg-muted);
    cursor: default;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shortcut {
    margin-left: auto;
    color: var(--fg-muted);
    font-size: 12px;
  }

  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
