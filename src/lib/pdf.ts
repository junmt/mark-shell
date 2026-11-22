// PDF プレビュー用のユーティリティと PDF.js 初期化（PDF を開いたときに動的読み込みする）

import type * as PdfJsNamespace from 'pdfjs-dist';

export type PdfJsLib = typeof PdfJsNamespace;
export type PDFDocumentLoadingTask = PdfJsNamespace.PDFDocumentLoadingTask;
export type PDFDocumentProxy = PdfJsNamespace.PDFDocumentProxy;
export type RenderTask = PdfJsNamespace.RenderTask;
export type TextLayer = PdfJsNamespace.TextLayer;

export const PDF_MIN_ZOOM = 0.25;
export const PDF_MAX_ZOOM = 4;
export const PDF_ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 2, 2.5, 3, 4];

/** WebView でキャンバスが大きすぎて描画に失敗するのを防ぐ上限ピクセル数 */
export const MAX_CANVAS_PIXELS = 4096 * 4096;

/** 印刷用 HTML を生成するときのページ数上限（超える場合は既定アプリでの印刷を案内する） */
export const MAX_PRINT_PAGES = 40;

let pdfjsPromise: Promise<PdfJsLib> | null = null;

/** PDF.js 本体と Worker を必要になったタイミングで読み込む */
export function loadPdfJs(): Promise<PdfJsLib> {
  if (!pdfjsPromise) {
    pdfjsPromise = Promise.all([
      import('pdfjs-dist'),
      import('pdfjs-dist/build/pdf.worker.min.mjs?url'),
    ]).then(([lib, worker]) => {
      lib.GlobalWorkerOptions.workerSrc = worker.default;
      return lib;
    });
  }
  return pdfjsPromise;
}

export function clampPdfZoom(z: number): number {
  return Math.min(PDF_MAX_ZOOM, Math.max(PDF_MIN_ZOOM, z));
}

/** 現在の倍率から一段階拡大・縮小した倍率を返す */
export function stepPdfZoom(current: number, dir: 1 | -1): number {
  const eps = 0.005;
  if (dir > 0) {
    return PDF_ZOOM_STEPS.find((s) => s > current + eps) ?? PDF_MAX_ZOOM;
  }
  for (let i = PDF_ZOOM_STEPS.length - 1; i >= 0; i--) {
    if (PDF_ZOOM_STEPS[i] < current - eps) return PDF_ZOOM_STEPS[i];
  }
  return PDF_MIN_ZOOM;
}

/** ページ幅をビューポートに収める倍率（幅フィット）を計算する */
export function computeFitWidthScale(pageWidth: number, containerWidth: number, padding = 48): number {
  if (!Number.isFinite(pageWidth) || pageWidth <= 0 || !Number.isFinite(containerWidth) || containerWidth <= 0) {
    return 1;
  }
  const avail = Math.max(120, containerWidth - padding);
  return clampPdfZoom(avail / pageWidth);
}

/** ページ番号を 1..numPages の範囲に収める */
export function clampPage(page: number, numPages: number): number {
  if (numPages <= 0) return 1;
  const n = Math.trunc(page);
  if (!Number.isFinite(n)) return 1;
  return Math.min(numPages, Math.max(1, n));
}

/** ページ入力欄の文字列をページ番号に変換する（不正なら null） */
export function parsePageInput(raw: string, numPages: number): number | null {
  const trimmed = raw.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const n = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(n) || n < 1) return null;
  return clampPage(n, numPages);
}

/** HiDPI と最大ピクセル数の両方を考慮したキャンバス描画倍率を計算する */
export function computeCanvasOutputScale(cssWidth: number, cssHeight: number, dpr: number): number {
  const base = Math.min(Math.max(dpr || 1, 1), 3);
  const pixels = cssWidth * cssHeight;
  if (!Number.isFinite(pixels) || pixels <= 0) return base;
  const maxByArea = Math.sqrt(MAX_CANVAS_PIXELS / pixels);
  return Math.max(0.5, Math.min(base, maxByArea));
}

/** PDF.js のドキュメント読み込みを開始する */
export async function openPdfDocument(
  data: Uint8Array,
  password?: string,
): Promise<{ lib: PdfJsLib; task: PDFDocumentLoadingTask }> {
  const lib = await loadPdfJs();
  const task = lib.getDocument({
    data,
    password,
    cMapUrl: '/pdfjs/cmaps/',
    cMapPacked: true,
    standardFontDataUrl: '/pdfjs/standard_fonts/',
    wasmUrl: '/pdfjs/wasm/',
    useSystemFonts: true,
  });
  return { lib, task };
}

/** 印刷用に各ページを PNG の data URL にレンダリングする */
export async function renderPdfPagesForPrint(bytes: ArrayBuffer): Promise<{ html: string; truncated: boolean; total: number }> {
  const { task } = await openPdfDocument(new Uint8Array(bytes));
  try {
    const pdf = await task.promise;
    const total = pdf.numPages;
    const count = Math.min(total, MAX_PRINT_PAGES);
    const parts: string[] = [];
    for (let i = 1; i <= count; i++) {
      const page = await pdf.getPage(i);
      const viewport = page.getViewport({ scale: 2 });
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.floor(viewport.width));
      canvas.height = Math.max(1, Math.floor(viewport.height));
      await page.render({ canvas, viewport, intent: 'print' }).promise;
      const url = canvas.toDataURL('image/png');
      canvas.width = 0;
      canvas.height = 0;
      page.cleanup();
      parts.push(
        `<div style="page-break-after:always;break-after:page;text-align:center;margin:0;padding:0"><img src="${url}" style="max-width:100%;max-height:100vh;display:block;margin:0 auto" /></div>`,
      );
    }
    return { html: parts.join('\n'), truncated: total > MAX_PRINT_PAGES, total };
  } finally {
    await task.destroy().catch(() => {});
  }
}
