<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { store, type Doc, type MenuItem } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { isMac, tr } from '../lib/i18n.svelte';
  import { mdimgUrl } from '../lib/markdown';
  import { clampZoom, stepZoom, svgToDataUrl, type ImageBg } from '../lib/image';
  import { basename } from '../lib/paths';

  let {
    doc: docProp,
    zoom = $bindable<number | null>(null),
    bg = $bindable<ImageBg>('checker'),
  }: { doc: Doc; zoom?: number | null; bg?: ImageBg } = $props();

  const doc = untrack(() => docProp);

  let stage: HTMLDivElement | undefined = $state();
  let imgEl: HTMLImageElement | undefined = $state();
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);
  let stageWidth = $state(0);
  let stageHeight = $state(0);
  let loadError = $state(false);
  let panning = $state(false);
  let panStart = { x: 0, y: 0, scrollLeft: 0, scrollTop: 0 };

  const isInlineSvg = $derived(doc.kind === 'text' && store.isSvg(doc.path));
  const src = $derived(
    isInlineSvg ? svgToDataUrl(doc.text) : `${mdimgUrl(doc.path)}?v=${doc.imageVersion}`,
  );

  // Fit モードでの実効倍率（コンテナの余白を引いた範囲に収まる倍率。小さい画像は 100% のまま）
  const fitScale = $derived.by(() => {
    if (!naturalWidth || !naturalHeight || !stageWidth || !stageHeight) return 1;
    const pad = 48;
    const availW = Math.max(32, stageWidth - pad);
    const availH = Math.max(32, stageHeight - pad);
    return Math.min(1, availW / naturalWidth, availH / naturalHeight);
  });

  const effectiveZoom = $derived(zoom ?? fitScale);
  const canPan = $derived(
    zoom !== null &&
      ((naturalWidth * zoom > stageWidth - 24) || (naturalHeight * zoom > stageHeight - 24)),
  );

  // 解像度・ズーム倍率をステータスバー用に反映する
  $effect(() => {
    doc.imageMeta = naturalWidth > 0 && naturalHeight > 0
      ? { width: naturalWidth, height: naturalHeight, zoom, fitScale }
      : null;
  });

  // ソースが変わったらエラー状態をリセットする
  $effect(() => {
    void src;
    loadError = false;
  });

  onMount(() => {
    const el = stage;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r) return;
      stageWidth = r.width;
      stageHeight = r.height;
    });
    ro.observe(el);
    // Ctrl/Cmd + ホイールでズーム（ブラウザのページ拡大を止めるため passive: false）
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !(isMac && e.metaKey)) return;
      e.preventDefault();
      if (e.deltaY === 0) return;
      const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
      zoom = clampZoom(Math.round(effectiveZoom * factor * 100) / 100);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => {
      ro.disconnect();
      el.removeEventListener('wheel', onWheel);
    };
  });

  onDestroy(() => {
    doc.imageMeta = null;
  });

  function onLoad() {
    if (!imgEl) return;
    naturalWidth = imgEl.naturalWidth;
    naturalHeight = imgEl.naturalHeight;
    loadError = false;
  }

  function onError() {
    naturalWidth = 0;
    naturalHeight = 0;
    loadError = true;
  }

  function copyImage() {
    if (isInlineSvg) void store.copyImage({ svgText: doc.text });
    else void store.copyImage({ path: doc.path });
  }

  function zoomIn() {
    zoom = stepZoom(effectiveZoom, 1);
  }

  function zoomOut() {
    zoom = stepZoom(effectiveZoom, -1);
  }

  function toggleFit() {
    zoom = zoom === null ? 1 : null;
  }

  function openContextMenu(e: MouseEvent) {
    e.stopPropagation();
    const bgLabel = (kind: ImageBg, text: string) => (bg === kind ? `✓ ${text}` : `　${text}`);
    const items: MenuItem[] = [
      {
        label: tr('画像をコピー', 'Copy image', '复制图片'),
        shortcut: 'Ctrl+C',
        disabled: loadError || doc.deleted,
        action: copyImage,
      },
      { label: '', separator: true },
      { label: tr('拡大', 'Zoom in', '放大'), shortcut: '+', action: zoomIn },
      { label: tr('縮小', 'Zoom out', '缩小'), shortcut: '-', action: zoomOut },
      { label: tr('実際のサイズ (100%)', 'Actual size (100%)', '实际大小 (100%)'), shortcut: '1', action: () => (zoom = 1) },
      { label: tr('画面に合わせる', 'Fit to view', '适合窗口'), shortcut: '0', action: () => (zoom = null) },
      { label: '', separator: true },
      { label: bgLabel('checker', tr('背景: 市松模様', 'Background: Checkerboard', '背景：棋盘格')), action: () => (bg = 'checker') },
      { label: bgLabel('light', tr('背景: 明るい', 'Background: Light', '背景：浅色')), action: () => (bg = 'light') },
      { label: bgLabel('dark', tr('背景: 暗い', 'Background: Dark', '背景：深色')), action: () => (bg = 'dark') },
      { label: '', separator: true },
      { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void store.copyPath(doc.path, false) },
      { label: tr('相対パスをコピー', 'Copy relative path', '复制相对路径'), action: () => void store.copyPath(doc.path, true) },
      {
        label: tr('既定のアプリで開く', 'Open in default app', '用默认应用打开'),
        action: () => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error')),
      },
      {
        label: isMac ? tr('Finder で表示', 'Reveal in Finder', '在访达中显示') : tr('エクスプローラーで表示', 'Reveal in Explorer', '在资源管理器中显示'),
        action: () => void api.revealInExplorer(doc.path).catch((err) => store.toast(errorMessage(err), 'error')),
      },
    ];
    store.openMenu(e, items);
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = (isMac ? e.metaKey : e.ctrlKey) && !e.altKey;
    if (mod && e.key.toLowerCase() === 'c') {
      e.preventDefault();
      e.stopPropagation();
      copyImage();
      return;
    }
    if (mod || e.altKey) return;
    if (e.key === '+' || e.key === '=') {
      e.preventDefault();
      zoomIn();
    } else if (e.key === '-') {
      e.preventDefault();
      zoomOut();
    } else if (e.key === '0') {
      e.preventDefault();
      zoom = null;
    } else if (e.key === '1') {
      e.preventDefault();
      zoom = 1;
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !canPan || !stage) return;
    panning = true;
    panStart = { x: e.clientX, y: e.clientY, scrollLeft: stage.scrollLeft, scrollTop: stage.scrollTop };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!panning || !stage) return;
    stage.scrollLeft = panStart.scrollLeft - (e.clientX - panStart.x);
    stage.scrollTop = panStart.scrollTop - (e.clientY - panStart.y);
  }

  function onPointerUp(e: PointerEvent) {
    if (!panning) return;
    panning = false;
    const el = e.currentTarget as HTMLElement;
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="image-preview bg-{bg}"
  class:pannable={canPan}
  class:panning
  bind:this={stage}
  tabindex="0"
  role="region"
  aria-label={tr('画像プレビュー', 'Image preview', '图片预览')}
  oncontextmenu={openContextMenu}
  onkeydown={onKeydown}
  ondblclick={toggleFit}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
>
  {#if loadError}
    <div class="error">
      <p>{tr('この画像を表示できませんでした。', 'Could not display this image.', '无法显示此图片。')}</p>
      <button class="btn" onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}>
        {tr('既定のアプリで開く', 'Open in default app', '用默认应用打开')}
      </button>
    </div>
  {:else}
    <div class="canvas" class:fit={zoom === null}>
      <img
        bind:this={imgEl}
        {src}
        alt={basename(doc.path)}
        class:fit={zoom === null}
        class:pixelated={!isInlineSvg && effectiveZoom >= 4}
        style:width={zoom !== null && naturalWidth ? `${Math.round(naturalWidth * zoom)}px` : undefined}
        style:height={zoom !== null && naturalHeight ? `${Math.round(naturalHeight * zoom)}px` : undefined}
        draggable="false"
        onload={onLoad}
        onerror={onError}
      />
    </div>
  {/if}
</div>

<style>
  .image-preview {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    outline: none;
    position: relative;
  }

  .image-preview.bg-checker {
    background-color: var(--bg);
    background-image: conic-gradient(
      color-mix(in srgb, var(--fg) 7%, transparent) 90deg,
      transparent 90deg 180deg,
      color-mix(in srgb, var(--fg) 7%, transparent) 180deg 270deg,
      transparent 270deg
    );
    background-size: 20px 20px;
  }

  .image-preview.bg-light {
    background: #ffffff;
  }

  .image-preview.bg-dark {
    background: #18191c;
  }

  .image-preview.pannable {
    cursor: grab;
  }

  .image-preview.panning {
    cursor: grabbing;
  }

  .canvas {
    margin: auto;
    padding: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 100%;
    min-height: 100%;
  }

  .canvas.fit {
    width: 100%;
    height: 100%;
  }

  img {
    display: block;
    box-shadow: 0 2px 16px rgba(0, 0, 0, 0.14);
    user-select: none;
    flex: none;
  }

  img.fit {
    max-width: 100%;
    max-height: 100%;
    width: auto;
    height: auto;
    object-fit: contain;
  }

  img.pixelated {
    image-rendering: pixelated;
  }

  .error {
    margin: auto;
    text-align: center;
    color: var(--fg-muted);
    padding: 24px;
  }

  .error p {
    margin: 0 0 12px;
  }
</style>
