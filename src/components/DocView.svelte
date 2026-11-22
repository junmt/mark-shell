<script lang="ts">
  import { untrack } from 'svelte';
  import { store, type Doc, type Mode } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import Editor from './Editor.svelte';
  import Preview from './Preview.svelte';
  import ImagePreview from './ImagePreview.svelte';
  import HtmlPreview from './HtmlPreview.svelte';
  import PdfPreview from './PdfPreview.svelte';
  import Icon from './Icon.svelte';
  import { stepZoom, type ImageBg } from '../lib/image';
  import { parsePageInput, stepPdfZoom } from '../lib/pdf';
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

  const isRasterImage = doc.kind === 'image';
  const isPdf = doc.kind === 'pdf';
  const isBinaryPreview = isRasterImage || isPdf;
  const isSvg = $derived(doc.kind === 'text' && store.isSvg(doc.path));
  const isHtml = $derived(doc.kind === 'text' && store.isHtml(doc.path));
  const hasPreview = $derived(!isBinaryPreview && store.hasPreview(doc.path));
  // Markdown・SVG・HTML はモードを切り替えられる。その他のテキストは編集だけ、ラスター画像・PDF はプレビューだけ
  const mode = $derived<Mode>(isBinaryPreview ? 'preview' : hasPreview ? doc.mode : 'edit');
  const showImageTools = $derived(isRasterImage || (isSvg && mode !== 'edit'));
  const showHtmlTools = $derived(isHtml && mode !== 'edit');

  let splitWidth = $state(0.5);
  let splitting = $state(false);
  let body: HTMLDivElement | undefined = $state();
  let imageZoom = $state<number | null>(null);
  let imageBg = $state<ImageBg>('checker');
  let htmlReloadSeq = $state(0);

  let pdfZoom = $state<number | null>(null);
  let pdfRotation = $state(0);
  let pdfPage = $state(1);
  let pdfPages = $state(0);
  let pdfJumpToPage = $state<((page: number) => void) | null>(null);
  let pdfPageInput = $state('1');

  $effect(() => {
    pdfPageInput = String(pdfPage);
  });

  const effectiveZoom = $derived(imageZoom ?? doc.imageMeta?.fitScale ?? 1);
  const zoomLabel = $derived(
    imageZoom === null
      ? tr('全体', 'Fit', '适合')
      : `${Math.round(imageZoom * 100)}%`,
  );

  const effectivePdfZoom = $derived(pdfZoom ?? doc.pdfMeta?.fitScale ?? 1);
  const pdfZoomLabel = $derived(
    pdfZoom === null
      ? tr('幅', 'Fit', '适宽')
      : `${Math.round(pdfZoom * 100)}%`,
  );

  function commitPdfPage() {
    if (pdfPages <= 0) return;
    const parsed = parsePageInput(pdfPageInput, pdfPages);
    if (parsed !== null) {
      pdfJumpToPage?.(parsed);
    } else {
      pdfPageInput = String(pdfPage);
    }
  }

  function setMode(m: Mode) {
    doc.mode = m;
    store.saveWorkspaceState();
  }

  function cycleBg() {
    imageBg = imageBg === 'checker' ? 'light' : imageBg === 'light' ? 'dark' : 'checker';
  }

  function copyCurrentImage() {
    if (isSvg) void store.copyImage({ svgText: doc.text });
    else void store.copyImage({ path: doc.path });
  }

  function dragSplit(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    splitting = true;
    const move = (ev: PointerEvent) => {
      if (!body) return;
      const r = body.getBoundingClientRect();
      splitWidth = Math.min(0.85, Math.max(0.15, (ev.clientX - r.left) / r.width));
    };
    const up = () => {
      splitting = false;
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  }

  // 分割表示のスクロール同期で使う
  const sync = { editorToPreview: null as ((line: number) => void) | null, previewToEditor: null as ((line: number) => void) | null };
</script>

<div class="doc">
  <div class="bar">
    {#if hasPreview}
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
    {#if showImageTools}
      <div class="img-tools">
        <button
          class="icon-btn"
          title={tr('縮小 (-)', 'Zoom out (-)', '缩小 (-)')}
          onclick={() => (imageZoom = stepZoom(effectiveZoom, -1))}
        >
          <Icon name="zoomOut" size={14} />
        </button>
        <button
          class="tool-btn"
          title={tr('画面に合わせる / 100% を切り替え', 'Toggle Fit / 100%', '切换适合窗口 / 100%')}
          onclick={() => (imageZoom = imageZoom === null ? 1 : null)}
        >
          {zoomLabel}
        </button>
        <button
          class="icon-btn"
          title={tr('拡大 (+)', 'Zoom in (+)', '放大 (+)')}
          onclick={() => (imageZoom = stepZoom(effectiveZoom, 1))}
        >
          <Icon name="zoomIn" size={14} />
        </button>
        <span class="tool-sep"></span>
        <button
          class="tool-btn"
          title={tr('背景を切り替え（市松模様 / 明るい / 暗い）', 'Switch background (Checkerboard / Light / Dark)', '切换背景（棋盘格 / 浅色 / 深色）')}
          onclick={cycleBg}
        >
          <span class="bg-swatch bg-{imageBg}"></span>
          {imageBg === 'checker'
            ? tr('透過', 'Grid', '透明')
            : imageBg === 'light'
              ? tr('明', 'Light', '浅')
              : tr('暗', 'Dark', '深')}
        </button>
        <span class="tool-sep"></span>
        <button
          class="tool-btn"
          title={tr('画像をクリップボードにコピー (Ctrl+C)', 'Copy image to clipboard (Ctrl+C)', '将图片复制到剪贴板 (Ctrl+C)')}
          disabled={doc.deleted}
          onclick={copyCurrentImage}
        >
          <Icon name="copy" size={14} />
          <span>{tr('画像をコピー', 'Copy image', '复制图片')}</span>
        </button>
        <button
          class="icon-btn"
          title={tr('既定のアプリで開く', 'Open in default app', '用默认应用打开')}
          disabled={doc.deleted}
          onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}
        >
          <Icon name="externalLink" size={14} />
        </button>
      </div>
    {:else if showHtmlTools}
      <div class="img-tools">
        <button
          class="icon-btn"
          title={tr('プレビューを再読み込み', 'Reload preview', '重新加载预览')}
          onclick={() => htmlReloadSeq++}
        >
          <Icon name="refresh" size={14} />
        </button>
        <button
          class="tool-btn"
          title={tr('既定のブラウザで開く', 'Open in default browser', '在默认浏览器中打开')}
          disabled={doc.deleted}
          onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}
        >
          <Icon name="externalLink" size={14} />
          <span>{tr('ブラウザで開く', 'Open in browser', '在浏览器中打开')}</span>
        </button>
      </div>
    {:else if isPdf}
      <div class="img-tools">
        {#if pdfPages > 0}
          <button
            class="icon-btn"
            title={tr('前のページ (PageUp)', 'Previous page (PageUp)', '上一页 (PageUp)')}
            disabled={pdfPage <= 1}
            onclick={() => pdfJumpToPage?.(pdfPage - 1)}
          >
            <Icon name="chevronLeft" size={14} />
          </button>
          <form
            class="page-nav"
            onsubmit={(e) => {
              e.preventDefault();
              commitPdfPage();
            }}
          >
            <input
              class="page-input"
              type="text"
              inputmode="numeric"
              aria-label={tr('ページ番号', 'Page number', '页码')}
              bind:value={pdfPageInput}
              onblur={commitPdfPage}
            />
            <span class="page-total">/ {pdfPages}</span>
          </form>
          <button
            class="icon-btn"
            title={tr('次のページ (PageDown)', 'Next page (PageDown)', '下一页 (PageDown)')}
            disabled={pdfPage >= pdfPages}
            onclick={() => pdfJumpToPage?.(pdfPage + 1)}
          >
            <Icon name="chevronRight" size={14} />
          </button>
          <span class="tool-sep"></span>
          <button
            class="icon-btn"
            title={tr('縮小 (-)', 'Zoom out (-)', '缩小 (-)')}
            onclick={() => (pdfZoom = stepPdfZoom(effectivePdfZoom, -1))}
          >
            <Icon name="zoomOut" size={14} />
          </button>
          <button
            class="tool-btn"
            title={tr('幅に合わせる / 100% を切り替え', 'Toggle Fit width / 100%', '切换适合宽度 / 100%')}
            onclick={() => (pdfZoom = pdfZoom === null ? 1 : null)}
          >
            {pdfZoomLabel}
          </button>
          <button
            class="icon-btn"
            title={tr('拡大 (+)', 'Zoom in (+)', '放大 (+)')}
            onclick={() => (pdfZoom = stepPdfZoom(effectivePdfZoom, 1))}
          >
            <Icon name="zoomIn" size={14} />
          </button>
          <span class="tool-sep"></span>
          <button
            class="icon-btn"
            title={tr('右に 90° 回転 (R)', 'Rotate 90° clockwise (R)', '顺时针旋转 90° (R)')}
            onclick={() => (pdfRotation = (pdfRotation + 90) % 360)}
          >
            <Icon name="rotateCw" size={14} />
          </button>
          <span class="tool-sep"></span>
        {/if}
        <button
          class="icon-btn"
          title={tr('既定のアプリで開く', 'Open in default app', '用默认应用打开')}
          disabled={doc.deleted}
          onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}
        >
          <Icon name="externalLink" size={14} />
        </button>
      </div>
    {/if}
  </div>

  {#if doc.deleted}
    {#if isBinaryPreview}
      <div class="banner warn">
        {tr('このファイルは削除されました。', 'This file was deleted.', '此文件已被删除。')}
        <button class="btn" onclick={() => void store.closeTab(doc.path)}>{tr('タブを閉じる', 'Close tab', '关闭标签页')}</button>
      </div>
    {:else}
      <div class="banner warn">{tr('このファイルは削除されました。保存すると作り直します。', 'This file was deleted. Saving will recreate it.', '此文件已被删除。保存后将重新创建。')}
        <button class="btn" onclick={() => void store.save(doc, true)}>{tr('保存して作り直す', 'Save to recreate', '保存并重新创建')}</button>
        <button class="btn" onclick={() => void store.closeTab(doc.path)}>{tr('タブを閉じる', 'Close tab', '关闭标签页')}</button>
      </div>
    {/if}
  {/if}
  {#if store.aiBefore.has(doc.path)}
    <div class="banner ai">✦ {tr('AI セッション中にこのファイルが変更されました。', 'This file was changed during an AI session.', '此文件在 AI 会话中被修改。')}
      <button class="btn" onclick={() => store.showAiDiff(doc)}>{tr('差分を見る', 'View diff', '查看差异')}</button>
      <button class="btn" onclick={() => void store.revertAiChange(doc)}>{tr('変更前に戻す', 'Revert changes', '还原修改')}</button>
      <button class="btn" onclick={() => store.dismissAiChange(doc.path)}>{tr('閉じる', 'Dismiss', '关闭')}</button>
    </div>
  {/if}

  <div class="body" bind:this={body}>
    {#if isRasterImage}
      <ImagePreview {doc} bind:zoom={imageZoom} bind:bg={imageBg} />
    {:else if isPdf}
      <PdfPreview
        {doc}
        bind:zoom={pdfZoom}
        bind:rotation={pdfRotation}
        bind:currentPage={pdfPage}
        bind:numPages={pdfPages}
        bind:jumpToPage={pdfJumpToPage}
      />
    {:else}
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
          {#if isSvg}
            <ImagePreview {doc} bind:zoom={imageZoom} bind:bg={imageBg} />
          {:else if isHtml}
            <HtmlPreview {doc} reloadSeq={htmlReloadSeq} dragging={splitting} />
          {:else}
            <Preview {doc} onscrollline={(l) => mode === 'split' && sync.previewToEditor?.(l)} bind:scrollToLine={sync.editorToPreview} />
          {/if}
        </div>
      {/if}
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
    flex: none;
  }

  .img-tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }

  .tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: none;
    background: transparent;
    color: var(--fg-muted);
    padding: 3px 7px;
    border-radius: 4px;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }

  .tool-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .tool-btn:disabled,
  .img-tools .icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .tool-sep {
    width: 1px;
    height: 14px;
    background: var(--border);
    margin: 0 4px;
  }

  .page-nav {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--fg-muted);
  }

  .page-input {
    width: 40px;
    padding: 1px 4px;
    font-size: 12px;
    text-align: center;
  }

  .page-total {
    white-space: nowrap;
  }

  .bg-swatch {
    width: 11px;
    height: 11px;
    border-radius: 2px;
    border: 1px solid var(--border);
    display: inline-block;
    flex: none;
  }

  .bg-swatch.bg-checker {
    background-color: var(--bg);
    background-image: conic-gradient(var(--fg-muted) 90deg, transparent 90deg 180deg, var(--fg-muted) 180deg 270deg, transparent 270deg);
    background-size: 6px 6px;
  }

  .bg-swatch.bg-light {
    background: #ffffff;
  }

  .bg-swatch.bg-dark {
    background: #18191c;
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
