<script lang="ts">
  import { untrack } from 'svelte';
  import { store, type Doc, type Mode } from '../lib/store.svelte';
  import Editor from './Editor.svelte';
  import Preview from './Preview.svelte';
  import { tr } from '../lib/i18n.svelte';

  let { doc: docProp }: { doc: Doc } = $props();

  // タブごとに作り直すので、作ったときのファイルに固定する。
  // プロパティのままだと、切り替えの途中で新しいタブのファイルを指してしまう
  const doc = untrack(() => docProp);

  const modes: { id: Mode; label: string }[] = $derived([
    { id: 'edit', label: tr('編集', 'Edit', '编辑') },
    { id: 'split', label: tr('分割', 'Split', '分屏') },
    { id: 'preview', label: tr('プレビュー', 'Preview', '预览') },
  ]);

  // Markdown 以外（ログなどのテキストファイル）はプレビューがないので編集だけ
  const markdown = $derived(store.isMarkdown(doc.path));
  const mode = $derived<Mode>(markdown ? doc.mode : 'edit');

  let splitWidth = $state(0.5);
  let body: HTMLDivElement | undefined = $state();

  function setMode(m: Mode) {
    doc.mode = m;
    store.saveWorkspaceState();
  }

  function dragSplit(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      if (!body) return;
      const r = body.getBoundingClientRect();
      splitWidth = Math.min(0.85, Math.max(0.15, (ev.clientX - r.left) / r.width));
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  // 分割表示のスクロール同期で使う
  const sync = { editorToPreview: null as ((line: number) => void) | null, previewToEditor: null as ((line: number) => void) | null };
</script>

<div class="doc">
  <div class="bar">
    {#if markdown}
      <div class="modes" role="radiogroup" aria-label={tr('表示モード', 'View mode', '视图模式')}>
        {#each modes as m (m.id)}
          <button class:on={doc.mode === m.id} role="radio" aria-checked={doc.mode === m.id} onclick={() => setMode(m.id)} title={tr('Ctrl+Shift+V で切り替え', 'Ctrl+Shift+V to switch', '按 Ctrl+Shift+V 切换')}>
            {m.label}
          </button>
        {/each}
      </div>
    {/if}
    <span class="path" title={doc.path}>{doc.path}</span>
    {#if doc.readonly}<span class="tag">{tr('読み取り専用', 'Read-only', '只读')}</span>{/if}
  </div>

  {#if doc.deleted}
    <div class="banner warn">{tr('このファイルは削除されました。保存すると作り直します。', 'This file was deleted. Saving will recreate it.', '此文件已被删除。保存后将重新创建。')}
      <button class="btn" onclick={() => void store.save(doc, true)}>{tr('保存して作り直す', 'Save to recreate', '保存并重新创建')}</button>
      <button class="btn" onclick={() => void store.closeTab(doc.path)}>{tr('タブを閉じる', 'Close tab', '关闭标签页')}</button>
    </div>
  {/if}
  {#if store.aiBefore.has(doc.path)}
    <div class="banner ai">✦ {tr('AI セッション中にこのファイルが変更されました。', 'This file was changed during an AI session.', '此文件在 AI 会话中被修改。')}
      <button class="btn" onclick={() => store.showAiDiff(doc)}>{tr('差分を見る', 'View diff', '查看差异')}</button>
      <button class="btn" onclick={() => void store.revertAiChange(doc)}>{tr('変更前に戻す', 'Revert changes', '还原修改')}</button>
      <button class="btn" onclick={() => store.dismissAiChange(doc.path)}>{tr('閉じる', 'Dismiss', '关闭')}</button>
    </div>
  {/if}

  <div class="body" bind:this={body}>
    {#if mode !== 'preview'}
      <div class="pane" style:flex={mode === 'split' ? `0 0 ${splitWidth * 100}%` : '1'}>
        <Editor {doc} onscrollline={(l) => mode === 'split' && sync.editorToPreview?.(l)} bind:scrollToLine={sync.previewToEditor} />
      </div>
    {/if}
    {#if mode === 'split'}
      <div class="split" role="separator" aria-orientation="vertical" onpointerdown={dragSplit}></div>
    {/if}
    {#if mode !== 'edit'}
      <div class="pane preview">
        <Preview {doc} onscrollline={(l) => mode === 'split' && sync.previewToEditor?.(l)} bind:scrollToLine={sync.editorToPreview} />
      </div>
    {/if}
  </div>
</div>

<style>
  .doc {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .modes {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
  }

  .modes button {
    border: none;
    background: transparent;
    padding: 2px 10px;
    cursor: pointer;
    color: var(--fg-muted);
  }

  .modes button + button {
    border-left: 1px solid var(--border);
  }

  .modes button.on {
    background: var(--bg-active);
    color: var(--fg);
  }

  .path {
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

  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    font-size: 12px;
    flex: none;
  }

  .banner .btn {
    padding: 2px 10px;
    font-size: 12px;
  }

  .banner.warn {
    background: color-mix(in srgb, #d97706 15%, var(--bg));
  }

  .banner.ai {
    background: color-mix(in srgb, var(--accent) 12%, var(--bg));
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .pane {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .split {
    width: 5px;
    margin: 0 -2px;
    cursor: col-resize;
    background: linear-gradient(to right, transparent 2px, var(--border) 2px, var(--border) 3px, transparent 3px);
    z-index: 2;
    flex: none;
  }

  .split:hover {
    background: linear-gradient(to right, transparent 2px, var(--accent) 2px, var(--accent) 3px, transparent 3px);
  }
</style>
