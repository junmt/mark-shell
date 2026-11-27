// エディタ内の検索・置換パネル（Ctrl+F / Ctrl+H）。CodeMirror 標準のパネルの代わりに使う。
// 標準のパネルは keyup でしか検索語を反映しないため、日本語入力の確定や貼り付けで検索語がずれることがあった。
// ここでは input / compositionend で反映し、入力中はカーソル位置から最初の一致へ移動し、件数も表示する。

import type { EditorState } from '@codemirror/state';
import { EditorView, runScopeHandlers, type Command, type Panel, type ViewUpdate } from '@codemirror/view';
import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  openSearchPanel,
  replaceAll,
  replaceNext,
  search,
  selectMatches,
  setSearchQuery,
} from '@codemirror/search';
import { tr } from './i18n.svelte';

const MAX_COUNT = 9999;

const ICONS = {
  chevronRight: 'M9 6l6 6-6 6',
  chevronDown: 'M6 9l6 6 6-6',
  arrowUp: 'M12 19V5 M6 11l6-6 6 6',
  arrowDown: 'M12 5v14 M6 13l6 6 6-6',
  selectAll: 'M4 7V5a1 1 0 0 1 1-1h2 M17 4h2a1 1 0 0 1 1 1v2 M20 17v2a1 1 0 0 1-1 1h-2 M7 20H5a1 1 0 0 1-1-1v-2 M8 9h8 M8 12h8 M8 15h5',
  close: 'M6 6l12 12 M18 6L6 18',
  replace: 'M4 7h11l-3-3 M15 7l-3 3 M20 17H9l3 3 M9 17l3-3',
  replaceAll: 'M4 6h10l-3-3 M14 6l-3 3 M20 18H10l3 3 M10 18l3-3 M4 12h16',
};

function icon(name: keyof typeof ICONS): SVGSVGElement {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('width', '16');
  svg.setAttribute('height', '16');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '1.8');
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  const path = document.createElementNS(ns, 'path');
  path.setAttribute('d', ICONS[name]);
  svg.appendChild(path);
  return svg;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, attrs: Record<string, string> = {}): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  return e;
}

function iconButton(name: keyof typeof ICONS, title: string, onclick: () => void): HTMLButtonElement {
  const b = el('button', 'mk-search-btn', { type: 'button', title, 'aria-label': title });
  b.appendChild(icon(name));
  // ボタンを押してもエディタや入力欄のフォーカスを奪わない
  b.addEventListener('mousedown', (e) => e.preventDefault());
  b.addEventListener('click', onclick);
  return b;
}

function toggleButton(label: string, title: string, onchange: () => void): HTMLButtonElement {
  const b = el('button', 'mk-search-toggle', { type: 'button', title, 'aria-label': title, 'aria-pressed': 'false' });
  b.textContent = label;
  b.addEventListener('mousedown', (e) => e.preventDefault());
  b.addEventListener('click', () => {
    b.setAttribute('aria-pressed', b.getAttribute('aria-pressed') === 'true' ? 'false' : 'true');
    onchange();
  });
  return b;
}

const pressed = (b: HTMLElement) => b.getAttribute('aria-pressed') === 'true';
const setPressed = (b: HTMLElement, on: boolean) => b.setAttribute('aria-pressed', on ? 'true' : 'false');

/** 日本語入力の変換中のキー操作か */
function composing(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}

/** 一致の総数と、選択範囲がそのうち何番目か（0 は該当なし） */
function countMatches(state: EditorState, query: SearchQuery): { total: number; current: number; capped: boolean } {
  const sel = state.selection.main;
  let total = 0;
  let current = 0;
  const cursor = query.getCursor(state);
  for (let r = cursor.next(); !r.done; r = cursor.next()) {
    total++;
    if (r.value.from === sel.from && r.value.to === sel.to) current = total;
    if (total >= MAX_COUNT) return { total, current, capped: true };
  }
  return { total, current, capped: false };
}

function regexpError(source: string): boolean {
  try {
    new RegExp(source);
    return false;
  } catch {
    return true;
  }
}

const panels = new WeakMap<EditorView, MarkSearchPanel>();

class MarkSearchPanel implements Panel {
  dom: HTMLElement;
  top = true;
  private query: SearchQuery;
  private searchField: HTMLInputElement;
  private replaceField: HTMLInputElement;
  private caseBtn: HTMLButtonElement;
  private wordBtn: HTMLButtonElement;
  private reBtn: HTMLButtonElement;
  private expandBtn: HTMLButtonElement;
  private count: HTMLElement;
  private replaceRow: HTMLElement;
  private readOnly: boolean;
  private expanded = false;

  constructor(private view: EditorView) {
    this.query = getSearchQuery(view.state);
    this.readOnly = view.state.readOnly;

    this.searchField = el('input', 'mk-search-input', {
      type: 'text',
      placeholder: tr('検索', 'Find', '查找'),
      'aria-label': tr('検索', 'Find', '查找'),
      'main-field': 'true',
      spellcheck: 'false',
      autocomplete: 'off',
    });
    this.searchField.value = this.query.search;
    this.replaceField = el('input', 'mk-search-input', {
      type: 'text',
      placeholder: tr('置換', 'Replace', '替换'),
      'aria-label': tr('置換', 'Replace', '替换'),
      spellcheck: 'false',
      autocomplete: 'off',
    });
    this.replaceField.value = this.query.replace;

    const onToggle = () => this.commit(true);
    this.caseBtn = toggleButton('Aa', tr('大文字と小文字を区別', 'Match case', '区分大小写'), onToggle);
    this.wordBtn = toggleButton('ab', tr('単語単位で検索', 'Match whole word', '全字匹配'), onToggle);
    this.wordBtn.classList.add('word');
    this.reBtn = toggleButton('.*', tr('正規表現', 'Use regular expression', '使用正则表达式'), onToggle);
    setPressed(this.caseBtn, this.query.caseSensitive);
    setPressed(this.wordBtn, this.query.wholeWord);
    setPressed(this.reBtn, this.query.regexp);

    for (const f of [this.searchField, this.replaceField]) {
      // 変換中は反映しない（確定したら compositionend で反映する）
      f.addEventListener('input', (e) => {
        if (!(e as InputEvent).isComposing) this.commit(f === this.searchField);
      });
      f.addEventListener('compositionend', () => this.commit(f === this.searchField));
    }

    this.expandBtn = iconButton('chevronRight', tr('置換を表示', 'Toggle replace', '切换替换'), () => {
      this.setExpanded(!this.expanded);
      (this.expanded ? this.replaceField : this.searchField).focus();
    });
    this.expandBtn.classList.add('mk-search-expand');

    this.count = el('span', 'mk-search-count', { 'aria-live': 'polite' });

    const findBox = el('div', 'mk-search-box');
    const toggles = el('div', 'mk-search-toggles');
    toggles.append(this.caseBtn, this.wordBtn, this.reBtn);
    findBox.append(this.searchField, toggles);

    const findRow = el('div', 'mk-search-row');
    findRow.append(
      findBox,
      this.count,
      iconButton('arrowUp', tr('前を検索 (Shift+Enter)', 'Previous match (Shift+Enter)', '上一个 (Shift+Enter)'), () => this.go(-1)),
      iconButton('arrowDown', tr('次を検索 (Enter)', 'Next match (Enter)', '下一个 (Enter)'), () => this.go(1)),
      iconButton('selectAll', tr('一致をすべて選択 (Alt+Enter)', 'Select all matches (Alt+Enter)', '选择所有匹配项 (Alt+Enter)'), () => this.selectAll()),
      iconButton('close', tr('閉じる (Esc)', 'Close (Esc)', '关闭 (Esc)'), () => closeSearchPanel(this.view)),
    );

    const replaceBox = el('div', 'mk-search-box');
    replaceBox.append(this.replaceField);
    this.replaceRow = el('div', 'mk-search-row mk-search-replace-row');
    this.replaceRow.append(
      replaceBox,
      iconButton('replace', tr('置換 (Enter)', 'Replace (Enter)', '替换 (Enter)'), () => {
        this.commit(false);
        replaceNext(this.view);
      }),
      iconButton('replaceAll', tr('すべて置換 (Ctrl+Alt+Enter)', 'Replace all (Ctrl+Alt+Enter)', '全部替换 (Ctrl+Alt+Enter)'), () => {
        this.commit(false);
        replaceAll(this.view);
      }),
    );

    const rows = el('div', 'mk-search-rows');
    rows.append(findRow, this.replaceRow);

    this.dom = el('div', 'cm-search mk-search', { role: 'search' });
    if (this.readOnly) this.dom.classList.add('readonly');
    else this.dom.append(this.expandBtn);
    this.dom.append(rows);
    this.dom.addEventListener('keydown', (e) => this.keydown(e));

    this.setExpanded(false);
    this.updateCount();
    panels.set(view, this);
  }

  mount() {
    this.searchField.focus();
    this.searchField.select();
  }

  destroy() {
    panels.delete(this.view);
  }

  update(u: ViewUpdate) {
    let queryChanged = false;
    for (const tr of u.transactions) {
      for (const effect of tr.effects) {
        if (!effect.is(setSearchQuery)) continue;
        // パネル自身が出した検索語でも件数は数え直す
        queryChanged = true;
        if (!effect.value.eq(this.query)) this.setQuery(effect.value);
      }
    }
    if (u.state.readOnly !== this.readOnly) {
      this.readOnly = u.state.readOnly;
      this.dom.classList.toggle('readonly', this.readOnly);
      if (this.readOnly) this.expandBtn.remove();
      else this.dom.prepend(this.expandBtn);
      this.setExpanded(this.expanded);
    }
    if (queryChanged || u.docChanged || u.selectionSet) this.updateCount();
  }

  /** Ctrl+H で置換欄を開く */
  showReplace() {
    if (this.readOnly) return;
    this.setExpanded(true);
    this.replaceField.focus();
    this.replaceField.select();
  }

  private setExpanded(on: boolean) {
    this.expanded = on && !this.readOnly;
    this.replaceRow.hidden = !this.expanded;
    this.expandBtn.replaceChildren(icon(this.expanded ? 'chevronDown' : 'chevronRight'));
    this.expandBtn.setAttribute('aria-expanded', String(this.expanded));
  }

  private setQuery(q: SearchQuery) {
    this.query = q;
    if (this.searchField.value !== q.search) this.searchField.value = q.search;
    if (this.replaceField.value !== q.replace) this.replaceField.value = q.replace;
    setPressed(this.caseBtn, q.caseSensitive);
    setPressed(this.wordBtn, q.wholeWord);
    setPressed(this.reBtn, q.regexp);
  }

  /** 入力欄の内容を検索語として反映する。reveal なら選択位置から最初の一致へ移動する */
  private commit(reveal: boolean) {
    const q = new SearchQuery({
      search: this.searchField.value,
      replace: this.replaceField.value,
      caseSensitive: pressed(this.caseBtn),
      wholeWord: pressed(this.wordBtn),
      regexp: pressed(this.reBtn),
    });
    if (q.eq(this.query)) return;
    const searchChanged = q.search !== this.query.search || q.caseSensitive !== this.query.caseSensitive || q.wholeWord !== this.query.wholeWord || q.regexp !== this.query.regexp;
    this.query = q;
    const effects = [setSearchQuery.of(q)];
    if (reveal && searchChanged && q.valid) {
      const match = this.firstMatchFrom(q, this.view.state.selection.main.from);
      if (match) {
        this.view.dispatch({
          effects: [...effects, EditorView.scrollIntoView(match.from, { y: 'center' })],
          selection: { anchor: match.from, head: match.to },
          userEvent: 'select.search',
        });
        return;
      }
    }
    this.view.dispatch({ effects });
  }

  private firstMatchFrom(q: SearchQuery, pos: number): { from: number; to: number } | null {
    const state = this.view.state;
    const after = q.getCursor(state, pos).next();
    if (!after.done) return after.value;
    const wrapped = q.getCursor(state, 0, pos).next();
    return wrapped.done ? null : wrapped.value;
  }

  private go(dir: 1 | -1) {
    this.commit(false);
    if (!this.query.valid) return;
    (dir > 0 ? findNext : findPrevious)(this.view);
  }

  private selectAll() {
    this.commit(false);
    if (this.query.valid) selectMatches(this.view);
  }

  private keydown(e: KeyboardEvent) {
    if (composing(e)) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.target === this.replaceField) {
        this.commit(false);
        if ((e.ctrlKey || e.metaKey) && e.altKey) replaceAll(this.view);
        else replaceNext(this.view);
      } else if (e.altKey) {
        this.selectAll();
      } else {
        this.go(e.shiftKey ? -1 : 1);
      }
      return;
    }
    // Esc・F3・Ctrl+G などは検索のキー割り当てに任せる
    this.commit(false);
    if (runScopeHandlers(this.view, e, 'search-panel')) e.preventDefault();
  }

  private updateCount() {
    const q = this.query;
    this.dom.classList.remove('no-match', 'invalid');
    if (!q.search) {
      this.count.textContent = '';
      return;
    }
    if (!q.valid) {
      if (q.regexp && regexpError(q.search)) {
        this.count.textContent = tr('正規表現が不正です', 'Invalid regex', '正则表达式无效');
        this.dom.classList.add('invalid');
      } else {
        this.count.textContent = '';
      }
      return;
    }
    const { total, current, capped } = countMatches(this.view.state, q);
    if (total === 0) {
      this.count.textContent = tr('結果なし', 'No results', '无结果');
      this.dom.classList.add('no-match');
      return;
    }
    const totalText = capped ? `${MAX_COUNT}+` : String(total);
    this.count.textContent = current ? `${current} / ${totalText}` : tr(`${totalText} 件`, `${totalText} results`, `${totalText} 个结果`);
  }
}

/** Ctrl+H: 置換欄を開いた状態で検索パネルを表示する */
export const openReplacePanel: Command = (view) => {
  if (view.state.readOnly) return openSearchPanel(view);
  openSearchPanel(view);
  panels.get(view)?.showReplace();
  return true;
};

/** 検索パネルの拡張（search() の代わりに使う） */
export function markSearch() {
  return search({ top: true, createPanel: (view) => new MarkSearchPanel(view) });
}

/** エディタの外（プレビューなど）で Ctrl+F を押したときに開く */
export function openSearchIn(view: EditorView, replace = false): void {
  if (replace) openReplacePanel(view);
  else openSearchPanel(view);
}
