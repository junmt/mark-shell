<script lang="ts">
  import type { GitTab } from '../lib/git.svelte';
  import GitDiffView from './GitDiffView.svelte';
  import GitRevView from './GitRevView.svelte';
  import GitTreeView from './GitTreeView.svelte';
  import GitCompareView from './GitCompareView.svelte';

  // 右ペインの Git のタブ（差分・その時点の内容・ファイルの一覧・比較）
  let { tab }: { tab: GitTab } = $props();
</script>

{#if tab.kind === 'diff'}
  <GitDiffView {tab} />
{:else if tab.kind === 'rev'}
  <GitRevView {tab} />
{:else if tab.kind === 'tree'}
  <GitTreeView {tab} />
{:else}
  <GitCompareView {tab} />
{/if}
