<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { store } from '../lib/store.svelte';
  import type { SearchHit } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  let input: HTMLInputElement | undefined = $state();
  let replacement = $state('');
  let showReplace = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  onMount(() => {
    input?.focus();
    input?.select();
  });

  function schedule() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void store.runSearch(), 200);
  }

  function toggle(key: 'caseSensitive' | 'wholeWord' | 'regex') {
    store.searchOptions[key] = !store.searchOptions[key];
    void store.runSearch();
  }

  const groups = $derived.by(() => {
    const map = new Map<string, SearchHit[]>();
    for (const h of store.searchHits) {
      const list = map.get(h.path);
      if (list) list.push(h);
      else map.set(h.path, [h]);
    }
    return [...map].sort((a, b) => a[0].localeCompare(b[0], 'ja'));
  });

  const collapsed = new SvelteSet<string>();

  function parts(hit: SearchHit): { text: string; match: boolean }[] {
    const out: { text: string; match: boolean }[] = [];
    let last = 0;
    for (const [s, e] of hit.ranges) {
      if (s > last) out.push({ text: hit.text.slice(last, s), match: false });
      out.push({ text: hit.text.slice(s, e), match: true });
      last = e;
    }
    out.push({ text: hit.text.slice(last), match: false });
    return out;
  }
</script>

<div class="form">
  <div class="row">
    <button class="icon-btn" title={tr('置換', 'Replace', '替换')} onclick={() => (showReplace = !showReplace)}>{showReplace ? '▾' : '▸'}</button>
    <input
      bind:this={input}
      bind:value={store.searchOptions.query}
      oninput={schedule}
      onkeydown={(e) => e.key === 'Enter' && void store.runSearch()}
      placeholder={tr('検索', 'Search', '搜索')}
      spellcheck="false"
    />
  </div>
  <div class="opts">
    <button class="icon-btn" class:on={store.searchOptions.caseSensitive} title={tr('大文字と小文字を区別', 'Match case', '区分大小写')} onclick={() => toggle('caseSensitive')}>Aa</button>
    <button class="icon-btn" class:on={store.searchOptions.wholeWord} title={tr('単語単位', 'Match whole word', '全字匹配')} onclick={() => toggle('wholeWord')}>ab|</button>
    <button class="icon-btn" class:on={store.searchOptions.regex} title={tr('正規表現', 'Use regular expression', '使用正则表达式')} onclick={() => toggle('regex')}>.*</button>
    <input class="folder" bind:value={store.searchOptions.folder} oninput={schedule} placeholder={tr('フォルダで絞り込み', 'Filter by folder', '按文件夹筛选')} spellcheck="false" />
  </div>
  {#if showReplace}
    <div class="row">
      <input bind:value={replacement} placeholder={tr('置換後の文字列', 'Replace with', '替换为')} spellcheck="false" />
      <button class="btn" disabled={store.searchHits.length === 0} onclick={() => void store.replaceAll(replacement)}>{tr('すべて置換', 'Replace all', '全部替换')}</button>
    </div>
  {/if}
  <div class="summary">
    {#if store.searchError}
      <span class="error">{store.searchError}</span>
    {:else if store.searchRunning}
      {tr(`検索中… ${store.searchHits.length} 件`, `Searching… ${store.searchHits.length} results`, `正在搜索… ${store.searchHits.length} 个结果`)}
    {:else if store.searchOptions.query}
      {tr(`${groups.length} 個のファイルで ${store.searchHits.length} 件`, `${store.searchHits.length} results in ${groups.length} files`, `${groups.length} 个文件中共 ${store.searchHits.length} 个结果`)}{store.searchTruncated
        ? tr('（多すぎるため途中まで）', ' (too many; showing partial results)', '（结果过多，仅显示部分）')
        : ''}
    {/if}
  </div>
</div>

<div class="results">
  {#each groups as [path, hits] (path)}
    <button class="file" onclick={() => (collapsed.has(path) ? collapsed.delete(path) : collapsed.add(path))}>
      <span class="twisty"><Icon name={collapsed.has(path) ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} /></span>
      <span class="ficon"><Icon name="fileText" size={14} /></span>
      <span class="fname">{path}</span>
      <span class="count">{hits.length}</span>
    </button>
    {#if !collapsed.has(path)}
      {#each hits as hit (hit.line)}
        <button class="hit" title="{path}:{hit.line}" onclick={() => void store.openFile(path, { pin: true, line: hit.line })}>
          <span class="ln">{hit.line}</span>
          <span class="text">{#each parts(hit) as p}{#if p.match}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}</span>
        </button>
      {/each}
    {/if}
  {/each}
</div>

<style>
  .form {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: none;
    border-bottom: 1px solid var(--border);
  }

  .row,
  .opts {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .row input {
    flex: 1;
    min-width: 0;
  }

  .opts .folder {
    flex: 1;
    min-width: 0;
    font-size: 12px;
  }

  .summary {
    font-size: 12px;
    color: var(--fg-muted);
    min-height: 16px;
  }

  .error {
    color: var(--danger);
  }

  .results {
    flex: 1;
    overflow: auto;
  }

  .file,
  .hit {
    display: flex;
    width: 100%;
    gap: 6px;
    border: none;
    background: transparent;
    text-align: left;
    cursor: pointer;
    padding: 3px 8px;
    align-items: baseline;
  }

  .file:hover,
  .hit:hover {
    background: var(--bg-hover);
  }

  .file {
    font-weight: 600;
  }

  .twisty,
  .ficon {
    display: flex;
    align-items: center;
    flex: none;
    color: var(--fg-muted);
  }

  .fname {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  .count {
    font-size: 11px;
    color: var(--fg-muted);
    background: var(--bg-hover);
    padding: 0 6px;
    border-radius: 8px;
  }

  .hit {
    padding-left: 24px;
  }

  .ln {
    font-size: 11px;
    color: var(--fg-muted);
    min-width: 2.5em;
    text-align: right;
    flex: none;
  }

  .text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  mark {
    background: var(--search-match);
    color: inherit;
    border-radius: 2px;
  }
</style>
