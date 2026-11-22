// HTML ファイルのプレビュー用前処理。
// sandbox="allow-same-origin"（allow-scripts なし）の iframe srcdoc で安全に表示するため、
// スクリプト・危険な要素・外部通信を除去し、ローカル画像・ローカル CSS・リンクをワークスペース内で解決する。

import { MDIMG_BASE, mdimgUrl, resolveLocal } from './markdown';
import { tr } from './i18n.svelte';

export type HtmlRenderContext = {
  root: string;
  filePath: string;
  externalImages: 'click' | 'always';
  /** ワークスペース内のテキストファイル（ローカル CSS など）を読む関数 */
  readText?: (rel: string) => Promise<string | null>;
};

export type HtmlRenderResult = {
  srcdoc: string;
  hasScripts: boolean;
  bodyHtml: string;
};

const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
const DANGEROUS_URL_RE = /^\s*(?:javascript|vbscript):/i;
const FORBIDDEN_TAGS = ['script', 'object', 'embed', 'applet', 'base', 'iframe', 'frame', 'frameset'];

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** CSS 内の @import（外部 URL）を除去し、url(...) のローカル参照を mdimg に書き換える */
export function rewriteCssUrls(
  css: string,
  baseFilePath: string,
  root: string,
  externalImages: 'click' | 'always',
): string {
  // 外部ネットワークアクセスを防ぐため @import は除去する
  const withoutImport = css.replace(/@import\s+(?:url\([^)]*\)|['"][^'"]*['"])[^;]*;?/gi, '');
  return withoutImport.replace(/url\(\s*(['"]?)([^'")]+)\1\s*\)/gi, (_m, _q, raw: string) => {
    const target = raw.trim();
    if (!target || target.startsWith('#') || /^data:/i.test(target) || /^mdimg:/i.test(target) || target.startsWith(MDIMG_BASE)) {
      return `url("${target}")`;
    }
    if (DANGEROUS_URL_RE.test(target)) return 'none';
    if (/^https?:\/\//i.test(target) || target.startsWith('//')) {
      return externalImages === 'always' ? `url("${target}")` : 'none';
    }
    const rel = resolveLocal(target, baseFilePath, root);
    if (rel === null) return 'none';
    return `url("${mdimgUrl(rel)}")`;
  });
}

function rewriteSrcset(
  srcset: string,
  filePath: string,
  root: string,
  externalImages: 'click' | 'always',
): string | null {
  const parts: string[] = [];
  for (const item of srcset.split(',')) {
    const trimmed = item.trim();
    if (!trimmed) continue;
    const m = /^(\S+)(\s+.*)?$/.exec(trimmed);
    if (!m) continue;
    const [, url, desc = ''] = m;
    if (/^data:image\//i.test(url)) {
      parts.push(`${url}${desc}`);
    } else if (/^https?:\/\//i.test(url) || url.startsWith('//')) {
      if (externalImages === 'always') parts.push(`${url}${desc}`);
    } else {
      const rel = resolveLocal(url, filePath, root);
      if (rel !== null) parts.push(`${mdimgUrl(rel)}${desc}`);
    }
  }
  return parts.length > 0 ? parts.join(', ') : null;
}

const PREVIEW_HELPER_CSS = `
html { background: #ffffff; color: #1f2328; }
img.mark-shell-external-image, img.mark-shell-blocked-image {
  display: inline-block;
  min-width: 160px;
  min-height: 60px;
  border: 1px dashed #d0d7de;
  border-radius: 6px;
  background: #f6f8fa;
}
img.mark-shell-external-image { cursor: pointer; }
`;

/** HTML テキストを安全な iframe srcdoc 用ドキュメントに変換する */
export async function prepareHtmlPreview(html: string, ctx: HtmlRenderContext): Promise<HtmlRenderResult> {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  const scripts = Array.from(doc.querySelectorAll('script'));
  const hasScripts = scripts.length > 0;

  for (const tag of FORBIDDEN_TAGS) {
    for (const el of Array.from(doc.querySelectorAll(tag))) el.remove();
  }

  for (const meta of Array.from(doc.querySelectorAll('meta[http-equiv]'))) {
    const eq = (meta.getAttribute('http-equiv') ?? '').toLowerCase();
    if (eq === 'refresh' || eq === 'content-security-policy') meta.remove();
  }

  // インライン <style> 内の url(...) を解決する（<link> 展開より先に行う）
  for (const style of Array.from(doc.querySelectorAll('style'))) {
    if (style.textContent) {
      style.textContent = rewriteCssUrls(style.textContent, ctx.filePath, ctx.root, ctx.externalImages);
    }
  }

  // ローカルの <link rel="stylesheet"> はワークスペース内から読み込んで <style> に展開し、その他の <link> は除去する
  const links = Array.from(doc.querySelectorAll('link'));
  for (const link of links) {
    const relAttr = (link.getAttribute('rel') ?? '').toLowerCase().split(/\s+/);
    const href = (link.getAttribute('href') ?? '').trim();
    if (
      relAttr.includes('stylesheet') &&
      href &&
      !/^https?:\/\//i.test(href) &&
      !href.startsWith('//') &&
      !DANGEROUS_URL_RE.test(href) &&
      ctx.readText
    ) {
      const resolved = resolveLocal(href, ctx.filePath, ctx.root);
      if (resolved !== null) {
        const css = await ctx.readText(resolved).catch(() => null);
        if (css !== null) {
          const style = doc.createElement('style');
          style.textContent = rewriteCssUrls(css, resolved, ctx.root, ctx.externalImages);
          link.replaceWith(style);
          continue;
        }
      }
    }
    link.remove();
  }

  // 全要素のイベント属性・危険な URL・インライン style をサニタイズ
  for (const el of Array.from(doc.querySelectorAll('*'))) {
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      if (name.startsWith('on')) {
        el.removeAttribute(attr.name);
        continue;
      }
      if (name === 'style') {
        el.setAttribute('style', rewriteCssUrls(attr.value, ctx.filePath, ctx.root, ctx.externalImages));
        continue;
      }
      if ((name === 'href' || name === 'src' || name === 'action' || name === 'formaction' || name === 'xlink:href') && DANGEROUS_URL_RE.test(attr.value)) {
        el.removeAttribute(attr.name);
      }
    }
  }

  // 画像参照の解決
  for (const img of Array.from(doc.querySelectorAll('img'))) {
    const srcset = img.getAttribute('srcset');
    if (srcset) {
      const rewritten = rewriteSrcset(srcset, ctx.filePath, ctx.root, ctx.externalImages);
      if (rewritten) img.setAttribute('srcset', rewritten);
      else img.removeAttribute('srcset');
    }
    const src = img.getAttribute('src');
    if (!src) continue;
    if (/^https?:\/\//i.test(src) || src.startsWith('//')) {
      if (ctx.externalImages !== 'always') {
        img.setAttribute('data-src', src);
        img.removeAttribute('src');
        img.classList.add('mark-shell-external-image');
        img.title = tr(
          `外部の画像（クリックで読み込み）: ${src}`,
          `External image (click to load): ${src}`,
          `外部图片（点击加载）: ${src}`,
        );
      }
    } else if (!/^data:image\//i.test(src)) {
      const rel = resolveLocal(src, ctx.filePath, ctx.root);
      if (rel === null) {
        img.removeAttribute('src');
        img.classList.add('mark-shell-blocked-image');
        img.title = tr(
          'フォルダの外の画像は表示しません',
          'Images outside the folder are not shown',
          '不显示文件夹以外的图片',
        );
      } else {
        img.setAttribute('src', mdimgUrl(rel));
        img.setAttribute('data-local-path', rel);
      }
    }
  }

  // <source srcset> の解決
  for (const source of Array.from(doc.querySelectorAll('source[srcset]'))) {
    const srcset = source.getAttribute('srcset');
    if (!srcset) continue;
    const rewritten = rewriteSrcset(srcset, ctx.filePath, ctx.root, ctx.externalImages);
    if (rewritten) source.setAttribute('srcset', rewritten);
    else source.removeAttribute('srcset');
  }

  // リンクの解決（クリック処理は HtmlPreview 側で行う）
  for (const a of Array.from(doc.querySelectorAll('a[href]'))) {
    const href = (a.getAttribute('href') ?? '').trim();
    a.removeAttribute('target');
    if (!href) continue;
    if (href.startsWith('#')) {
      a.setAttribute('data-anchor', safeDecode(href.slice(1)));
    } else if (SCHEME_RE.test(href) && !/^[a-zA-Z]:[\\/]/.test(href)) {
      a.setAttribute('data-external', href);
    } else {
      const rel = resolveLocal(href, ctx.filePath, ctx.root);
      const hash = href.includes('#') ? href.slice(href.indexOf('#')) : '';
      if (rel !== null) a.setAttribute('data-href', rel + safeDecode(hash));
    }
  }

  // 既定の背景とプレースホルダー用スタイルを先頭に追加（HTML 自身のスタイルが優先されるように先頭へ置く）
  const helper = doc.createElement('style');
  helper.textContent = PREVIEW_HELPER_CSS;
  doc.head.prepend(helper);

  return {
    srcdoc: `<!DOCTYPE html>\n${doc.documentElement.outerHTML}`,
    hasScripts,
    bodyHtml: doc.body.innerHTML,
  };
}
