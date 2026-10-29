// エディタの Git 関連の表示（docs/git.md §3.3・§4.8）
// - 行番号の横の変更の帯（ステージの内容との差分）
// - 競合の印（<<<<<<< ～ >>>>>>>）の色分けと「採用」ボタン

import { RangeSetBuilder, StateEffect, StateField, Text, type EditorState, type Extension } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, WidgetType, gutter, type DecorationSet } from '@codemirror/view';
import { Chunk } from '@codemirror/merge';
import { tr } from './i18n.svelte';

// ================= 変更の帯 =================

/** 変更の帯の基準（ステージの内容）を設定する。null なら帯を出さない */
export const setGitBase = StateEffect.define<string | null>();

type GitDiff = { base: Text | null; chunks: readonly Chunk[] };

export const gitDiffField = StateField.define<GitDiff>({
  create: () => ({ base: null, chunks: [] }),
  update(v, tr) {
    for (const e of tr.effects) {
      if (e.is(setGitBase)) {
        const base = e.value === null ? null : Text.of(e.value.split('\n'));
        return { base, chunks: base ? Chunk.build(base, tr.state.doc) : [] };
      }
    }
    if (tr.docChanged && v.base) return { base: v.base, chunks: Chunk.updateB(v.chunks, v.base, tr.state.doc, tr.changes) };
    return v;
  },
});

class ChangeMarker extends GutterMarker {
  constructor(readonly kind: 'added' | 'modified' | 'deleted') {
    super();
  }
  eq(other: ChangeMarker) {
    return other.kind === this.kind;
  }
  toDOM() {
    const el = document.createElement('div');
    el.className = `cm-git-${this.kind}`;
    return el;
  }
}

const added = new ChangeMarker('added');
const modified = new ChangeMarker('modified');
const deleted = new ChangeMarker('deleted');

/** 位置を含む変更のまとまり */
export function gitChunkAt(state: EditorState, pos: number): { chunk: Chunk; base: Text } | null {
  const d = state.field(gitDiffField, false);
  if (!d?.base) return null;
  const line = state.doc.lineAt(pos);
  for (const ch of d.chunks) {
    const end = Math.min(ch.toB, state.doc.length);
    if ((line.from >= ch.fromB && line.from <= Math.max(end - 1, ch.fromB)) || (ch.fromB === ch.toB && line.from === state.doc.lineAt(Math.min(ch.fromB, state.doc.length)).from)) {
      return { chunk: ch, base: d.base };
    }
  }
  return null;
}

export function gitGutter(onClick: (view: EditorView, pos: number, e: MouseEvent) => void): Extension {
  return [
    gitDiffField,
    gutter({
      class: 'cm-git-gutter',
      markers(view) {
        const d = view.state.field(gitDiffField, false);
        const b = new RangeSetBuilder<GutterMarker>();
        if (!d?.base) return b.finish();
        const doc = view.state.doc;
        let last = -1;
        for (const ch of d.chunks) {
          if (ch.fromB === ch.toB) {
            const from = doc.lineAt(Math.min(ch.fromB, doc.length)).from;
            if (from > last) b.add(from, from, deleted);
            last = from;
            continue;
          }
          const kind = ch.fromA === ch.toA ? added : modified;
          let pos = ch.fromB;
          while (pos <= doc.length && pos < ch.toB) {
            const line = doc.lineAt(pos);
            if (line.from > last) b.add(line.from, line.from, kind);
            last = line.from;
            pos = line.to + 1;
          }
        }
        return b.finish();
      },
      lineMarkerChange: (u) => u.docChanged || u.transactions.some((t) => t.effects.some((e) => e.is(setGitBase))),
      initialSpacer: () => added,
      domEventHandlers: {
        click(view, line, event) {
          if (!gitChunkAt(view.state, line.from)) return false;
          onClick(view, line.from, event as MouseEvent);
          return true;
        },
      },
    }),
    EditorView.baseTheme({
      '.cm-git-gutter': { width: '4px', paddingLeft: '2px' },
      '.cm-git-gutter .cm-gutterElement': { cursor: 'pointer' },
      '.cm-git-added, .cm-git-modified': { width: '3px', height: '100%' },
      '.cm-git-added': { background: '#2da44e' },
      '.cm-git-modified': { background: '#3b82f6' },
      '.cm-git-deleted': { width: 0, height: 0, borderTop: '4px solid transparent', borderBottom: '4px solid transparent', borderLeft: '5px solid #cf222e', marginTop: '-4px' },
    }),
  ];
}

// ================= 競合の印 =================

export type ConflictBlock = { from: number; to: number; ours: [number, number]; base: [number, number] | null; theirs: [number, number]; oursLabel: string; theirsLabel: string };

/** 文書の中の競合の範囲。各 [from, to] は中身の行の範囲（印の行は含まない） */
export function findConflicts(doc: Text): ConflictBlock[] {
  const out: ConflictBlock[] = [];
  if (!doc.toString().includes('<<<<<<<')) return out;
  let cur: { from: number; oursStart: number; oursLabel: string; baseStart?: number; sep?: number; oursEnd?: number; baseEnd?: number } | null = null;
  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    const t = line.text;
    if (t.startsWith('<<<<<<<')) {
      cur = { from: line.from, oursStart: Math.min(line.to + 1, doc.length), oursLabel: t.slice(7).trim() };
    } else if (cur && t.startsWith('|||||||') && cur.sep === undefined) {
      cur.oursEnd = line.from;
      cur.baseStart = Math.min(line.to + 1, doc.length);
    } else if (cur && /^={7}\s*$/.test(t) && cur.sep === undefined) {
      if (cur.baseStart !== undefined) cur.baseEnd = line.from;
      else cur.oursEnd = line.from;
      cur.sep = Math.min(line.to + 1, doc.length);
    } else if (cur && cur.sep !== undefined && t.startsWith('>>>>>>>')) {
      out.push({
        from: cur.from,
        to: line.to,
        ours: [cur.oursStart, cur.oursEnd ?? cur.oursStart],
        base: cur.baseStart !== undefined ? [cur.baseStart, cur.baseEnd ?? cur.baseStart] : null,
        theirs: [cur.sep, line.from],
        oursLabel: cur.oursLabel,
        theirsLabel: t.slice(7).trim(),
      });
      cur = null;
    }
  }
  return out;
}

/** 競合を解決する: ours（現在の変更）、theirs（取り込む変更）、both（両方） */
export function resolveConflict(view: EditorView, at: number, choice: 'ours' | 'theirs' | 'both') {
  const block = findConflicts(view.state.doc).find((b) => b.from === at);
  if (!block) return;
  const doc = view.state.doc;
  const ours = doc.sliceString(block.ours[0], block.ours[1]);
  const theirs = doc.sliceString(block.theirs[0], block.theirs[1]);
  const text = choice === 'ours' ? ours : choice === 'theirs' ? theirs : ours + theirs;
  // 最後の印の行の改行も含めて置き換える
  const end = block.to < doc.length ? block.to + 1 : block.to;
  view.dispatch({ changes: { from: block.from, to: end, insert: text }, userEvent: 'input' });
}

class ConflictWidget extends WidgetType {
  constructor(readonly at: number, readonly oursLabel: string, readonly theirsLabel: string) {
    super();
  }
  eq(other: ConflictWidget) {
    return other.at === this.at && other.oursLabel === this.oursLabel && other.theirsLabel === this.theirsLabel;
  }
  toDOM(view: EditorView) {
    const box = document.createElement('div');
    box.className = 'cm-conflict-actions';
    const add = (label: string, choice: 'ours' | 'theirs' | 'both', title: string) => {
      const b = document.createElement('button');
      b.textContent = label;
      b.title = title;
      b.onmousedown = (e) => e.preventDefault();
      b.onclick = () => resolveConflict(view, this.at, choice);
      box.append(b);
    };
    add(tr('現在の変更を採用', 'Accept current', '采用当前更改'), 'ours', this.oursLabel);
    add(tr('取り込む変更を採用', 'Accept incoming', '采用传入的更改'), 'theirs', this.theirsLabel);
    add(tr('両方を採用', 'Accept both', '保留双方'), 'both', '');
    return box;
  }
  ignoreEvent() {
    return true;
  }
}

function conflictDecorations(state: EditorState): DecorationSet {
  const blocks = findConflicts(state.doc);
  if (!blocks.length) return Decoration.none;
  const b = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const lines = (from: number, to: number, cls: string) => {
    let pos = from;
    while (pos < to && pos <= doc.length) {
      const line = doc.lineAt(pos);
      b.add(line.from, line.from, Decoration.line({ class: cls }));
      pos = line.to + 1;
    }
  };
  for (const blk of blocks) {
    b.add(blk.from, blk.from, Decoration.widget({ widget: new ConflictWidget(blk.from, blk.oursLabel, blk.theirsLabel), block: true, side: -1 }));
    b.add(blk.from, blk.from, Decoration.line({ class: 'cm-conflict-marker cm-conflict-ours-head' }));
    lines(blk.ours[0], blk.ours[1], 'cm-conflict-ours');
    if (blk.base) {
      const baseMarker = doc.lineAt(blk.ours[1]);
      b.add(baseMarker.from, baseMarker.from, Decoration.line({ class: 'cm-conflict-marker' }));
      lines(blk.base[0], blk.base[1], 'cm-conflict-base');
    }
    const sep = doc.lineAt(blk.base ? blk.base[1] : blk.ours[1]);
    b.add(sep.from, sep.from, Decoration.line({ class: 'cm-conflict-marker' }));
    lines(blk.theirs[0], blk.theirs[1], 'cm-conflict-theirs');
    const end = doc.lineAt(blk.to);
    b.add(end.from, end.from, Decoration.line({ class: 'cm-conflict-marker cm-conflict-theirs-head' }));
  }
  return b.finish();
}

const conflictField = StateField.define<DecorationSet>({
  create: (state) => conflictDecorations(state),
  update: (v, tr) => (tr.docChanged ? conflictDecorations(tr.state) : v),
  provide: (f) => EditorView.decorations.from(f),
});

export const conflictMarkers: Extension = [
  conflictField,
  EditorView.baseTheme({
    '.cm-conflict-actions': { display: 'flex', gap: '6px', padding: '2px 24px', fontFamily: 'var(--ui-font)', fontSize: '12px' },
    '.cm-conflict-actions button': {
      border: '1px solid var(--border)',
      background: 'var(--bg-alt)',
      color: 'var(--fg)',
      borderRadius: '4px',
      padding: '1px 8px',
      cursor: 'pointer',
    },
    '.cm-conflict-actions button:hover': { borderColor: 'var(--accent)' },
    '.cm-conflict-marker': { color: 'var(--fg-muted)', fontWeight: '600' },
    '.cm-conflict-ours, .cm-conflict-ours-head': { background: 'color-mix(in srgb, #2da44e 14%, transparent)' },
    '.cm-conflict-theirs, .cm-conflict-theirs-head': { background: 'color-mix(in srgb, #3b82f6 14%, transparent)' },
    '.cm-conflict-base': { background: 'color-mix(in srgb, #9ca3af 14%, transparent)' },
  }),
];
