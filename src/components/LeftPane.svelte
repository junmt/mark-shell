<script lang="ts">
  import { store, type LeftView } from '../lib/store.svelte';
  import FileTree from './FileTree.svelte';
  import SearchPanel from './SearchPanel.svelte';
  import Outline from './Outline.svelte';

  const views: { id: LeftView; label: string; title: string }[] = [
    { id: 'files', label: 'ファイル', title: 'ファイル' },
    { id: 'search', label: '検索', title: '全文検索 (Ctrl+Shift+F)' },
    { id: 'outline', label: 'アウトライン', title: '見出しの一覧' },
  ];
</script>

<div class="tabs" role="tablist">
  {#each views as v (v.id)}
    <button role="tab" class:on={store.leftView === v.id} title={v.title} aria-selected={store.leftView === v.id} onclick={() => store.showLeft(v.id)}>
      {v.label}
    </button>
  {/each}
</div>
<div class="body">
  {#if store.leftView === 'files'}
    <FileTree />
  {:else if store.leftView === 'search'}
    <SearchPanel />
  {:else}
    <Outline />
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    flex: none;
    border-bottom: 1px solid var(--border);
    padding: 0 4px;
  }

  .tabs button {
    border: none;
    background: transparent;
    padding: 7px 10px 6px;
    cursor: pointer;
    color: var(--fg-muted);
    border-bottom: 2px solid transparent;
  }

  .tabs button.on {
    color: var(--fg);
    border-bottom-color: var(--accent);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
</style>
