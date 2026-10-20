<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { headings } from '../lib/markdown';

  const doc = $derived(store.activeDoc);
  const items = $derived(doc ? headings(doc.text) : []);
  const minLevel = $derived(Math.min(...items.map((i) => i.level), 6));
</script>

<div class="outline">
  {#if !doc}
    <div class="empty">ファイルを開くと見出しの一覧が出ます。</div>
  {:else if items.length === 0}
    <div class="empty">見出しがありません。</div>
  {:else}
    {#each items as h (h.line)}
      <button
        class="item"
        class:current={store.cursor.line >= h.line && !items.some((x) => x.line > h.line && x.line <= store.cursor.line)}
        style:padding-left="{10 + (h.level - minLevel) * 14}px"
        onclick={() => doc && void store.openFile(doc.path, { line: h.line })}
      >
        {h.text}
      </button>
    {/each}
  {/if}
</div>

<style>
  .outline {
    flex: 1;
    overflow: auto;
    padding: 4px 0;
  }

  .item {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    padding: 4px 10px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item:hover {
    background: var(--bg-hover);
  }

  .item.current {
    color: var(--accent);
    font-weight: 600;
  }

  .empty {
    padding: 16px;
    color: var(--fg-muted);
  }
</style>
