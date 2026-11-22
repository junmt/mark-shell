import { describe, expect, it } from 'vitest';
import {
  MAX_CANVAS_PIXELS,
  PDF_MAX_ZOOM,
  PDF_MIN_ZOOM,
  clampPage,
  clampPdfZoom,
  computeCanvasOutputScale,
  computeFitWidthScale,
  parsePageInput,
  stepPdfZoom,
} from './pdf';

describe('pdf helpers', () => {
  it('clampPdfZoom / stepPdfZoom', () => {
    expect(clampPdfZoom(0.05)).toBe(PDF_MIN_ZOOM);
    expect(clampPdfZoom(10)).toBe(PDF_MAX_ZOOM);
    expect(stepPdfZoom(1, 1)).toBe(1.1);
    expect(stepPdfZoom(1, -1)).toBe(0.9);
    expect(stepPdfZoom(1.15, 1)).toBe(1.25);
    expect(stepPdfZoom(1.15, -1)).toBe(1.1);
    expect(stepPdfZoom(PDF_MAX_ZOOM, 1)).toBe(PDF_MAX_ZOOM);
    expect(stepPdfZoom(PDF_MIN_ZOOM, -1)).toBe(PDF_MIN_ZOOM);
  });

  it('computeFitWidthScale', () => {
    expect(computeFitWidthScale(595, 643, 48)).toBeCloseTo(1, 5);
    expect(computeFitWidthScale(600, 1248, 48)).toBeCloseTo(2, 5);
    expect(computeFitWidthScale(0, 800)).toBe(1);
    expect(computeFitWidthScale(600, 0)).toBe(1);
    // 非常に大きいコンテナでも上限でクランプされる
    expect(computeFitWidthScale(200, 5000)).toBe(PDF_MAX_ZOOM);
  });

  it('clampPage / parsePageInput', () => {
    expect(clampPage(0, 10)).toBe(1);
    expect(clampPage(15, 10)).toBe(10);
    expect(clampPage(3.9, 10)).toBe(3);
    expect(clampPage(5, 0)).toBe(1);

    expect(parsePageInput(' 4 ', 10)).toBe(4);
    expect(parsePageInput('99', 10)).toBe(10);
    expect(parsePageInput('0', 10)).toBeNull();
    expect(parsePageInput('-1', 10)).toBeNull();
    expect(parsePageInput('abc', 10)).toBeNull();
    expect(parsePageInput('', 10)).toBeNull();
  });

  it('computeCanvasOutputScale respects dpr and pixel budget', () => {
    expect(computeCanvasOutputScale(600, 800, 2)).toBe(2);
    expect(computeCanvasOutputScale(600, 800, 4)).toBe(3);
    expect(computeCanvasOutputScale(600, 800, 0)).toBe(1);
    // 巨大な CSS サイズでは MAX_CANVAS_PIXELS 内に収まるよう抑える
    const scale = computeCanvasOutputScale(4000, 4000, 2);
    expect(4000 * scale * (4000 * scale)).toBeLessThanOrEqual(MAX_CANVAS_PIXELS + 1);
  });
});
