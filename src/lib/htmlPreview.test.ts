import { describe, expect, it } from 'vitest';
import { prepareHtmlPreview, rewriteCssUrls } from './htmlPreview';
import { MDIMG_BASE } from './markdown';

describe('htmlPreview', () => {
  const baseCtx = { root: 'C:\\Notes', filePath: 'pages/demo.html', externalImages: 'click' as const };

  it('スクリプト・危険な要素・イベント属性を除去して hasScripts を返す', async () => {
    const res = await prepareHtmlPreview(
      `<!DOCTYPE html>
      <html>
        <head>
          <meta http-equiv="refresh" content="0;url=https://evil.example">
          <script>alert(1)</script>
        </head>
        <body>
          <button onclick="alert(1)">OK</button>
          <a href="javascript:alert(1)">bad</a>
          <iframe src="https://evil.example"></iframe>
          <p>safe</p>
        </body>
      </html>`,
      baseCtx,
    );
    expect(res.hasScripts).toBe(true);
    const doc = new DOMParser().parseFromString(res.srcdoc, 'text/html');
    expect(doc.querySelector('script')).toBeNull();
    expect(doc.querySelector('iframe')).toBeNull();
    expect(doc.querySelector('meta[http-equiv="refresh"]')).toBeNull();
    expect(doc.querySelector('button')?.getAttribute('onclick')).toBeNull();
    expect(doc.querySelector('a')?.getAttribute('href')).toBeNull();
    expect(doc.querySelector('p')?.textContent).toBe('safe');
  });

  it('画像をローカル・外部・ルート外で適切に解決する', async () => {
    const res = await prepareHtmlPreview(
      `<img src="img/a b.png"><img src="https://example.com/x.png"><img src="../../outside.png">`,
      baseCtx,
    );
    expect(res.hasScripts).toBe(false);
    const doc = new DOMParser().parseFromString(res.srcdoc, 'text/html');
    const imgs = doc.querySelectorAll('img');
    expect(imgs[0].getAttribute('src')).toBe(MDIMG_BASE + 'pages/img/a%20b.png');
    expect(imgs[0].getAttribute('data-local-path')).toBe('pages/img/a b.png');
    expect(imgs[1].getAttribute('src')).toBeNull();
    expect(imgs[1].getAttribute('data-src')).toBe('https://example.com/x.png');
    expect(imgs[1].classList.contains('mark-shell-external-image')).toBe(true);
    expect(imgs[2].getAttribute('src')).toBeNull();
    expect(imgs[2].classList.contains('mark-shell-blocked-image')).toBe(true);
  });

  it('ローカル CSS を展開して url(...) を解決し、外部 link は除去する', async () => {
    const files: Record<string, string> = {
      'pages/styles/main.css': 'body { background: url("../img/bg.png"); } @import url("https://evil.example/a.css");',
    };
    const res = await prepareHtmlPreview(
      `<head>
        <link rel="stylesheet" href="styles/main.css">
        <link rel="stylesheet" href="https://cdn.example/a.css">
      </head><body>hi</body>`,
      { ...baseCtx, readText: async (rel) => files[rel] ?? null },
    );
    const doc = new DOMParser().parseFromString(res.srcdoc, 'text/html');
    expect(doc.querySelectorAll('link')).toHaveLength(0);
    const styles = Array.from(doc.querySelectorAll('style')).map((s) => s.textContent ?? '').join('\n');
    expect(styles).toContain(MDIMG_BASE + 'pages/img/bg.png');
    expect(styles).not.toContain('evil.example');
  });

  it('リンクをページ内・ローカル・外部に分類する', async () => {
    const res = await prepareHtmlPreview(
      `<a href="#sec">a</a><a href="../note.md#top">b</a><a href="https://example.com">c</a>`,
      baseCtx,
    );
    const doc = new DOMParser().parseFromString(res.srcdoc, 'text/html');
    const [a, b, c] = doc.querySelectorAll('a');
    expect(a.getAttribute('data-anchor')).toBe('sec');
    expect(b.getAttribute('data-href')).toBe('note.md#top');
    expect(c.getAttribute('data-external')).toBe('https://example.com');
  });

  it('rewriteCssUrls は外部画像設定に従う', () => {
    expect(rewriteCssUrls('background: url(https://x.test/a.png);', 'a.html', 'C:\\Notes', 'click')).toBe('background: none;');
    expect(rewriteCssUrls('background: url(https://x.test/a.png);', 'a.html', 'C:\\Notes', 'always')).toBe(
      'background: url("https://x.test/a.png");',
    );
  });
});
