<script module lang="ts">
  import { EditorState } from '@codemirror/state';
  import { EditorView } from '@codemirror/view';
  import {
    createExtensions,
    fontComp,
    fontTheme,
    htmlToMarkdown,
    lineNumbersComp,
    loadLanguageFor,
    readOnlyComp,
    wrapComp,
    type EditorHooks,
  } from '../lib/editor';
  import { lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
  import { store, type Doc } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { headings, slugify } from '../lib/markdown';
  import { git } from '../lib/git.svelte';
  import { applyChunk, revertChunk } from '../lib/git';
  import { gitChunkAt, setGitBase } from '../lib/gitEditor';

  // 表示中のエディタは常に 1 つなので、エディタの状態に埋め込むコールバックは共通にする。
  // タブごとの EditorState を作り直さずに使い回せる
  const bridge = { onScrollLine: null as ((line: number) => void) | null };
  /** エディタごとに、表示しているファイル */
  const docOf = new WeakMap<EditorView, Doc>();

  const hooks: EditorHooks = {
    files: () => store.markdownFiles(),
    currentPath: () => store.activeDoc?.path ?? '',
    onChange: (text, external, view) => {
      const doc = docOf.get(view);
      if (!doc) return;
      if (external) doc.text = text;
      else store.onDocChanged(doc, text);
    },
    onCursor: (line, col) => (store.cursor = { line, col }),
    onScroll: (line) => bridge.onScrollLine?.(line),
    onBlur: (view) => {
      const doc = docOf.get(view);
      if (doc?.dirty) void store.save(doc);
    },
    onPasteImage: (f) => void store.pasteImage(f),
    onPlainPaste: () => void store.plainPaste(),
    convertHtml: htmlToMarkdown,
    onContextMenu: (e) => openEditorMenu(e),
    onGitChunk: (view, pos, e) => openChunkMenu(view, pos, e),
  };

  /** 変更の帯のクリック: 差分の表示・元に戻す・ステージ */
  function openChunkMenu(view: EditorView, pos: number, e: MouseEvent) {
    const doc = docOf.get(view);
    const found = gitChunkAt(view.state, pos);
    if (!doc || !found) return;
    const base = found.base.toString();
    const text = view.state.doc.toString();
    store.openMenu(e, [
      { label: tr('変更を表示', 'Show changes', '显示更改'), action: () => git.openWorkingDiff(doc.path) },
      {
        label: tr('この変更を元に戻す', 'Revert this change', '还原此更改'),
        disabled: doc.readonly,
        action: () => {
          const next = revertChunk(base, text, found.chunk);
          view.dispatch({ changes: minimalChange(text, next), userEvent: 'input' });
        },
      },
      { label: tr('この変更をステージ', 'Stage this change', '暂存此更改'), action: () => void git.stageText(doc.path, applyChunk(base, text, found.chunk), doc.encoding, doc.eol) },
    ]);
  }

  /** 前後の同じ部分を除いた置き換え（カーソルの位置を保つため） */
  function minimalChange(a: string, b: string) {
    let s = 0;
    while (s < a.length && s < b.length && a[s] === b[s]) s++;
    let e = 0;
    while (e < a.length - s && e < b.length - s && a[a.length - 1 - e] === b[b.length - 1 - e]) e++;
    return { from: s, to: a.length - e, insert: b.slice(s, b.length - e) };
  }

  function openEditorMenu(e: MouseEvent) {
    const view = store.view;
    const doc = store.activeDoc;
    if (!view || !doc) return;
    const hasSel = !view.state.selection.main.empty;
    const items = [
      { label: tr('切り取り', 'Cut', '剪切'), shortcut: 'Ctrl+X', disabled: !hasSel, action: () => document.execCommand('cut') },
      { label: tr('コピー', 'Copy', '复制'), shortcut: 'Ctrl+C', disabled: !hasSel, action: () => document.execCommand('copy') },
      {
        label: tr('貼り付け', 'Paste', '粘贴'),
        shortcut: 'Ctrl+V',
        action: async () => {
          const text = await navigator.clipboard.readText();
          view.dispatch(view.state.replaceSelection(text), { userEvent: 'input.paste' });
        },
      },
      { label: tr('すべて選択', 'Select all', '全选'), shortcut: 'Ctrl+A', action: () => view.dispatch({ selection: { anchor: 0, head: view.state.doc.length } }) },
      { label: '', separator: true },
    ] as { label: string; shortcut?: string; disabled?: boolean; separator?: boolean; action?: () => void }[];
    const tools = store.aiTools.filter((t) => t.found);
    for (const t of tools) {
      for (const a of t.actions) items.push({ label: `${t.name}: ${a.label}`, action: () => void store.launchTool(t, a.id) });
    }
    if (tools.length === 0) items.push({ label: tr('AI ツールが見つかりません', 'No AI tools found', '未找到 AI 工具'), disabled: true });
    store.openMenu(e, items);
    view.focus();
  }

  function stateFor(doc: Doc): EditorState {
    if (!doc.cm || doc.cm.doc.toString() !== doc.text) {
      doc.cm = EditorState.create({
        doc: doc.text,
        extensions: createExtensions(hooks, {
          markdown: store.isMarkdown(doc.path),
          lineNumbers: store.config.lineNumbers,
          wrap: store.config.lineWrapping,
          readOnly: doc.readonly,
          fontSize: store.config.editorFontSize,
          fontFamily: store.config.editorFontFamily,
        }),
      });
    }
    return doc.cm;
  }
</script>

<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';

  let {
    doc: docProp,
    onscrollline,
    scrollToLine = $bindable(),
  }: { doc: Doc; onscrollline: (line: number) => void; scrollToLine?: ((line: number) => void) | null } = $props();

  // タブを切り替えるとプロパティは新しいタブのファイルを指すので、作ったときのファイルに固定する。
  // そうしないと、閉じる側のエディタの状態が別のファイルに書き込まれる
  const doc = untrack(() => docProp);

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let programmatic = 0;

  onMount(() => {
    view = new EditorView({ parent: host, state: stateFor(doc) });
    docOf.set(view, doc);
    store.view = view;
    bridge.onScrollLine = (line) => {
      if (Date.now() - programmatic > 150) onscrollline(line);
    };
    scrollToLine = (line: number) => {
      if (!view) return;
      programmatic = Date.now();
      const l = view.state.doc.line(Math.min(Math.max(1, line), view.state.doc.lines));
      view.scrollDOM.scrollTop = view.lineBlockAt(l.from).top;
    };
    requestAnimationFrame(() => {
      if (!view) return;
      view.scrollDOM.scrollTop = doc.scrollTop;
      if (!consumeReveal()) view.focus();
    });
  });

  onDestroy(() => {
    if (view) {
      doc.cm = view.state;
      doc.scrollTop = view.scrollDOM.scrollTop;
      if (store.view === view) store.view = null;
      view.destroy();
      view = null;
    }
    bridge.onScrollLine = null;
  });

  // 設定が変わったらエディタに反映する
  $effect(() => {
    const c = store.config;
    const ro = doc.readonly;
    view?.dispatch({
      effects: [
        lineNumbersComp.reconfigure(c.lineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : []),
        wrapComp.reconfigure(c.lineWrapping ? EditorView.lineWrapping : []),
        readOnlyComp.reconfigure(EditorState.readOnly.of(ro)),
        fontComp.reconfigure(fontTheme(c.editorFontSize, c.editorFontFamily)),
      ],
    });
  });

  // Markdown 以外のファイルの色付け。手動で選んだ言語が変わったら付け直す
  $effect(() => {
    const override = doc.language;
    if (!view || store.isMarkdown(doc.path)) return;
    void loadLanguageFor(view, doc.path, override).then((name) => (doc.languageName = name));
  });

  // Git のステージの内容が変わったら変更の帯を描き直す
  $effect(() => {
    const base = doc.gitBase;
    view?.dispatch({ effects: setGitBase.of(base) });
  });

  /** 検索結果やアウトラインから指定された行・見出しへ移動する */
  function consumeReveal(): boolean {
    if (!view) return false;
    let line: number | null = null;
    const r = store.pendingReveal;
    if (r && r.path === doc.path) {
      line = r.line;
      store.pendingReveal = null;
    } else if (store.pendingAnchor) {
      const slug = store.pendingAnchor.toLowerCase();
      store.pendingAnchor = null;
      line = headings(doc.text).find((h) => slugify(h.text) === slug)?.line ?? null;
    }
    if (line === null) return false;
    const l = view.state.doc.line(Math.min(Math.max(1, line), view.state.doc.lines));
    view.dispatch({ selection: { anchor: l.from }, effects: EditorView.scrollIntoView(l.from, { y: 'start', yMargin: 60 }) });
    view.focus();
    return true;
  }

  $effect(() => {
    void store.revealSeq;
    consumeReveal();
  });
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--bg);
    user-select: text;
  }

  .editor :global(.cm-editor) {
    height: 100%;
  }

  .editor :global(.cm-editor.cm-focused) {
    outline: none;
  }
</style>
