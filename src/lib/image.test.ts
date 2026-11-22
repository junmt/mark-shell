import { describe, expect, it } from 'vitest';
import { clampZoom, formatBytes, imageFormatLabel, stepZoom, svgToDataUrl } from './image';

describe('image helpers', () => {
  it('formatBytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1024)).toBe('1.00 KB');
    expect(formatBytes(15 * 1024)).toBe('15.0 KB');
    expect(formatBytes(256 * 1024)).toBe('256 KB');
    expect(formatBytes(2.5 * 1024 * 1024)).toBe('2.50 MB');
  });

  it('imageFormatLabel', () => {
    expect(imageFormatLabel('assets/photo.JPG')).toBe('JPEG');
    expect(imageFormatLabel('icon.svg')).toBe('SVG');
    expect(imageFormatLabel('a/b.webp')).toBe('WebP');
    expect(imageFormatLabel('a/b.png')).toBe('PNG');
  });

  it('stepZoom / clampZoom', () => {
    expect(stepZoom(1, 1)).toBe(1.25);
    expect(stepZoom(1, -1)).toBe(0.75);
    expect(stepZoom(0.42, 1)).toBe(0.5);
    expect(stepZoom(0.42, -1)).toBe(0.33);
    expect(clampZoom(0.01)).toBe(0.1);
    expect(clampZoom(100)).toBe(8);
  });

  it('svgToDataUrl encodes UTF-8 SVG cleanly', () => {
    const svg = '<svg xmlns="http://www.w3.org/2000/svg"><text>日本語</text></svg>';
    const url = svgToDataUrl(svg);
    expect(url.startsWith('data:image/svg+xml;base64,')).toBe(true);
    const decoded = new TextDecoder().decode(Uint8Array.from(atob(url.split(',')[1]), (c) => c.charCodeAt(0)));
    expect(decoded).toBe(svg);
  });
});
