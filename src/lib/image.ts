// 画像プレビュー・クリップボードコピー用のユーティリティ

import { api } from './api';
import { tr } from './i18n.svelte';
import { extname } from './paths';

export type ImageBg = 'checker' | 'light' | 'dark';

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;
export const ZOOM_STEPS = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8];

export function clampZoom(z: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}

/** 現在の倍率から一段階拡大・縮小した倍率を返す */
export function stepZoom(current: number, dir: 1 | -1): number {
  const eps = 0.005;
  if (dir > 0) {
    return ZOOM_STEPS.find((s) => s > current + eps) ?? MAX_ZOOM;
  }
  for (let i = ZOOM_STEPS.length - 1; i >= 0; i--) {
    if (ZOOM_STEPS[i] < current - eps) return ZOOM_STEPS[i];
  }
  return MIN_ZOOM;
}

/** バイト数を読みやすい単位にする */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '0 B';
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ['KB', 'MB', 'GB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(value >= 10 ? 1 : 2)} ${units[unit]}`;
}

const FORMAT_LABELS: Record<string, string> = {
  png: 'PNG',
  jpg: 'JPEG',
  jpeg: 'JPEG',
  gif: 'GIF',
  webp: 'WebP',
  svg: 'SVG',
  bmp: 'BMP',
  avif: 'AVIF',
  ico: 'ICO',
};

export function imageFormatLabel(path: string): string {
  const ext = extname(path);
  return FORMAT_LABELS[ext] ?? ext.toUpperCase();
}

/** SVG の文字列を <img> で安全に表示できる data URL にする（<script> などは <img> 内では動かない） */
export function svgToDataUrl(svgText: string): string {
  const bytes = new TextEncoder().encode(svgText);
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return `data:image/svg+xml;base64,${btoa(bin)}`;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () =>
      reject(
        new Error(
          tr('画像を読み込めませんでした', 'Failed to load image', '无法加载图片'),
        ),
      );
    img.src = src;
  });
}

/** 読み込み済みの <img> を PNG の base64 に変換する */
export function renderToPngBase64(img: HTMLImageElement): string {
  const w = img.naturalWidth || img.width || 512;
  const h = img.naturalHeight || img.height || 512;
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('Canvas 2D context is not available');
  ctx.drawImage(img, 0, 0, w, h);
  try {
    const dataUrl = canvas.toDataURL('image/png');
    const prefix = 'data:image/png;base64,';
    if (!dataUrl.startsWith(prefix)) {
      throw new Error(tr('画像を PNG に変換できませんでした', 'Failed to convert image to PNG', '无法将图片转换为 PNG'));
    }
    return dataUrl.slice(prefix.length);
  } catch (e) {
    if (e instanceof DOMException && e.name === 'SecurityError') {
      throw new Error(
        tr(
          'この外部画像はセキュリティ制限によりコピーできません（画像 URL のコピーをご利用ください）',
          'This external image cannot be copied due to security restrictions (try copying the image URL instead)',
          '由于安全限制，无法复制此外部图片（请尝试复制图片地址）',
        ),
      );
    }
    throw e;
  }
}

export async function dataUrlToPngBase64(dataUrl: string): Promise<string> {
  const pngPrefix = 'data:image/png;base64,';
  if (dataUrl.startsWith(pngPrefix)) return dataUrl.slice(pngPrefix.length);
  const img = await loadImage(dataUrl);
  return renderToPngBase64(img);
}

/** ワークスペース内の画像ファイルをクリップボードにコピーする */
export async function copyLocalImageToClipboard(path: string): Promise<void> {
  if (extname(path) === 'png') {
    try {
      await api.copyImageFile(path);
      return;
    } catch {
      // 拡張子が .png でも実体が別形式の場合はデコード経由で PNG 化してコピーする
    }
  }
  const dataUrl = await api.readImageDataUrl(path);
  const b64 = await dataUrlToPngBase64(dataUrl);
  await api.copyImagePng(b64);
}

/** 編集中の SVG テキストを PNG 画像としてクリップボードにコピーする */
export async function copySvgTextToClipboard(svgText: string): Promise<void> {
  const b64 = await dataUrlToPngBase64(svgToDataUrl(svgText));
  await api.copyImagePng(b64);
}

/** Markdown プレビューなどに表示中の <img> 要素をクリップボードにコピーする */
export async function copyImgElementToClipboard(img: HTMLImageElement): Promise<void> {
  if (img.dataset.localPath) {
    await copyLocalImageToClipboard(img.dataset.localPath);
    return;
  }
  const src = img.currentSrc || img.src;
  if (!src) {
    throw new Error(tr('画像がまだ読み込まれていません', 'Image is not loaded yet', '图片尚未加载'));
  }
  if (src.startsWith('data:image/')) {
    const b64 = await dataUrlToPngBase64(src);
    await api.copyImagePng(b64);
    return;
  }
  if (!img.complete || img.naturalWidth === 0) {
    throw new Error(tr('画像がまだ読み込まれていません', 'Image is not loaded yet', '图片尚未加载'));
  }
  const b64 = renderToPngBase64(img);
  await api.copyImagePng(b64);
}
