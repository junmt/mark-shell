// Markdown → HTML。CommonMark + GFM + 数式 + Mermaid + [[内部リンク]] + GitHub 形式のアラート

import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import remarkFrontmatter from 'remark-frontmatter';
import remarkRehype from 'remark-rehype';
import rehypeRaw from 'rehype-raw';
import rehypeKatex from 'rehype-katex';
import rehypeStringify from 'rehype-stringify';
import { visit, SKIP } from 'unist-util-visit';
import DOMPurify from 'dompurify';
import YAML from 'yaml';
import type { Root as MdRoot, Text, PhrasingContent, Blockquote, Paragraph } from 'mdast';
import type { Root as HRoot, Element, ElementContent } from 'hast';
import { absToRel, dirname, join, normalize } from './paths';

export type RenderContext = {
  root: string;
  notePath: string;
  externalImages: 'click' | 'always';
};

let ctx: RenderContext = { root: '', notePath: '', externalImages: 'click' };

const WIKI_RE = /\[\[([^[\]\n|#]+)(#[^[\]\n|]*)?(?:\|([^[\]\n]+))?\]\]/g;

function remarkWikiLinks() {
  return (tree: MdRoot) => {
    visit(tree, 'text', (node: Text, index, parent) => {
      if (!parent || index === undefined || parent.type === 'link' || !node.value.includes('[[')) return;
      const parts: PhrasingContent[] = [];
      let last = 0;
      for (const m of node.value.matchAll(WIKI_RE)) {
        const start = m.index!;
        if (start > last) parts.push({ type: 'text', value: node.value.slice(last, start) });
        const target = m[1].trim();
        const hash = m[2] ?? '';
        parts.push({
          type: 'link',
          url: '#',
          data: { hProperties: { dataWiki: target + hash, className: ['wiki-link'] } },
          children: [{ type: 'text', value: m[3]?.trim() || target + (hash ? hash : '') }],
        });
        last = start + m[0].length;
      }
      if (parts.length === 0) return;
      if (last < node.value.length) parts.push({ type: 'text', value: node.value.slice(last) });
      parent.children.splice(index, 1, ...(parts as never[]));
      return [SKIP, index + parts.length];
    });
  };
}

const ALERTS: Record<string, string> = {
  note: 'Note',
  tip: 'Tip',
  important: 'Important',
  warning: 'Warning',
  caution: 'Caution',
};

function remarkAlerts() {
  return (tree: MdRoot) => {
    visit(tree, 'blockquote', (node: Blockquote) => {
      const first = node.children[0] as Paragraph | undefined;
      if (first?.type !== 'paragraph') return;
      const text = first.children[0];
      if (text?.type !== 'text') return;
      const m = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*\n?/i.exec(text.value);
      if (!m) return;
      const kind = m[1].toLowerCase();
      text.value = text.value.slice(m[0].length);
      if (text.value === '' && first.children.length === 1) node.children.shift();
      node.data = { hName: 'div', hProperties: { className: ['markdown-alert', `markdown-alert-${kind}`] } };
      node.children.unshift({
        type: 'paragraph',
        data: { hProperties: { className: ['markdown-alert-title'] } },
        children: [{ type: 'text', value: ALERTS[kind] }],
      });
    });
  };
}

function remarkMermaid() {
  return (tree: MdRoot) => {
    visit(tree, 'code', (node, index, parent) => {
      if (node.lang !== 'mermaid' || !parent || index === undefined) return;
      parent.children[index] = {
        type: 'mermaidBlock',
        data: {
          hName: 'div',
          // DOMPurify は `-->` を含む属性を消すのでエンコードして持たせる
          hProperties: { className: ['mermaid-block'], dataMermaid: encodeURIComponent(node.value) },
        },
        position: node.position,
      } as never;
    });
  };
}

const BLOCK_TAGS = new Set([
  'p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'li', 'pre', 'blockquote', 'table', 'tr', 'hr', 'div', 'details', 'img',
]);

export function mdimgUrl(rel: string): string {
  return 'http://mdimg.localhost/' + rel.split('/').map(encodeURIComponent).join('/');
}

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

/** ノートからの相対パス・絶対パスを、ルートからの相対パスに。ルートの外は null */
export function resolveLocal(src: string, notePath: string, root: string): string | null {
  const clean = safeDecode(src.split('#')[0].split('?')[0]);
  if (/^[a-zA-Z]:[\\/]/.test(clean)) return absToRel(root, clean);
  if (/^file:/i.test(clean)) return absToRel(root, clean.replace(/^file:\/*/i, ''));
  if (clean.startsWith('/')) return normalize(clean);
  return normalize(join(dirname(notePath), clean));
}

const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;

export function slugify(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[\s]+/g, '-')
    .replace(/[!"#$%&'()*+,./:;<=>?@[\\\]^`{|}~、。「」『』（）！？：；]/g, '');
}

function textOf(node: ElementContent | Element): string {
  if (node.type === 'text') return node.value;
  if (node.type === 'element') return node.children.map(textOf).join('');
  return '';
}

function rehypeMarkShell() {
  return (tree: HRoot) => {
    const slugs = new Map<string, number>();
    visit(tree, 'element', (node: Element) => {
      const p = node.properties;
      if (node.position && BLOCK_TAGS.has(node.tagName)) p.dataLine = node.position.start.line;

      if (/^h[1-6]$/.test(node.tagName) && !p.id) {
        const base = slugify(textOf(node));
        const n = slugs.get(base) ?? 0;
        slugs.set(base, n + 1);
        p.id = n === 0 ? base : `${base}-${n}`;
      }

      if (node.tagName === 'input' && p.type === 'checkbox') {
        delete p.disabled;
        p.className = ['task-checkbox'];
      }

      if (node.tagName === 'img' && typeof p.src === 'string') {
        const src = p.src;
        if (/^https?:\/\//i.test(src) || src.startsWith('//')) {
          if (ctx.externalImages !== 'always') {
            p.dataSrc = src;
            delete p.src;
            p.className = ['external-image'];
            p.title = `外部の画像（クリックで読み込み）: ${src}`;
          }
        } else if (!/^data:image\//i.test(src)) {
          const rel = resolveLocal(src, ctx.notePath, ctx.root);
          if (rel === null) {
            delete p.src;
            p.className = ['blocked-image'];
            p.title = 'フォルダの外の画像は表示しません';
          } else {
            p.src = mdimgUrl(rel);
          }
        }
      }

      if (node.tagName === 'a' && typeof p.href === 'string' && !p.dataWiki) {
        const href = p.href;
        if (href.startsWith('#')) {
          p.dataAnchor = safeDecode(href.slice(1));
        } else if (SCHEME_RE.test(href) && !/^[a-zA-Z]:[\\/]/.test(href)) {
          p.dataExternal = href;
        } else {
          const rel = resolveLocal(href, ctx.notePath, ctx.root);
          const hash = href.includes('#') ? href.slice(href.indexOf('#')) : '';
          if (rel !== null) p.dataHref = rel + safeDecode(hash);
        }
      }
    });
  };
}

const processor = unified()
  .use(remarkParse)
  .use(remarkFrontmatter, ['yaml'])
  .use(remarkGfm)
  .use(remarkMath)
  .use(remarkWikiLinks)
  .use(remarkAlerts)
  .use(remarkMermaid)
  .use(remarkRehype, { allowDangerousHtml: true })
  .use(rehypeRaw)
  .use(rehypeKatex, { throwOnError: false, strict: false })
  .use(rehypeMarkShell)
  .use(rehypeStringify);

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
}

function frontmatterHtml(yaml: string): string {
  let data: unknown;
  try {
    data = YAML.parse(yaml);
  } catch {
    return `<details class="frontmatter"><summary>フロントマター（解析できません）</summary><pre>${escapeHtml(yaml)}</pre></details>`;
  }
  if (!data || typeof data !== 'object') return '';
  const rows = Object.entries(data as Record<string, unknown>)
    .map(([k, v]) => {
      const val = Array.isArray(v) ? v.join(', ') : typeof v === 'object' && v !== null ? JSON.stringify(v) : String(v ?? '');
      return `<tr><th>${escapeHtml(k)}</th><td>${escapeHtml(val)}</td></tr>`;
    })
    .join('');
  return `<details class="frontmatter" data-line="1"><summary>フロントマター</summary><table>${rows}</table></details>`;
}

const PURIFY_CONFIG = {
  ADD_ATTR: ['data-line', 'data-mermaid', 'data-src', 'data-wiki', 'data-href', 'data-external', 'data-anchor', 'target'],
  FORBID_TAGS: ['style', 'iframe', 'form', 'object', 'embed', 'button', 'textarea', 'select', 'link', 'meta', 'base'],
  FORBID_ATTR: ['onerror', 'onload', 'onclick', 'formaction'],
};

export function renderMarkdown(text: string, context: RenderContext): string {
  ctx = context;
  const mdast = processor.parse(text) as MdRoot;
  let front = '';
  const first = mdast.children[0];
  if (first?.type === 'yaml') front = frontmatterHtml(first.value);
  const hast = processor.runSync(mdast);
  const html = processor.stringify(hast as HRoot);
  return front + (DOMPurify.sanitize(html, PURIFY_CONFIG) as unknown as string);
}

/** 見出しの一覧（アウトライン用）。コードブロック内は無視する */
export function headings(text: string): { level: number; text: string; line: number }[] {
  const out: { level: number; text: string; line: number }[] = [];
  let fence: string | null = null;
  const lines = text.split('\n');
  let start = 0;
  if (lines[0] === '---') {
    const end = lines.indexOf('---', 1);
    if (end > 0) start = end + 1;
  }
  for (let i = start; i < lines.length; i++) {
    const line = lines[i];
    const f = /^\s{0,3}(`{3,}|~{3,})/.exec(line);
    if (f) {
      if (fence === null) fence = f[1][0];
      else if (f[1][0] === fence) fence = null;
      continue;
    }
    if (fence !== null) continue;
    const m = /^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (m) out.push({ level: m[1].length, text: m[2], line: i + 1 });
  }
  return out;
}
