<script lang="ts">
  import { untrack } from 'svelte';
  import { git, type GitTab } from '../lib/git.svelte';
  import { gitApi } from '../lib/git';
  import { errorMessage } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import { basename, dirname } from '../lib/paths';

  // ある時点のファイルの一覧（チェックアウトせずに中身を確かめる）
  let { tab: tabProp }: { tab: GitTab } = $props();
  const tab = untrack(() => tabProp);
  const t = tab.tree!;

  let files = $state<string[] | null>(null);
  let error = $state<string | null>(null);
  let filter = $state('');

  gitApi
    .lsTree(t.repo, t.rev)
    .then((f) => (files = f))
    .catch((e) => (error = errorMessage(e)));

  const LIMIT = 3000;
  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const list = (files ?? []).filter((f) => !q || f.toLowerCase().includes(q));
    return { list: list.slice(0, LIMIT), total: list.length };
  });
</script>

<div class="tree">
  <div class="bar">
    <span class="title">{tr(`${t.label} の時点のファイル`, `Files at ${t.label}`, `${t.label} 时的文件`)}</span>
    <input type="search" bind:value={filter} placeholder={tr('パスで絞り込む', 'Filter by path', '按路径筛选')} spellcheck="false" />
    {#if files}<span class="count">{shown.total.toLocaleString()}</span>{/if}
  </div>
  {#if error}
    <div class="msg">{error}</div>
  {:else if !files}
    <div class="msg">{tr('読み込み中…', 'Loading…', '正在加载…')}</div>
  {:else}
    <div class="list">
      {#each shown.list as f (f)}
        <div class="row" role="button" tabindex="0" title={f} onclick={() => git.openRevision(t.rev, f, t.repo)} onkeydown={(e) => e.key === 'Enter' && git.openRevision(t.rev, f, t.repo)}>
          <span class="name">{basename(f)}</span>
          <span class="dir">{dirname(f)}</span>
        </div>
      {/each}
      {#if shown.total > LIMIT}
        <div class="msg">{tr(`ほか ${(shown.total - LIMIT).toLocaleString()} 件。絞り込んでください。`, `${(shown.total - LIMIT).toLocaleString()} more. Narrow the filter.`, `另有 ${(shown.total - LIMIT).toLocaleString()} 项，请缩小筛选范围。`)}</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tree {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .title {
    font-weight: 600;
    white-space: nowrap;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
  }

  .count {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .list {
    flex: 1;
    overflow: auto;
    padding: 4px 0;
  }

  .row {
    display: flex;
    gap: 8px;
    padding: 2px 14px;
    cursor: pointer;
    white-space: nowrap;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .name {
    flex: none;
  }

  .dir {
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .msg {
    padding: 12px 16px;
    color: var(--fg-muted);
  }
</style>
