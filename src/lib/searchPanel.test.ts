import { afterEach, describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { getSearchQuery, openSearchPanel } from '@codemirror/search';
import { markSearch, openReplacePanel } from './searchPanel';

let view: EditorView | null = null;

function setup(doc: string, readOnly = false): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  view = new EditorView({ parent, state: EditorState.create({ doc, extensions: [markSearch(), EditorState.readOnly.of(readOnly)] }) });
  openSearchPanel(view);
  return view;
}

function field(v: EditorView, name: 'Find' | 'Replace' = 'Find'): HTMLInputElement {
  const inputs = v.dom.querySelectorAll<HTMLInputElement>('.mk-search-input');
  return inputs[name === 'Find' ? 0 : 1];
}

function type(input: HTMLInputElement, text: string) {
  input.value = text;
  input.dispatchEvent(new InputEvent('input', { bubbles: true }));
}

function key(input: HTMLInputElement, k: string, init: KeyboardEventInit = {}) {
  input.dispatchEvent(new KeyboardEvent('keydown', { key: k, keyCode: k === 'Enter' ? 13 : 0, bubbles: true, cancelable: true, ...init }));
}

const count = (v: EditorView) => v.dom.querySelector('.mk-search-count')!.textContent;

afterEach(() => {
  view?.destroy();
  view = null;
  document.body.innerHTML = '';
});

describe('検索パネル', () => {
  it('入力するとすぐ検索語が反映され、最初の一致を選ぶ', () => {
    const v = setup('りんご みかん りんご ぶどう りんご');
    type(field(v), 'りんご');
    expect(getSearchQuery(v.state).search).toBe('りんご');
    expect(v.state.selection.main.from).toBe(0);
    expect(count(v)).toBe('1 / 3');
  });

  it('Enter で次、Shift+Enter で前へ移動する', () => {
    const v = setup('abc abc abc');
    type(field(v), 'abc');
    key(field(v), 'Enter');
    expect(v.state.selection.main.from).toBe(4);
    expect(count(v)).toBe('2 / 3');
    key(field(v), 'Enter', { shiftKey: true });
    expect(v.state.selection.main.from).toBe(0);
  });

  it('日本語入力の変換中は検索語を反映せず、確定で反映する', () => {
    const v = setup('検索 けんさく');
    const f = field(v);
    f.value = 'けんさ';
    f.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true }));
    expect(getSearchQuery(v.state).search).toBe('');
    key(f, 'Enter', { isComposing: true, keyCode: 229 });
    expect(v.state.selection.main.empty).toBe(true);
    f.value = '検索';
    f.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
    expect(getSearchQuery(v.state).search).toBe('検索');
    expect(count(v)).toBe('1 / 1');
  });

  it('一致がなければ「結果なし」、不正な正規表現はエラーを表示する', () => {
    const v = setup('hello');
    type(field(v), 'xyz');
    expect(v.dom.querySelector('.mk-search')!.classList.contains('no-match')).toBe(true);
    const re = v.dom.querySelectorAll<HTMLButtonElement>('.mk-search-toggle')[2];
    re.click();
    type(field(v), '(');
    expect(v.dom.querySelector('.mk-search')!.classList.contains('invalid')).toBe(true);
  });

  it('置換とすべて置換', () => {
    const v = setup('a-a-a');
    openReplacePanel(v);
    type(field(v), 'a');
    type(field(v, 'Replace'), 'b');
    key(field(v, 'Replace'), 'Enter');
    expect(v.state.doc.toString()).toBe('b-a-a');
    key(field(v, 'Replace'), 'Enter', { ctrlKey: true, altKey: true });
    expect(v.state.doc.toString()).toBe('b-b-b');
  });

  it('読み取り専用では置換欄を出さない', () => {
    const v = setup('abc', true);
    openReplacePanel(v);
    expect(v.dom.querySelector('.mk-search-expand')).toBeNull();
    expect((v.dom.querySelector('.mk-search-replace-row') as HTMLElement).hidden).toBe(true);
  });
});
