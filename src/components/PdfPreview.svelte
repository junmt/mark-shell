<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { store, type Doc, type MenuItem } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { isMac, tr } from '../lib/i18n.svelte';
  import {
    clampPage,
    clampPdfZoom,
    computeCanvasOutputScale,
    computeFitWidthScale,
    openPdfDocument,
    stepPdfZoom,
    type PDFDocumentLoadingTask,
    type PDFDocumentProxy,
    type PdfJsLib,
    type RenderTask,
    type TextLayer,
  } from '../lib/pdf';

  let {
    doc: docProp,
    zoom = $bindable<number | null>(null),
    rotation = $bindable<number>(0),
    currentPage = $bindable<number>(1),
    numPages = $bindable<number>(0),
    jumpToPage = $bindable<((page: number) => void) | null>(null),
  }: {
    doc: Doc;
    zoom?: number | null;
    rotation?: number;
    currentPage?: number;
    numPages?: number;
    jumpToPage?: ((page: number) => void) | null;
  } = $props();

  const doc = untrack(() => docProp);

  type PageSize = { width: number; height: number };
  type RenderJob = { cancel: () => void };

  let stage: HTMLDivElement | undefined = $state();
  let stageWidth = $state(0);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let passwordRequired = $state(false);
  let passwordIncorrect = $state(false);
  let passwordInput = $state('');
  let pageSizes = $state<PageSize[]>([]);

  let pdfjsLib: PdfJsLib | null = null;
  let pdfDoc: PDFDocumentProxy | null = null;
  let loadingTask: PDFDocumentLoadingTask | null = null;
  let loadSeq = 0;
  let io: IntersectionObserver | null = null;
  let scrollRaf = 0;
  let destroyed = false;

  // Svelte の再描画と切り離した DOM 参照・レンダリング管理
  const pageEls = new Map<number, HTMLDivElement>();
  const visiblePages = new Set<number>();
  const renderedKeys = new Map<number, string>();
  const renderJobs = new Map<number, RenderJob>();

  const refPageSize = $derived(pageSizes[0] ?? { width: 595, height: 842 });
  const refRotatedWidth = $derived(rotation % 180 === 0 ? refPageSize.width : refPageSize.height);
  const fitScale = $derived(computeFitWidthScale(refRotatedWidth, stageWidth, 48));
  const effectiveZoom = $derived(zoom ?? fitScale);
  const renderKey = $derived(`${loadSeq}:${effectiveZoom.toFixed(4)}:${rotation}`);

  function pageCssSize(index: number): { width: number; height: number } {
    const base = pageSizes[index] ?? refPageSize;
    const w = rotation % 180 === 0 ? base.width : base.height;
    const h = rotation % 180 === 0 ? base.height : base.width;
    return {
      width: Math.max(1, Math.round(w * effectiveZoom)),
      height: Math.max(1, Math.round(h * effectiveZoom)),
    };
  }

  function cancelAllRenders() {
    for (const job of renderJobs.values()) job.cancel();
    renderJobs.clear();
    renderedKeys.clear();
  }

  async function destroyPdf() {
    cancelAllRenders();
    visiblePages.clear();
    const task = loadingTask ?? pdfDoc?.loadingTask ?? null;
    loadingTask = null;
    pdfDoc = null;
    if (task) {
      await task.destroy().catch(() => {});
    }
  }

  async function loadPdf(password?: string, keepPage?: number) {
    const mySeq = ++loadSeq;
    loading = true;
    loadError = null;
    if (password === undefined) {
      passwordRequired = false;
      passwordIncorrect = false;
    }

    await destroyPdf();
    if (destroyed || mySeq !== loadSeq) return;

    let bytes: ArrayBuffer;
    try {
      bytes = await api.readPdf(doc.path);
    } catch (e) {
      if (destroyed || mySeq !== loadSeq) return;
      loading = false;
      loadError = errorMessage(e);
      numPages = 0;
      pageSizes = [];
      return;
    }
    if (destroyed || mySeq !== loadSeq) return;

    const opened = await openPdfDocument(new Uint8Array(bytes), password);
    if (destroyed || mySeq !== loadSeq) {
      await opened.task.destroy().catch(() => {});
      return;
    }
    pdfjsLib = opened.lib;
    const task = opened.task;
    loadingTask = task;

    try {
      const pdf = await task.promise;
      if (destroyed || mySeq !== loadSeq) {
        await task.destroy().catch(() => {});
        return;
      }
      pdfDoc = pdf;
      passwordRequired = false;
      passwordIncorrect = false;
      passwordInput = '';

      const total = pdf.numPages;
      const firstPage = await pdf.getPage(1);
      if (destroyed || mySeq !== loadSeq) return;
      const firstVp = firstPage.getViewport({ scale: 1 });
      const defaultSize: PageSize = { width: firstVp.width, height: firstVp.height };
      const sizes: PageSize[] = Array.from({ length: total }, () => ({ ...defaultSize }));

      // ページ数が少ない PDF は初回に全ページサイズを取得してスクロール位置を正確にする
      if (total > 1 && total <= 30) {
        for (let i = 2; i <= total; i++) {
          const p = await pdf.getPage(i);
          if (destroyed || mySeq !== loadSeq) return;
          const vp = p.getViewport({ scale: 1 });
          sizes[i - 1] = { width: vp.width, height: vp.height };
        }
      }

      numPages = total;
      pageSizes = sizes;
      const nextPage = keepPage ? clampPage(keepPage, total) : 1;
      currentPage = nextPage;
      loading = false;

      await tick();
      if (destroyed || mySeq !== loadSeq) return;
      reconnectObserver();
      if (keepPage && nextPage > 1) {
        scrollToPage(nextPage, 'instant');
      }
      scheduleVisibleRenders();
    } catch (e) {
      if (destroyed || mySeq !== loadSeq) return;
      loading = false;
      const err = e as { name?: string; code?: number; message?: string };
      if (err?.name === 'PasswordException') {
        passwordRequired = true;
        passwordIncorrect = err.code === pdfjsLib?.PasswordResponses.INCORRECT_PASSWORD;
        return;
      }
      loadError = err?.message || errorMessage(e);
      numPages = 0;
      pageSizes = [];
    }
  }

  function clearPageDom(pageNum: number) {
    const el = pageEls.get(pageNum);
    if (!el) return;
    const canvas = el.querySelector<HTMLCanvasElement>('canvas');
    if (canvas) {
      canvas.width = 0;
      canvas.height = 0;
    }
    const textLayer = el.querySelector<HTMLDivElement>('.textLayer');
    if (textLayer) textLayer.replaceChildren();
    const linkLayer = el.querySelector<HTMLDivElement>('.link-layer');
    if (linkLayer) linkLayer.replaceChildren();
  }

  async function renderPage(pageNum: number, key: string) {
    const pdf = pdfDoc;
    const el = pageEls.get(pageNum);
    if (!pdf || !el) return;
    if (renderedKeys.get(pageNum) === key) return;

    renderJobs.get(pageNum)?.cancel();

    let cancelled = false;
    let activeRenderTask: RenderTask | null = null;
    let activeTextLayer: TextLayer | null = null;

    const job: RenderJob = {
      cancel: () => {
        cancelled = true;
        try {
          activeRenderTask?.cancel();
        } catch {
          // 既に終了している場合は無視
        }
        try {
          activeTextLayer?.cancel();
        } catch {
          // 既に終了している場合は無視
        }
      },
    };
    renderJobs.set(pageNum, job);

    try {
      const page = await pdf.getPage(pageNum);
      if (cancelled || destroyed || pdfDoc !== pdf || renderKey !== key) return;

      const baseVp = page.getViewport({ scale: 1 });
      const prevSize = pageSizes[pageNum - 1];
      if (
        prevSize &&
        (Math.abs(prevSize.width - baseVp.width) > 0.5 || Math.abs(prevSize.height - baseVp.height) > 0.5)
      ) {
        pageSizes[pageNum - 1] = { width: baseVp.width, height: baseVp.height };
      }

      const scale = effectiveZoom;
      const rot = rotation;
      const viewport = page.getViewport({ scale, rotation: rot });
      const outputScale = computeCanvasOutputScale(viewport.width, viewport.height, window.devicePixelRatio || 1);

      const canvas = el.querySelector<HTMLCanvasElement>('canvas');
      const textDiv = el.querySelector<HTMLDivElement>('.textLayer');
      const linkDiv = el.querySelector<HTMLDivElement>('.link-layer');
      if (!canvas || !textDiv || !linkDiv) return;

      canvas.width = Math.max(1, Math.floor(viewport.width * outputScale));
      canvas.height = Math.max(1, Math.floor(viewport.height * outputScale));
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;

      const transform = outputScale !== 1 ? [outputScale, 0, 0, outputScale, 0, 0] : undefined;
      activeRenderTask = page.render({ canvas, viewport, transform });
      await activeRenderTask.promise;
      if (cancelled || destroyed || pdfDoc !== pdf || renderKey !== key) return;

      // テキスト選択レイヤーを描画
      textDiv.replaceChildren();
      textDiv.style.setProperty('--total-scale-factor', String(viewport.scale));
      textDiv.style.width = `${viewport.width}px`;
      textDiv.style.height = `${viewport.height}px`;
      if (pdfjsLib) {
        activeTextLayer = new pdfjsLib.TextLayer({
          textContentSource: page.streamTextContent(),
          container: textDiv,
          viewport,
        });
        await activeTextLayer.render();
      }
      if (cancelled || destroyed || pdfDoc !== pdf || renderKey !== key) return;

      // リンクアノテーション（ページ内ジャンプ・外部 URL）を重ねる
      linkDiv.replaceChildren();
      const annotations = await page.getAnnotations({ intent: 'display' }).catch(() => []);
      if (cancelled || destroyed || pdfDoc !== pdf || renderKey !== key) return;

      for (const annot of annotations as Array<{ subtype?: string; rect?: number[]; url?: string; unsafeUrl?: string; dest?: unknown }>) {
        if (annot.subtype !== 'Link' || !annot.rect || annot.rect.length < 4) continue;
        const url = annot.url;
        const dest = annot.dest;
        if (!url && !dest) continue;

        const [x1, y1] = viewport.convertToViewportPoint(annot.rect[0], annot.rect[1]);
        const [x2, y2] = viewport.convertToViewportPoint(annot.rect[2], annot.rect[3]);
        const left = Math.min(x1, x2);
        const top = Math.min(y1, y2);
        const width = Math.abs(x2 - x1);
        const height = Math.abs(y2 - y1);
        if (width < 1 || height < 1) continue;

        const a = document.createElement('a');
        a.className = 'pdf-link';
        a.style.left = `${left}px`;
        a.style.top = `${top}px`;
        a.style.width = `${width}px`;
        a.style.height = `${height}px`;
        if (url) {
          a.href = url;
          a.title = url;
          a.addEventListener('click', (ev) => {
            ev.preventDefault();
            if (/^(?:https?|mailto):/i.test(url)) void store.openExternal(url);
          });
        } else if (dest) {
          a.href = '#';
          a.addEventListener('click', (ev) => {
            ev.preventDefault();
            void navigateDest(dest);
          });
        }
        linkDiv.appendChild(a);
      }

      renderedKeys.set(pageNum, key);
    } catch (e) {
      const err = e as { name?: string };
      if (err?.name !== 'RenderingCancelledException' && err?.name !== 'AbortException') {
        // 個別ページの描画エラーは無視して他ページへの影響を避ける
      }
    } finally {
      if (renderJobs.get(pageNum) === job) {
        renderJobs.delete(pageNum);
      }
    }
  }

  async function navigateDest(dest: unknown) {
    const pdf = pdfDoc;
    if (!pdf) return;
    try {
      const explicit = typeof dest === 'string' ? await pdf.getDestination(dest) : dest;
      if (!Array.isArray(explicit) || !explicit[0]) return;
      const ref = explicit[0];
      const pageIndex = typeof ref === 'number' ? ref : await pdf.getPageIndex(ref);
      scrollToPage(pageIndex + 1, 'smooth');
    } catch {
      // 不正なリンク先は無視
    }
  }

  function scheduleVisibleRenders() {
    if (!pdfDoc || numPages === 0) return;
    const key = renderKey;
    const anchor = visiblePages.size > 0 ? [...visiblePages] : [currentPage];
    const minVis = Math.min(...anchor);
    const maxVis = Math.max(...anchor);

    const wanted = new Set<number>();
    const buffer = 2;
    for (let p = Math.max(1, minVis - buffer); p <= Math.min(numPages, maxVis + buffer); p++) {
      wanted.add(p);
    }

    // 遠いページのキャンバスを解放してメモリ使用量を抑える
    const keepRadius = 5;
    for (const p of [...renderedKeys.keys()]) {
      if (p < minVis - keepRadius || p > maxVis + keepRadius) {
        renderJobs.get(p)?.cancel();
        renderJobs.delete(p);
        renderedKeys.delete(p);
        clearPageDom(p);
      }
    }

    for (const p of wanted) {
      if (renderedKeys.get(p) !== key) {
        void renderPage(p, key);
      }
    }
  }

  function updateCurrentPageFromScroll() {
    const el = stage;
    if (!el || numPages === 0) return;
    const viewCenter = el.scrollTop + el.clientHeight * 0.35;
    let bestPage = currentPage;
    let bestDist = Infinity;
    for (let p = 1; p <= numPages; p++) {
      const pageEl = pageEls.get(p);
      if (!pageEl) continue;
      const top = pageEl.offsetTop;
      const bottom = top + pageEl.offsetHeight;
      if (viewCenter >= top && viewCenter <= bottom) {
        bestPage = p;
        break;
      }
      const dist = viewCenter < top ? top - viewCenter : viewCenter - bottom;
      if (dist < bestDist) {
        bestDist = dist;
        bestPage = p;
      }
    }
    if (bestPage !== currentPage) {
      currentPage = bestPage;
    }
  }

  function onScroll() {
    if (scrollRaf) return;
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0;
      updateCurrentPageFromScroll();
    });
  }

  function scrollToPage(page: number, behavior: ScrollBehavior = 'smooth') {
    if (numPages === 0 || !stage) return;
    const target = clampPage(page, numPages);
    currentPage = target;
    const el = pageEls.get(target);
    if (!el) return;
    const top = Math.max(0, el.offsetTop - 16);
    stage.scrollTo({ top, behavior });
    scheduleVisibleRenders();
  }

  function registerPage(node: HTMLDivElement, pageNum: number) {
    pageEls.set(pageNum, node);
    io?.observe(node);
    return {
      destroy() {
        io?.unobserve(node);
        pageEls.delete(pageNum);
        visiblePages.delete(pageNum);
        renderJobs.get(pageNum)?.cancel();
        renderJobs.delete(pageNum);
        renderedKeys.delete(pageNum);
      },
    };
  }

  function reconnectObserver() {
    io?.disconnect();
    visiblePages.clear();
    if (!stage) return;
    io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const p = Number((entry.target as HTMLElement).dataset.page);
          if (!p) continue;
          if (entry.isIntersecting) visiblePages.add(p);
          else visiblePages.delete(p);
        }
        scheduleVisibleRenders();
      },
      { root: stage, rootMargin: '300px 0px' },
    );
    for (const el of pageEls.values()) {
      io.observe(el);
    }
  }

  // ステータスバー用にメタ情報を同期する
  $effect(() => {
    doc.pdfMeta =
      numPages > 0
        ? { page: currentPage, pages: numPages, zoom, fitScale, rotation }
        : null;
  });

  // 外部でファイルが更新されたら再読み込みする（初回も実行）
  let prevVersion = -1;
  $effect(() => {
    const v = doc.imageVersion;
    const keep = prevVersion >= 0 ? untrack(() => currentPage) : undefined;
    prevVersion = v;
    void loadPdf(undefined, keep);
  });

  // 倍率・回転が変わったら現在見えているページ群を再描画する
  $effect(() => {
    void renderKey;
    if (!pdfDoc || numPages === 0) return;
    const targetPage = untrack(() => currentPage);
    void tick().then(() => {
      if (destroyed) return;
      scheduleVisibleRenders();
      // 幅フィット時のリサイズ等でスクロールが大きくズレないようにする
      void targetPage;
    });
  });

  onMount(() => {
    jumpToPage = (p: number) => scrollToPage(p, 'smooth');
    const el = stage;
    if (!el) return;
    stageWidth = el.clientWidth;

    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (!r) return;
      if (Math.abs(r.width - stageWidth) >= 2) {
        stageWidth = r.width;
      }
    });
    ro.observe(el);

    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !(isMac && e.metaKey)) return;
      e.preventDefault();
      if (e.deltaY === 0) return;
      const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
      zoom = clampPdfZoom(Math.round(effectiveZoom * factor * 100) / 100);
    };
    el.addEventListener('wheel', onWheel, { passive: false });

    return () => {
      ro.disconnect();
      el.removeEventListener('wheel', onWheel);
    };
  });

  onDestroy(() => {
    destroyed = true;
    jumpToPage = null;
    doc.pdfMeta = null;
    if (scrollRaf) cancelAnimationFrame(scrollRaf);
    io?.disconnect();
    void destroyPdf();
  });

  function zoomIn() {
    zoom = stepPdfZoom(effectiveZoom, 1);
  }

  function zoomOut() {
    zoom = stepPdfZoom(effectiveZoom, -1);
  }

  function rotateCw() {
    rotation = (rotation + 90) % 360;
  }

  function submitPassword(e: Event) {
    e.preventDefault();
    void loadPdf(passwordInput);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    const mod = (isMac ? e.metaKey : e.ctrlKey) && !e.altKey;
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
    } else if (e.key === 'r' || e.key === 'R') {
      e.preventDefault();
      rotateCw();
    } else if (e.key === 'PageDown') {
      e.preventDefault();
      scrollToPage(currentPage + 1);
    } else if (e.key === 'PageUp') {
      e.preventDefault();
      scrollToPage(currentPage - 1);
    }
  }

  function openContextMenu(e: MouseEvent) {
    e.stopPropagation();
    const sel = window.getSelection()?.toString() ?? '';
    const items: MenuItem[] = [];
    if (sel) {
      items.push(
        {
          label: tr('コピー', 'Copy', '复制'),
          shortcut: 'Ctrl+C',
          action: () => void navigator.clipboard.writeText(sel),
        },
        { label: '', separator: true },
      );
    }
    items.push(
      { label: tr('拡大', 'Zoom in', '放大'), shortcut: '+', action: zoomIn },
      { label: tr('縮小', 'Zoom out', '缩小'), shortcut: '-', action: zoomOut },
      { label: tr('実際のサイズ (100%)', 'Actual size (100%)', '实际大小 (100%)'), shortcut: '1', action: () => (zoom = 1) },
      { label: tr('幅に合わせる', 'Fit to width', '适合宽度'), shortcut: '0', action: () => (zoom = null) },
      { label: tr('右に 90° 回転', 'Rotate 90° clockwise', '顺时针旋转 90°'), shortcut: 'R', action: rotateCw },
      { label: '', separator: true },
      { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void store.copyPath(doc.path, false) },
      { label: tr('相対パスをコピー', 'Copy relative path', '复制相对路径'), action: () => void store.copyPath(doc.path, true) },
      {
        label: tr('既定のアプリで開く', 'Open in default app', '用默认应用打开'),
        disabled: doc.deleted,
        action: () => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error')),
      },
      {
        label: isMac ? tr('Finder で表示', 'Reveal in Finder', '在访达中显示') : tr('エクスプローラーで表示', 'Reveal in Explorer', '在资源管理器中显示'),
        action: () => void api.revealInExplorer(doc.path).catch((err) => store.toast(errorMessage(err), 'error')),
      },
    );
    store.openMenu(e, items);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="pdf-preview"
  bind:this={stage}
  tabindex="0"
  role="region"
  aria-label={tr('PDF プレビュー', 'PDF preview', 'PDF 预览')}
  onscroll={onScroll}
  onkeydown={onKeydown}
  oncontextmenu={openContextMenu}
>
  {#if passwordRequired}
    <form class="state-box" onsubmit={submitPassword}>
      <p>{tr('この PDF はパスワードで保護されています。', 'This PDF is password protected.', '此 PDF 受密码保护。')}</p>
      {#if passwordIncorrect}
        <p class="err-text">{tr('パスワードが正しくありません。', 'Incorrect password.', '密码不正确。')}</p>
      {/if}
      <div class="pw-row">
        <input
          type="password"
          bind:value={passwordInput}
          placeholder={tr('パスワードを入力', 'Enter password', '输入密码')}
          autocomplete="off"
        />
        <button class="btn primary" type="submit">{tr('開く', 'Open', '打开')}</button>
      </div>
    </form>
  {:else if loadError}
    <div class="state-box">
      <p>{tr('この PDF を表示できませんでした。', 'Could not display this PDF.', '无法显示此 PDF。')}</p>
      <p class="detail">{loadError}</p>
      <button
        class="btn"
        disabled={doc.deleted}
        onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}
      >
        {tr('既定のアプリで開く', 'Open in default app', '用默认应用打开')}
      </button>
    </div>
  {:else if loading && numPages === 0}
    <div class="state-box">
      <p>{tr('PDF を読み込んでいます…', 'Loading PDF…', '正在加载 PDF…')}</p>
    </div>
  {:else}
    <div class="pages">
      {#each pageSizes as _, i (i)}
        {@const pageNum = i + 1}
        {@const dim = pageCssSize(i)}
        <div
          class="page"
          data-page={pageNum}
          style:width="{dim.width}px"
          style:height="{dim.height}px"
          use:registerPage={pageNum}
        >
          <canvas></canvas>
          <div class="textLayer"></div>
          <div class="link-layer"></div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pdf-preview {
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
    background: var(--bg-pane);
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 20px 24px 32px;
    min-width: fit-content;
    margin: 0 auto;
  }

  .page {
    position: relative;
    background: #ffffff;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.16);
    flex: none;
    overflow: hidden;
  }

  .page canvas {
    display: block;
    user-select: none;
  }

  .link-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 2;
  }

  .link-layer :global(.pdf-link) {
    position: absolute;
    display: block;
    pointer-events: auto;
    cursor: pointer;
    border-radius: 2px;
  }

  .link-layer :global(.pdf-link:hover) {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  /* PDF.js TextLayer 用スタイル（テキスト選択・コピー用） */
  .page :global(.textLayer) {
    position: absolute;
    inset: 0;
    overflow: clip;
    opacity: 1;
    line-height: 1;
    letter-spacing: normal;
    word-spacing: normal;
    text-align: initial;
    transform-origin: 0 0;
    z-index: 1;
    --min-font-size: 1;
    --text-scale-factor: calc(var(--total-scale-factor, 1) * var(--min-font-size));
    --min-font-size-inv: calc(1 / var(--min-font-size));
  }

  .page :global(.textLayer span),
  .page :global(.textLayer br) {
    color: transparent;
    position: absolute;
    white-space: pre;
    cursor: text;
    transform-origin: 0% 0%;
    -webkit-user-select: text;
    user-select: text;
  }

  .page :global(.textLayer > :not(.markedContent)),
  .page :global(.textLayer .markedContent span:not(.markedContent)) {
    --font-height: 0;
    font-size: calc(var(--text-scale-factor) * var(--font-height));
    --scale-x: 1;
    --rotate: 0deg;
    transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
  }

  .page :global(.textLayer .markedContent) {
    display: contents;
  }

  .page :global(.textLayer ::selection) {
    background: rgba(0, 102, 255, 0.28);
    color: transparent;
  }

  .page :global(.textLayer br::selection) {
    background: transparent;
  }

  .page :global(.textLayer .endOfContent) {
    display: block;
    position: absolute;
    inset: 100% 0 0;
    z-index: 0;
    cursor: default;
    user-select: none;
  }

  .state-box {
    margin: auto;
    text-align: center;
    color: var(--fg-muted);
    padding: 24px;
    max-width: 420px;
  }

  .state-box p {
    margin: 0 0 12px;
  }

  .detail {
    font-size: 12px;
    word-break: break-word;
  }

  .err-text {
    color: var(--danger);
    font-size: 12px;
  }

  .pw-row {
    display: flex;
    gap: 8px;
    justify-content: center;
  }

  .pw-row input {
    width: 220px;
  }
</style>
