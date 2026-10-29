// CodeMirror 6 の設定と Markdown 用の編集コマンド

import { Compartment, EditorSelection, EditorState, Prec, type Extension } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor,
  highlightSpecialChars,
  placeholder,
  type Command,
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { yamlFrontmatter } from '@codemirror/lang-yaml';
import { languages } from '@codemirror/language-data';
import { syntaxHighlighting, HighlightStyle, indentOnInput, bracketMatching, indentUnit, LanguageDescription } from '@codemirror/language';
import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete';
import { tags as t } from '@lezer/highlight';
import { basename, dirname, encodeLinkTarget, relative, stem } from './paths';
import { tr } from './i18n.svelte';
import { conflictMarkers, gitGutter } from './gitEditor';

export const lineNumbersComp = new Compartment();
export const wrapComp = new Compartment();
export const readOnlyComp = new Compartment();
export const fontComp = new Compartment();

// 色は CSS 変数で指定し、ライト / ダークは CSS 側で切り替える
const highlightStyle = HighlightStyle.define([
  { tag: t.heading1, fontWeight: '700', fontSize: '1.45em', color: 'var(--md-heading)' },
  { tag: t.heading2, fontWeight: '700', fontSize: '1.3em', color: 'var(--md-heading)' },
  { tag: t.heading3, fontWeight: '700', fontSize: '1.15em', color: 'var(--md-heading)' },
  { tag: [t.heading4, t.heading5, t.heading6], fontWeight: '700', color: 'var(--md-heading)' },
  { tag: t.strong, fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: [t.link, t.url], color: 'var(--md-link)' },
  { tag: t.monospace, fontFamily: 'var(--editor-mono)', color: 'var(--md-code)' },
  { tag: [t.processingInstruction, t.meta], color: 'var(--md-meta)' },
  { tag: t.quote, color: 'var(--md-quote)' },
  { tag: t.contentSeparator, color: 'var(--md-meta)' },
  { tag: [t.keyword, t.operatorKeyword, t.modifier], color: 'var(--code-keyword)' },
  { tag: [t.string, t.special(t.string), t.regexp], color: 'var(--code-string)' },
  { tag: [t.comment, t.lineComment, t.blockComment], color: 'var(--code-comment)', fontStyle: 'italic' },
  { tag: [t.number, t.bool, t.null, t.atom], color: 'var(--code-number)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'var(--code-function)' },
  { tag: [t.typeName, t.className, t.tagName], color: 'var(--code-type)' },
  { tag: [t.propertyName, t.attributeName], color: 'var(--code-property)' },
  { tag: [t.self, t.special(t.variableName)], color: 'var(--code-keyword)' },
  { tag: [t.escape, t.special(t.string)], color: 'var(--code-number)' },
  { tag: [t.definition(t.function(t.variableName)), t.definition(t.typeName)], color: 'var(--code-function)' },
  { tag: t.invalid, color: 'var(--code-keyword)', textDecoration: 'underline wavy' },
]);

const baseTheme = EditorView.theme({
  '&': { height: '100%', backgroundColor: 'var(--bg)', color: 'var(--fg)' },
  '.cm-scroller': { fontFamily: 'var(--editor-font)', lineHeight: '1.7' },
  '.cm-content': { padding: '16px 0 40vh', caretColor: 'var(--fg)' },
  '.cm-line': { padding: '0 24px' },
  '.cm-gutters': { backgroundColor: 'var(--bg)', color: 'var(--fg-muted)', border: 'none' },
  '.cm-activeLine': { backgroundColor: 'var(--active-line)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--active-line)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--selection) !important',
  },
  '.cm-cursor': { borderLeftColor: 'var(--fg)' },
  '.cm-panels': { backgroundColor: 'var(--bg-alt)', color: 'var(--fg)' },
  '.cm-searchMatch': { backgroundColor: 'var(--search-match)' },
  '.cm-tooltip': { backgroundColor: 'var(--bg-alt)', border: '1px solid var(--border)' },
});

/** 全角文字は幅 2 として数える（表の整形用） */
export function displayWidth(s: string): number {
  let w = 0;
  for (const ch of s) {
    const c = ch.codePointAt(0)!;
    w +=
      (c >= 0x1100 && c <= 0x115f) ||
      (c >= 0x2e80 && c <= 0xa4cf) ||
      (c >= 0xac00 && c <= 0xd7a3) ||
      (c >= 0xf900 && c <= 0xfaff) ||
      (c >= 0xfe30 && c <= 0xfe4f) ||
      (c >= 0xff00 && c <= 0xff60) ||
      (c >= 0xffe0 && c <= 0xffe6) ||
      (c >= 0x1f300 && c <= 0x1faff) ||
      (c >= 0x20000 && c <= 0x3fffd)
        ? 2
        : 1;
  }
  return w;
}

/** 選択範囲を記号で囲む（既に囲まれていれば外す） */
export function wrapWith(marker: string): Command {
  return (view) => {
    const changes = view.state.changeByRange((range) => {
      const text = view.state.sliceDoc(range.from, range.to);
      const before = view.state.sliceDoc(range.from - marker.length, range.from);
      const after = view.state.sliceDoc(range.to, range.to + marker.length);
      if (before === marker && after === marker) {
        return {
          changes: [
            { from: range.from - marker.length, to: range.from, insert: '' },
            { from: range.to, to: range.to + marker.length, insert: '' },
          ],
          range: EditorSelection.range(range.from - marker.length, range.to - marker.length),
        };
      }
      if (text.startsWith(marker) && text.endsWith(marker) && text.length >= marker.length * 2) {
        const inner = text.slice(marker.length, text.length - marker.length);
        return {
          changes: { from: range.from, to: range.to, insert: inner },
          range: EditorSelection.range(range.from, range.from + inner.length),
        };
      }
      return {
        changes: { from: range.from, to: range.to, insert: marker + text + marker },
        range: EditorSelection.range(range.from + marker.length, range.to + marker.length),
      };
    });
    view.dispatch(view.state.update(changes, { scrollIntoView: true, userEvent: 'input' }));
    return true;
  };
}

export const insertLink: Command = (view) => {
  const changes = view.state.changeByRange((range) => {
    const text = view.state.sliceDoc(range.from, range.to);
    const isUrl = /^https?:\/\/\S+$/.test(text);
    const insert = isUrl ? `[](${text})` : `[${text}](url)`;
    const sel = isUrl
      ? EditorSelection.cursor(range.from + 1)
      : EditorSelection.range(range.from + text.length + 3, range.from + text.length + 6);
    return { changes: { from: range.from, to: range.to, insert }, range: sel };
  });
  view.dispatch(view.state.update(changes, { scrollIntoView: true, userEvent: 'input' }));
  return true;
};

export const insertCodeBlock: Command = (view) => {
  const { state } = view;
  const range = state.selection.main;
  const fromLine = state.doc.lineAt(range.from);
  const toLine = state.doc.lineAt(range.to);
  const body = state.sliceDoc(fromLine.from, toLine.to);
  const insert = '```\n' + body + (body ? '\n' : '') + '```';
  view.dispatch({
    changes: { from: fromLine.from, to: toLine.to, insert },
    selection: EditorSelection.cursor(fromLine.from + 3),
    scrollIntoView: true,
    userEvent: 'input',
  });
  return true;
};

const TASK_RE = /^(\s*(?:[-*+]|\d+[.)])\s+)\[( |x|X)\]/;
const LIST_RE = /^(\s*(?:[-*+]|\d+[.)])\s+)/;

export function toggleTaskLine(line: string): string {
  const m = TASK_RE.exec(line);
  if (m) return m[1] + (m[2] === ' ' ? '[x]' : '[ ]') + line.slice(m[0].length);
  const l = LIST_RE.exec(line);
  if (l) return l[1] + '[ ] ' + line.slice(l[0].length);
  const indent = /^\s*/.exec(line)![0];
  return indent + '- [ ] ' + line.slice(indent.length);
}

export const toggleTask: Command = (view) => {
  const { state } = view;
  const lines = new Set<number>();
  for (const r of state.selection.ranges) {
    for (let n = state.doc.lineAt(r.from).number; n <= state.doc.lineAt(r.to).number; n++) lines.add(n);
  }
  const changes = [...lines].map((n) => {
    const line = state.doc.line(n);
    return { from: line.from, to: line.to, insert: toggleTaskLine(line.text) };
  });
  view.dispatch({ changes, userEvent: 'input' });
  return true;
};

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith('|')) s = s.slice(1);
  if (s.endsWith('|') && !s.endsWith('\\|')) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = '';
  let code = false;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === '\\' && s[i + 1] === '|') {
      cur += '\\|';
      i++;
    } else if (ch === '`') {
      code = !code;
      cur += ch;
    } else if (ch === '|' && !code) {
      cells.push(cur.trim());
      cur = '';
    } else {
      cur += ch;
    }
  }
  cells.push(cur.trim());
  return cells;
}

/** 表の列幅をそろえる（全角は幅 2） */
export function formatTableText(lines: string[]): string[] | null {
  if (lines.length < 2) return null;
  const rows = lines.map(splitRow);
  const delim = rows[1];
  if (!delim.every((c) => /^:?-+:?$/.test(c))) return null;
  const cols = Math.max(...rows.map((r) => r.length));
  const align = Array.from({ length: cols }, (_, i) => {
    const c = delim[i] ?? '---';
    return c.startsWith(':') && c.endsWith(':') ? 'c' : c.endsWith(':') ? 'r' : c.startsWith(':') ? 'l' : '';
  });
  const width = Array.from({ length: cols }, (_, i) =>
    Math.max(3, ...rows.filter((_, r) => r !== 1).map((r) => displayWidth(r[i] ?? ''))),
  );
  const pad = (s: string, w: number, a: string) => {
    const gap = w - displayWidth(s);
    if (a === 'r') return ' '.repeat(gap) + s;
    if (a === 'c') return ' '.repeat(Math.floor(gap / 2)) + s + ' '.repeat(Math.ceil(gap / 2));
    return s + ' '.repeat(gap);
  };
  return rows.map((r, ri) => {
    const cells = Array.from({ length: cols }, (_, i) => {
      if (ri === 1) {
        const a = align[i];
        const dash = '-'.repeat(width[i] - (a === 'c' ? 2 : a ? 1 : 0));
        return a === 'c' ? `:${dash}:` : a === 'r' ? `${dash}:` : a === 'l' ? `:${dash}` : dash;
      }
      return pad(r[i] ?? '', width[i], align[i]);
    });
    return `| ${cells.join(' | ')} |`;
  });
}

export const formatTable: Command = (view) => {
  const { state } = view;
  const cur = state.doc.lineAt(state.selection.main.head);
  if (!cur.text.includes('|')) return false;
  let first = cur.number;
  let last = cur.number;
  while (first > 1 && state.doc.line(first - 1).text.includes('|')) first--;
  while (last < state.doc.lines && state.doc.line(last + 1).text.includes('|')) last++;
  const lines: string[] = [];
  for (let n = first; n <= last; n++) lines.push(state.doc.line(n).text);
  const formatted = formatTableText(lines);
  if (!formatted) return false;
  const from = state.doc.line(first).from;
  const to = state.doc.line(last).to;
  view.dispatch({ changes: { from, to, insert: formatted.join('\n') }, userEvent: 'input' });
  return true;
};

export type EditorHooks = {
  /** ルート内の Markdown ファイル一覧（相対パス） */
  files: () => string[];
  currentPath: () => string;
  /** external は外部での変更を取り込んだとき（ユーザーの編集ではない） */
  onChange: (text: string, external: boolean, view: EditorView) => void;
  onCursor: (line: number, col: number) => void;
  onScroll: (topLine: number) => void;
  onBlur: (view: EditorView) => void;
  onPasteImage: (file: File) => void;
  onPlainPaste: () => void;
  onContextMenu: (e: MouseEvent) => void;
  convertHtml: (html: string) => Promise<string | null>;
  /** 変更の帯をクリックした（Git） */
  onGitChunk?: (view: EditorView, pos: number, e: MouseEvent) => void;
};

function linkCompletion(hooks: EditorHooks) {
  return (ctx: CompletionContext): CompletionResult | null => {
    if (ctx.view?.composing) return null;
    const wiki = ctx.matchBefore(/\[\[[^\]\n|#]*/);
    if (wiki) {
      return {
        from: wiki.from + 2,
        options: hooks.files().map((f) => ({ label: stem(f), detail: dirname(f) || undefined, type: 'text' })),
        validFor: /^[^\]\n|#]*$/,
      };
    }
    const md = ctx.matchBefore(/\]\([^)\s]*/);
    if (md) {
      const dir = dirname(hooks.currentPath());
      return {
        from: md.from + 2,
        options: hooks.files().map((f) => ({
          label: encodeLinkTarget(relative(dir, f)),
          displayLabel: relative(dir, f),
          detail: basename(f),
          type: 'text',
        })),
        validFor: /^[^)\s]*$/,
      };
    }
    return null;
  };
}

function looksLikeRichHtml(html: string): boolean {
  // エディタ（VS Code など）からのコピーは書式付きの HTML でも変換しない
  if (/<meta[^>]+vscode|data-vscode|class="?mtk/i.test(html)) return false;
  return /<(a\s|h[1-6]|ul|ol|li|table|strong|b>|em>|i>|img\s|blockquote)/i.test(html);
}

/** Markdown 以外のファイルの色付け。拡張子から言語を探し、読み込めたら差し替える（ログなど該当しないものは色なし） */
export const languageComp = new Compartment();

/** 言語の指定で「色付けしない」を表す名前 */
export const PLAIN_TEXT = 'Plain Text';

/** 選べる言語の名前（表示名順） */
export function languageNames(): string[] {
  return languages.map((l) => l.name).sort((a, b) => a.localeCompare(b));
}

/** ファイル名（拡張子）から言語を探す。見つからなければ先頭の行（#!/usr/bin/env python、<?php など）から */
export function detectLanguage(path: string, text = ''): LanguageDescription | null {
  const byName = LanguageDescription.matchFilename(languages, basename(path));
  if (byName) return byName;
  const first = text.slice(0, 200).split('\n')[0].trim();
  const find = (name: string) => languages.find((l) => l.name === name) ?? null;
  const shebang = /^#!.*?\b(?:env\s+)?([a-z]+)[\d.]*\s*$/i.exec(first);
  if (shebang) {
    const cmd = shebang[1].toLowerCase();
    const map: Record<string, string> = { python: 'Python', node: 'JavaScript', bash: 'Shell', sh: 'Shell', zsh: 'Shell', ruby: 'Ruby', perl: 'Perl', php: 'PHP', pwsh: 'PowerShell', deno: 'TypeScript' };
    if (map[cmd]) return find(map[cmd]);
  }
  if (first.startsWith('<?php')) return find('PHP');
  if (first.startsWith('<?xml')) return find('XML');
  if (/^<!doctype html|^<html/i.test(first)) return find('HTML');
  return null;
}

/**
 * Markdown 以外のファイルの色付け。override に言語名を渡すとその言語、PLAIN_TEXT なら色付けなし、
 * null なら拡張子や先頭の行から自動で選ぶ。戻り値は使った言語の名前（色付けなしなら null）
 */
export async function loadLanguageFor(view: EditorView, path: string, override: string | null = null): Promise<string | null> {
  const desc =
    override === PLAIN_TEXT ? null : override ? (languages.find((l) => l.name === override) ?? null) : detectLanguage(path, view.state.sliceDoc(0, 200));
  if (!desc) {
    if (view.dom.isConnected) view.dispatch({ effects: languageComp.reconfigure([]) });
    return null;
  }
  const support = await desc.load();
  if (view.dom.isConnected) view.dispatch({ effects: languageComp.reconfigure(support) });
  return desc.name;
}

/** Markdown 専用の機能（言語モード・リンク補完・書式のショートカット・HTML や画像の貼り付け） */
function markdownExtensions(hooks: EditorHooks): Extension[] {
  return [
    autocompletion({ override: [linkCompletion(hooks)], activateOnTyping: true, icons: false }),
    // 先頭の --- で囲まれたフロントマターは YAML として色付けする
    yamlFrontmatter({ content: markdown({ base: markdownLanguage, codeLanguages: languages, addKeymap: true }) }),
    placeholder(tr('ここに Markdown で書きます', 'Write Markdown here', '在此输入 Markdown')),
    Prec.high(
      keymap.of([
        { key: 'Mod-b', run: wrapWith('**') },
        { key: 'Mod-i', run: wrapWith('*') },
        { key: 'Mod-k', run: insertLink },
        { key: 'Mod-Shift-c', run: insertCodeBlock },
        { key: 'Mod-Enter', run: toggleTask },
        { key: 'Alt-Shift-f', run: formatTable },
        {
          key: 'Mod-Alt-v',
          run: () => {
            hooks.onPlainPaste();
            return true;
          },
        },
      ]),
    ),
    EditorView.domEventHandlers({
      paste(e, view) {
        const data = e.clipboardData;
        if (!data) return false;
        const image = Array.from(data.items).find((i) => i.kind === 'file' && i.type.startsWith('image/'));
        if (image) {
          const f = image.getAsFile();
          if (f) {
            e.preventDefault();
            hooks.onPasteImage(f);
            return true;
          }
        }
        const html = data.getData('text/html');
        if (html && looksLikeRichHtml(html)) {
          e.preventDefault();
          void hooks.convertHtml(html).then((md) => {
            const text = md ?? data.getData('text/plain');
            view.dispatch(view.state.replaceSelection(text), { userEvent: 'input.paste', scrollIntoView: true });
          });
          return true;
        }
        return false;
      },
    }),
  ];
}

export function createExtensions(
  hooks: EditorHooks,
  opts: { markdown: boolean; lineNumbers: boolean; wrap: boolean; readOnly: boolean; fontSize: number; fontFamily: string },
): Extension[] {
  return [
    lineNumbersComp.of(opts.lineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : []),
    gitGutter((view, pos, e) => hooks.onGitChunk?.(view, pos, e)),
    conflictMarkers,
    wrapComp.of(opts.wrap ? EditorView.lineWrapping : []),
    readOnlyComp.of(EditorState.readOnly.of(opts.readOnly)),
    fontComp.of(fontTheme(opts.fontSize, opts.fontFamily)),
    highlightSpecialChars(),
    history(),
    drawSelection(),
    dropCursor(),
    EditorState.allowMultipleSelections.of(true),
    indentOnInput(),
    indentUnit.of('  '),
    bracketMatching(),
    closeBrackets(),
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    search({ top: true }),
    opts.markdown ? markdownExtensions(hooks) : languageComp.of([]),
    syntaxHighlighting(highlightStyle),
    baseTheme,
    keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...searchKeymap, ...historyKeymap, ...completionKeymap, indentWithTab]),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) {
        const external = u.transactions.every((tr) => !tr.docChanged || tr.isUserEvent('external'));
        hooks.onChange(u.state.doc.toString(), external, u.view);
      }
      if (u.selectionSet || u.docChanged) {
        const head = u.state.selection.main.head;
        const line = u.state.doc.lineAt(head);
        hooks.onCursor(line.number, head - line.from + 1);
      }
      if (u.focusChanged && !u.view.hasFocus) hooks.onBlur(u.view);
    }),
    EditorView.domEventHandlers({
      scroll(_e, view) {
        const top = view.lineBlockAtHeight(view.scrollDOM.scrollTop);
        hooks.onScroll(view.state.doc.lineAt(top.from).number);
      },
      contextmenu(e) {
        e.preventDefault();
        hooks.onContextMenu(e);
        return true;
      },
    }),
  ];
}

/** Git の差分やその時点の内容の表示に使う拡張（readOnly でなければ編集もできる） */
export function viewerExtensions(opts: { markdown: boolean; wrap: boolean; readOnly: boolean; fontSize: number; fontFamily: string }): Extension[] {
  return [
    lineNumbers(),
    opts.wrap ? EditorView.lineWrapping : [],
    EditorState.readOnly.of(opts.readOnly),
    fontTheme(opts.fontSize, opts.fontFamily),
    highlightSpecialChars(),
    drawSelection(),
    history(),
    opts.markdown ? markdown({ base: markdownLanguage, codeLanguages: languages }) : languageComp.of([]),
    syntaxHighlighting(highlightStyle),
    baseTheme,
    EditorView.theme({ '.cm-content': { paddingBottom: '24px' } }),
    EditorState.phrases.of({ '$ unchanged lines': tr('$ 行は同じ', '$ unchanged lines', '$ 行未更改') }),
    search({ top: true }),
    keymap.of([...defaultKeymap, ...searchKeymap, ...historyKeymap]),
  ];
}

export function fontTheme(size: number, family: string): Extension {
  return EditorView.theme({
    '&': { fontSize: `${size}px` },
    '.cm-scroller': family ? { fontFamily: family } : {},
  });
}

/** HTML を Markdown に変換する（貼り付け用。必要になったときに読み込む） */
export async function htmlToMarkdown(html: string): Promise<string | null> {
  try {
    const [{ default: TurndownService }, { gfm }] = await Promise.all([import('turndown'), import('turndown-plugin-gfm')]);
    const td = new TurndownService({ headingStyle: 'atx', codeBlockStyle: 'fenced', bulletListMarker: '-', emDelimiter: '*' });
    td.use(gfm);
    td.remove(['script', 'style', 'meta']);
    return td.turndown(html).trim();
  } catch {
    return null;
  }
}
