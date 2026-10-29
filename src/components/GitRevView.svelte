<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import { git, type GitTab } from '../lib/git.svelte';
  import { gitApi, repoToRoot, shortHash, type FileSide } from '../lib/git';
  import { store } from '../lib/store.svelte';
  import { errorMessage } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import { loadLanguageFor, viewerExtensions } from '../lib/editor';
  import Icon from './Icon.svelte';

  // ある時点のファイルの内容（読み取り専用）
  let { tab: tabProp }: { tab: GitTab } = $props();
  const tab = untrack(() => tabProp);
  const r = tab.rev!;

  let side = $state<FileSide | null>(null);
  let error = $state<string | null>(null);
  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | null = null;

  const repoInfo = $derived(git.info?.repos.find((x) => x.path === r.repo) ?? null);
  const rel = $derived(repoInfo ? repoToRoot(repoInfo, r.path) : null);
  const revLabel = r.rev.length === 40 ? shortHash(r.rev) : r.rev;

  gitApi
    .showFile(r.repo, r.rev, r.path)
    .then((s) => (side = s))
    .catch((e) => (error = errorMessage(e)));

  $effect(() => {
    if (!host || !side || side.binary || !side.exists) return;
    const text = side.text ?? '';
    untrack(() => {
      view?.destroy();
      view = new EditorView({
        parent: host!,
        state: EditorState.create({
          doc: text,
          extensions: viewerExtensions({
            markdown: store.isMarkdown(r.path),
            wrap: store.config.lineWrapping,
            readOnly: true,
            fontSize: store.config.editorFontSize,
            fontFamily: store.config.editorFontFamily,
          }),
        }),
      });
      if (!store.isMarkdown(r.path)) loadLanguageFor(view, r.path);
    });
  });

  onDestroy(() => view?.destroy());

  function compareWithWorking() {
    git.openDiff({
      repo: r.repo,
      path: r.path,
      orig: null,
      left: { rev: r.rev, label: revLabel },
      right: { rev: null, label: tr('作業ツリー', 'Working Tree', '工作区') },
      stage: 'none',
    });
  }
</script>

<div class="rev">
  <div class="bar">
    <span class="path" title={r.path}>{r.path} @ {revLabel}</span>
    <span class="tag">{tr('読み取り専用', 'Read-only', '只读')}</span>
    {#if rel !== null}
      <button class="btn small" onclick={compareWithWorking}>{tr('作業ツリーと比較', 'Compare with working tree', '与工作区比较')}</button>
      <button class="icon-btn" title={tr('現在のファイルを開く', 'Open current file', '打开当前文件')} onclick={() => rel && void store.openFile(rel, { pin: true })}><Icon name="openFile" size={14} /></button>
    {/if}
  </div>
  {#if error}
    <div class="msg">{error}</div>
  {:else if !side}
    <div class="msg">{tr('読み込み中…', 'Loading…', '正在加载…')}</div>
  {:else if !side.exists}
    <div class="msg">{tr('この時点にはファイルがありません。', 'The file does not exist at this point.', '此时该文件不存在。')}</div>
  {:else if side.image}
    <div class="image"><img src={side.image} alt={r.path} /></div>
  {:else if side.binary}
    <div class="msg">{tr('バイナリファイルです。', 'This is a binary file.', '这是二进制文件。')}</div>
  {:else}
    <div class="host" bind:this={host}></div>
  {/if}
</div>

<style>
  .rev {
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

  .path {
    flex: 1;
    min-width: 0;
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    font-size: 11px;
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0 5px;
    color: var(--fg-muted);
  }

  .btn.small {
    padding: 1px 8px;
    font-size: 12px;
  }

  .icon-btn {
    border: none;
    background: transparent;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--fg-muted);
    padding: 0;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
  }

  .host {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--bg);
    user-select: text;
  }

  .host :global(.cm-editor) {
    height: 100%;
  }

  .msg {
    padding: 16px;
    color: var(--fg-muted);
  }

  .image {
    padding: 16px;
    overflow: auto;
  }

  .image img {
    max-width: 100%;
  }
</style>
