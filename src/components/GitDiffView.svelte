<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { EditorState, RangeSetBuilder, type Extension } from '@codemirror/state';
  import { EditorView, GutterMarker, gutter } from '@codemirror/view';
  import { MergeView, getChunks, getOriginalDoc, goToNextChunk, goToPreviousChunk, unifiedMergeView, type Chunk } from '@codemirror/merge';
  import { diffArrays } from 'diff';
  import { git, type GitTab } from '../lib/git.svelte';
  import { applyChunk, applyLines, gitApi, repoToRoot, revertChunk, type FileSide } from '../lib/git';
  import { store } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import { loadLanguageFor, viewerExtensions } from '../lib/editor';
  import { renderMarkdown } from '../lib/markdown';
  import { enhance } from '../lib/enhance';
  import Icon from './Icon.svelte';

  let { tab: tabProp }: { tab: GitTab } = $props();
  // タブごとに作り直すので、作ったときのタブに固定する
  const tab = untrack(() => tabProp);
  const spec = tab.diff!;

  let a = $state<FileSide | null>(null);
  let b = $state<FileSide | null>(null);
  let error = $state<string | null>(null);
  let mode = $state<'split' | 'inline' | 'preview'>(store.config.gitDiffMode === 'inline' ? 'inline' : 'split');
  let host: HTMLDivElement | undefined = $state();
  let previewA: HTMLDivElement | undefined = $state();
  let previewB: HTMLDivElement | undefined = $state();
  let merge: MergeView | null = null;
  let single: EditorView | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let built = $state(0);

  const repoInfo = $derived(git.info?.repos.find((r) => r.path === spec.repo) ?? null);
  /** ルートからの相対パス（ルートの外なら null） */
  const rel = $derived(repoInfo ? repoToRoot(repoInfo, spec.path) : null);
  const markdown = $derived(store.isMarkdown(spec.path));
  const textual = $derived(!!a && !!b && !a.binary && !b.binary);
  /** 右側（作業ツリー）を編集できるか */
  const editable = $derived(spec.stage === 'worktree' && rel !== null && !!b?.exists && !b.binary);
  /** まとまりごとのボタンを出すか */
  const chunkActions = $derived(spec.stage !== 'none' && !!a?.exists && !!b?.exists && textual);

  async function side(rev: string | null, path: string): Promise<FileSide> {
    if (rev === '__none__') return { exists: false, binary: false, size: 0, text: '', encoding: '', eol: '', image: null };
    return gitApi.showFile(spec.repo, rev, path);
  }

  async function load() {
    try {
      const [na, nb] = await Promise.all([side(spec.left.rev, spec.orig ?? spec.path), side(spec.right.rev, spec.path)]);
      error = null;
      // 編集中（保存待ち）の右側は読み直さない
      if (saveTimer && a && na.text === a.text) return;
      const same = a && b && na.text === a.text && nb.text === b.text && na.exists === a.exists && nb.exists === b.exists;
      a = na;
      b = nb;
      if (!same) built++;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // ステージや外部の変更で読み直す
  $effect(() => {
    void tab.version;
    untrack(() => void load());
  });

  function destroyViews() {
    merge?.destroy();
    single?.destroy();
    merge = null;
    single = null;
  }

  onDestroy(() => {
    destroyViews();
    if (saveTimer) {
      clearTimeout(saveTimer);
      flushSave();
    }
  });

  /** 右側（作業ツリー）の編集を保存する。開いているタブがあればそちらに反映して保存する */
  let pendingText: string | null = null;
  function scheduleSave(text: string) {
    pendingText = text;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(flushSave, 700);
  }

  function flushSave() {
    saveTimer = null;
    const text = pendingText;
    pendingText = null;
    if (text === null || rel === null || !b) return;
    b.text = text;
    const doc = store.docs.find((d) => d.path.toLowerCase() === rel.toLowerCase());
    if (doc) {
      store.setDocText(doc, text);
      void store.save(doc);
    } else {
      void api.writeFile(rel, text, b.encoding || 'utf-8', b.eol || 'lf').catch((e) => store.toast(errorMessage(e), 'error'));
    }
  }

  // ---- まとまりごとのボタン ----

  function texts(): { aText: string; bText: string; bView: EditorView } | null {
    if (merge) return { aText: merge.a.state.doc.toString(), bText: merge.b.state.doc.toString(), bView: merge.b };
    if (single) return { aText: getOriginalDoc(single.state).toString(), bText: single.state.doc.toString(), bView: single };
    return null;
  }

  async function setIndex(text: string) {
    if (!b || !a) return;
    const r = await git.setIndex(spec.path, text, (spec.stage === 'worktree' ? a.encoding || b.encoding : b.encoding) || 'utf-8', (a.eol || b.eol) || 'lf');
    if (r?.ok) tab.version++;
  }

  function onChunk(chunk: Chunk, action: 'stage' | 'discard' | 'unstage') {
    const t = texts();
    if (!t) return;
    if (action === 'stage') void setIndex(applyChunk(t.aText, t.bText, chunk));
    else if (action === 'unstage') void setIndex(revertChunk(t.aText, t.bText, chunk));
    else {
      const next = revertChunk(t.aText, t.bText, chunk);
      t.bView.dispatch({ changes: { from: 0, to: t.bView.state.doc.length, insert: next }, userEvent: 'input' });
    }
  }

  function stageSelection() {
    const t = texts();
    if (!t) return;
    const chunks = getChunks(t.bView.state)?.chunks ?? [];
    const sel = t.bView.state.selection.main;
    void setIndex(applyLines(t.aText, t.bText, chunks, sel.from, sel.to));
  }

  class ActionMarker extends GutterMarker {
    chunk: Chunk | null;
    constructor(chunk: Chunk | null) {
      super();
      this.chunk = chunk;
    }
    eq(o: ActionMarker) {
      return !!o.chunk && !!this.chunk && o.chunk.fromA === this.chunk.fromA && o.chunk.toA === this.chunk.toA && o.chunk.fromB === this.chunk.fromB && o.chunk.toB === this.chunk.toB;
    }
    toDOM() {
      const box = document.createElement('div');
      box.className = 'cm-chunk-acts';
      const add = (label: string, title: string, action: 'stage' | 'discard' | 'unstage') => {
        const btn = document.createElement('button');
        btn.textContent = label;
        btn.title = title;
        btn.onmousedown = (e) => e.preventDefault();
        btn.onclick = (e) => {
          e.stopPropagation();
          if (this.chunk) onChunk(this.chunk, action);
        };
        box.append(btn);
      };
      if (spec.stage === 'worktree') {
        add('+', tr('この変更をステージ', 'Stage this change', '暂存此更改'), 'stage');
        if (editable) add('↶', tr('この変更を破棄', 'Discard this change', '放弃此更改'), 'discard');
      } else if (spec.stage === 'index') {
        add('−', tr('この変更のステージを外す', 'Unstage this change', '取消暂存此更改'), 'unstage');
      }
      return box;
    }
  }

  function actionGutter(): Extension {
    return gutter({
      class: 'cm-chunk-gutter',
      markers(view) {
        const builder = new RangeSetBuilder<GutterMarker>();
        const c = getChunks(view.state);
        if (!c) return builder.finish();
        let last = -1;
        for (const ch of c.chunks) {
          const pos = view.state.doc.lineAt(Math.min(ch.fromB, view.state.doc.length)).from;
          if (pos <= last) continue;
          builder.add(pos, pos, new ActionMarker(ch));
          last = pos;
        }
        return builder.finish();
      },
      initialSpacer: () => new ActionMarker(null),
    });
  }

  // ---- 表示を作る ----

  function common(readOnly: boolean): Extension[] {
    return viewerExtensions({
      markdown,
      wrap: store.config.lineWrapping,
      readOnly,
      fontSize: store.config.editorFontSize,
      fontFamily: store.config.editorFontFamily,
    });
  }

  function build() {
    destroyViews();
    if (!host || !a || !b || !textual || mode === 'preview') return;
    const aText = a.text ?? '';
    const bText = b.text ?? '';
    const onEdit = EditorView.updateListener.of((u) => {
      if (u.docChanged && editable && u.transactions.some((t) => t.isUserEvent('input') || t.isUserEvent('delete') || t.isUserEvent('undo') || t.isUserEvent('redo') || t.isUserEvent('move'))) {
        scheduleSave(u.state.doc.toString());
      }
    });
    const bExt = [common(!editable), onEdit, chunkActions ? actionGutter() : []];
    const collapse = { margin: 3, minSize: 6 };
    if (mode === 'split') {
      merge = new MergeView({
        a: { doc: aText, extensions: common(true) },
        b: { doc: bText, extensions: bExt },
        parent: host,
        collapseUnchanged: collapse,
        gutter: true,
        highlightChanges: true,
      });
      if (!markdown) {
        loadLanguageFor(merge.a, spec.path);
        loadLanguageFor(merge.b, spec.path);
      }
    } else {
      single = new EditorView({
        parent: host,
        state: EditorState.create({
          doc: bText,
          extensions: [unifiedMergeView({ original: aText, mergeControls: false, gutter: true, collapseUnchanged: collapse, highlightChanges: true }), ...bExt],
        }),
      });
      if (!markdown) loadLanguageFor(single, spec.path);
    }
  }

  $effect(() => {
    void built;
    void mode;
    void host;
    void store.config.editorFontSize;
    untrack(build);
  });

  // ---- Markdown のプレビューで比べる ----

  /** 描画結果の段落（上の階層の要素）ごとに比べ、変わったものに印を付ける */
  function renderPair() {
    if (!previewA || !previewB || !a || !b) return;
    const ctx = { root: store.root ?? '', notePath: rel ?? spec.path, externalImages: store.config.externalImages };
    const ta = document.createElement('template');
    const tb = document.createElement('template');
    ta.innerHTML = renderMarkdown(a.text ?? '', ctx);
    tb.innerHTML = renderMarkdown(b.text ?? '', ctx);
    const ea = Array.from(ta.content.children) as HTMLElement[];
    const eb = Array.from(tb.content.children) as HTMLElement[];
    const key = (el: HTMLElement) => {
      const c = el.cloneNode(true) as HTMLElement;
      c.removeAttribute('data-line');
      c.querySelectorAll('[data-line]').forEach((x) => x.removeAttribute('data-line'));
      return c.outerHTML;
    };
    let ia = 0;
    let ib = 0;
    for (const part of diffArrays(ea.map(key), eb.map(key))) {
      const n = part.count ?? part.value.length;
      if (part.removed) for (let k = 0; k < n; k++) ea[ia++]?.classList.add('git-del');
      else if (part.added) for (let k = 0; k < n; k++) eb[ib++]?.classList.add('git-add');
      else {
        ia += n;
        ib += n;
      }
    }
    previewA.replaceChildren(ta.content);
    previewB.replaceChildren(tb.content);
    void enhance(previewA, store.dark, () => true);
    void enhance(previewB, store.dark, () => true);
  }

  $effect(() => {
    void built;
    if (mode !== 'preview' || !previewA || !previewB) return;
    untrack(renderPair);
  });

  function keydown(e: KeyboardEvent) {
    if (e.key !== 'F5' || !e.altKey) return;
    e.preventDefault();
    const view = merge?.b ?? single;
    if (!view) return;
    (e.shiftKey ? goToPreviousChunk : goToNextChunk)({ state: view.state, dispatch: view.dispatch });
  }

  function jump(dir: 1 | -1) {
    const view = merge?.b ?? single;
    if (!view) return;
    (dir < 0 ? goToPreviousChunk : goToNextChunk)({ state: view.state, dispatch: view.dispatch });
    view.focus();
  }

  function setMode(m: 'split' | 'inline' | 'preview') {
    if (saveTimer) {
      clearTimeout(saveTimer);
      flushSave();
    }
    mode = m;
    if (m !== 'preview' && store.config.gitDiffMode !== m) void store.updateConfig({ gitDiffMode: m });
  }

  function size(n: number) {
    return n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="diff" onkeydown={keydown}>
  <div class="bar">
    <div class="modes" role="radiogroup" aria-label={tr('差分の表示', 'Diff view', '差异视图')}>
      <button class:on={mode === 'split'} role="radio" aria-checked={mode === 'split'} onclick={() => setMode('split')}>{tr('左右', 'Side by side', '并排')}</button>
      <button class:on={mode === 'inline'} role="radio" aria-checked={mode === 'inline'} onclick={() => setMode('inline')}>{tr('行内', 'Inline', '内联')}</button>
      {#if markdown}
        <button class:on={mode === 'preview'} role="radio" aria-checked={mode === 'preview'} onclick={() => setMode('preview')}>{tr('プレビュー', 'Preview', '预览')}</button>
      {/if}
    </div>
    {#if mode !== 'preview' && textual}
      <button class="icon-btn" title={tr('前の変更 (Alt+Shift+F5)', 'Previous change (Alt+Shift+F5)', '上一处更改 (Alt+Shift+F5)')} onclick={() => jump(-1)}><Icon name="arrowUp" size={14} /></button>
      <button class="icon-btn" title={tr('次の変更 (Alt+F5)', 'Next change (Alt+F5)', '下一处更改 (Alt+F5)')} onclick={() => jump(1)}><Icon name="arrowDown" size={14} /></button>
    {/if}
    <span class="path" title={spec.orig ? `${spec.orig} → ${spec.path}` : spec.path}>
      {#if spec.orig && spec.orig !== spec.path}{spec.orig} → {/if}{spec.path}
    </span>
    {#if chunkActions && spec.stage === 'worktree' && mode !== 'preview'}
      <button class="btn small" onclick={stageSelection} title={tr('選んだ行の変更だけをステージします', 'Stage only the changes on the selected lines', '仅暂存所选行的更改')}>{tr('選択行をステージ', 'Stage selected lines', '暂存所选行')}</button>
    {/if}
    {#if rel !== null && b?.exists !== false && spec.right.rev === null}
      <button class="icon-btn" title={tr('ファイルを開く', 'Open file', '打开文件')} onclick={() => rel && void store.openFile(rel, { pin: true })}><Icon name="openFile" size={14} /></button>
    {/if}
  </div>
  {#if mode === 'split' || mode === 'preview'}
    <div class="labels">
      <span>− {spec.left.label}{a && !a.exists ? tr('（なし）', ' (none)', '（无）') : ''}</span>
      <span>＋ {spec.right.label}{b && !b.exists ? tr('（なし）', ' (none)', '（无）') : ''}</span>
    </div>
  {/if}
  {#if error}
    <div class="msg error">{error}</div>
  {:else if !a || !b}
    <div class="msg">{tr('読み込み中…', 'Loading…', '正在加载…')}</div>
  {:else if !textual}
    <div class="binary">
      {#if a.image || b.image}
        <div class="images">
          <figure>
            {#if a.image}<img src={a.image} alt={spec.left.label} />{:else}<div class="none">{tr('なし', 'None', '无')}</div>{/if}
            <figcaption>{spec.left.label}{a.exists ? ` · ${size(a.size)}` : ''}</figcaption>
          </figure>
          <figure>
            {#if b.image}<img src={b.image} alt={spec.right.label} />{:else}<div class="none">{tr('なし', 'None', '无')}</div>{/if}
            <figcaption>{spec.right.label}{b.exists ? ` · ${size(b.size)}` : ''}</figcaption>
          </figure>
        </div>
      {:else}
        <p>{tr('バイナリファイルが変更されました。', 'Binary file changed.', '二进制文件已更改。')}</p>
        <p class="muted">{a.exists ? size(a.size) : tr('なし', 'none', '无')} → {b.exists ? size(b.size) : tr('なし', 'none', '无')}</p>
      {/if}
    </div>
  {:else if mode === 'preview'}
    <div class="previews">
      <div class="scroller"><div class="markdown-body" style:font-size="{store.config.previewFontSize}px" bind:this={previewA}></div></div>
      <div class="scroller"><div class="markdown-body" style:font-size="{store.config.previewFontSize}px" bind:this={previewB}></div></div>
    </div>
  {:else}
    <div class="host" bind:this={host}></div>
  {/if}
</div>

<style>
  .diff {
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

  .modes {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 5px;
    overflow: hidden;
    flex: none;
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
    flex: 1;
    min-width: 0;
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn.small {
    padding: 1px 8px;
    font-size: 12px;
    flex: none;
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
    flex: none;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .labels {
    display: flex;
    flex: none;
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }

  .labels span {
    flex: 1;
    padding: 2px 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .labels span:first-child {
    background: color-mix(in srgb, #cf222e 8%, transparent);
  }

  .labels span:last-child {
    background: color-mix(in srgb, #2da44e 8%, transparent);
  }

  .msg {
    padding: 16px;
    color: var(--fg-muted);
  }

  .msg.error {
    color: #cf222e;
    white-space: pre-wrap;
  }

  .host {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg);
    user-select: text;
  }

  .host :global(.cm-mergeView) {
    min-height: 100%;
  }

  .host :global(.cm-editor) {
    min-height: 100%;
  }

  .host :global(.cm-editor.cm-focused) {
    outline: none;
  }

  .host :global(.cm-chunk-gutter) {
    min-width: 34px;
  }

  .host :global(.cm-chunk-acts) {
    display: flex;
    gap: 2px;
    padding: 0 2px;
  }

  .host :global(.cm-chunk-acts button) {
    border: 1px solid var(--border);
    background: var(--bg-alt);
    color: var(--fg);
    border-radius: 3px;
    width: 16px;
    height: 16px;
    line-height: 13px;
    padding: 0;
    font-size: 12px;
    cursor: pointer;
  }

  .host :global(.cm-chunk-acts button:hover) {
    border-color: var(--accent);
    color: var(--accent);
  }

  .host :global(.cm-changedLine) {
    background: color-mix(in srgb, #2da44e 10%, transparent) !important;
  }

  .host :global(.cm-merge-a .cm-changedLine),
  .host :global(.cm-deletedChunk) {
    background: color-mix(in srgb, #cf222e 10%, transparent) !important;
  }

  .host :global(.cm-changedText) {
    background: color-mix(in srgb, #2da44e 30%, transparent) !important;
  }

  .host :global(.cm-merge-a .cm-changedText),
  .host :global(.cm-deletedChunk .cm-deletedText) {
    background: color-mix(in srgb, #cf222e 30%, transparent) !important;
  }

  .host :global(.cm-collapsedLines) {
    background: var(--bg-alt);
    color: var(--fg-muted);
    font-size: 12px;
  }

  .binary {
    padding: 16px 20px;
    overflow: auto;
  }

  .muted {
    color: var(--fg-muted);
  }

  .images {
    display: flex;
    gap: 16px;
  }

  figure {
    flex: 1;
    margin: 0;
    min-width: 0;
    text-align: center;
  }

  figure img {
    max-width: 100%;
    max-height: 60vh;
    background: repeating-conic-gradient(var(--bg-alt) 0 25%, var(--bg) 0 50%) 0 0 / 16px 16px;
    border: 1px solid var(--border);
  }

  figcaption {
    font-size: 12px;
    color: var(--fg-muted);
    margin-top: 4px;
  }

  .none {
    padding: 40px;
    color: var(--fg-muted);
    border: 1px dashed var(--border);
  }

  .previews {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .scroller {
    flex: 1;
    min-width: 0;
    overflow: auto;
    background: var(--bg);
  }

  .scroller + .scroller {
    border-left: 1px solid var(--border);
  }

  .markdown-body {
    padding: 16px 24px 40px;
  }

  .markdown-body :global(.git-del) {
    background: color-mix(in srgb, #cf222e 12%, transparent);
    box-shadow: -4px 0 0 #cf222e;
  }

  .markdown-body :global(.git-add) {
    background: color-mix(in srgb, #2da44e 12%, transparent);
    box-shadow: -4px 0 0 #2da44e;
  }
</style>
