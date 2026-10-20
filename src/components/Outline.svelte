<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { headings } from '../lib/markdown';
  import { tr } from '../lib/i18n.svelte';

  const doc = $derived(store.activeDoc);
  const markdown = $derived(!!doc && store.isMarkdown(doc.path));
  const items = $derived(doc && markdown ? headings(doc.text) : []);
  const minLevel = $derived(Math.min(...items.map((i) => i.level), 6));
</script>

<div class="outline">
  {#if !doc}
    <div class="empty">{tr('ファイルを開くと見出しの一覧が出ます。', 'Open a file to see its headings.', '打开文件后将显示标题列表。')}</div>
  {:else if !markdown}
    <div class="empty">{tr('Markdown ではないファイルには見出しの一覧がありません。', 'Only Markdown files have a heading list.', '非 Markdown 文件没有标题列表。')}</div>
  {:else if items.length === 0}
    <div class="empty">{tr('見出しがありません。', 'No headings.', '没有标题。')}</div>
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
