// プレビューの後処理: コードのハイライト（Shiki）と図（Mermaid）。どちらも必要になったときに読み込む

import type { HighlighterGeneric } from 'shiki';
import { tr } from './i18n.svelte';

type Highlighter = HighlighterGeneric<string, string>;

let highlighterPromise: Promise<Highlighter> | null = null;
const codeCache = new Map<string, string>();
const mermaidCache = new Map<string, string>();

function getHighlighter(): Promise<Highlighter> {
  highlighterPromise ??= (async () => {
    const [{ createHighlighter }, { createJavaScriptRegexEngine }] = await Promise.all([
      import('shiki'),
      import('shiki/engine/javascript'),
    ]);
    return (await createHighlighter({
      themes: ['github-light', 'github-dark'],
      langs: [],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    })) as unknown as Highlighter;
  })();
  return highlighterPromise;
}

async function highlight(code: string, lang: string): Promise<string | null> {
  const key = `${lang}\u0000${code}`;
  const cached = codeCache.get(key);
  if (cached) return cached;
  const { bundledLanguages } = await import('shiki');
  if (!(lang in bundledLanguages)) return null;
  const hl = await getHighlighter();
  if (!hl.getLoadedLanguages().includes(lang)) {
    await hl.loadLanguage(lang as never);
  }
  const html = hl.codeToHtml(code, {
    lang,
    themes: { light: 'github-light', dark: 'github-dark' },
    defaultColor: false,
  });
  if (codeCache.size > 500) codeCache.clear();
  codeCache.set(key, html);
  return html;
}

let mermaidSeq = 0;
let mermaidTheme = '';

async function renderMermaid(src: string, dark: boolean): Promise<string> {
  const theme = dark ? 'dark' : 'default';
  const key = `${theme}\u0000${src}`;
  const cached = mermaidCache.get(key);
  if (cached) return cached;
  const mermaid = (await import('mermaid')).default;
  if (mermaidTheme !== theme) {
    mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme, fontFamily: 'inherit' });
    mermaidTheme = theme;
  }
  const { svg } = await mermaid.render(`mermaid-${++mermaidSeq}`, src);
  if (mermaidCache.size > 200) mermaidCache.clear();
  mermaidCache.set(key, svg);
  return svg;
}

/** 描画済みの HTML にハイライトと図を適用する。isCurrent が false になったら中断する */
export async function enhance(container: HTMLElement, dark: boolean, isCurrent: () => boolean): Promise<void> {
  const blocks = Array.from(container.querySelectorAll<HTMLElement>('pre > code[class*="language-"]'));
  for (const code of blocks) {
    if (!isCurrent()) return;
    const lang = /language-([\w#+-]+)/.exec(code.className)?.[1]?.toLowerCase();
    const pre = code.parentElement!;
    if (!lang || lang === 'math') continue;
    try {
      const html = await highlight(code.textContent ?? '', lang);
      if (!html || !isCurrent()) continue;
      const tpl = document.createElement('template');
      tpl.innerHTML = html;
      const el = tpl.content.firstElementChild as HTMLElement | null;
      if (el) {
        if (pre.dataset.line) el.dataset.line = pre.dataset.line;
        pre.replaceWith(el);
      }
    } catch {
      // ハイライトできなくても本文はそのまま見える
    }
  }

  const diagrams = Array.from(container.querySelectorAll<HTMLElement>('.mermaid-block'));
  for (const div of diagrams) {
    if (!isCurrent()) return;
    const src = decodeURIComponent(div.dataset.mermaid ?? '');
    try {
      const svg = await renderMermaid(src, dark);
      if (isCurrent()) div.innerHTML = svg;
    } catch (e) {
      if (!isCurrent()) return;
      div.classList.add('mermaid-error');
      const pre = document.createElement('pre');
      const msg = e instanceof Error ? e.message : String(e);
      pre.textContent = `${tr(`図を描画できません: ${msg}`, `Cannot render diagram: ${msg}`, `无法绘制图表: ${msg}`)}\n\n${src}`;
      div.replaceChildren(pre);
      // Mermaid が失敗時に body に残す要素を消す
      document.querySelectorAll('body > [id^="dmermaid-"]').forEach((n) => n.remove());
    }
  }
}
