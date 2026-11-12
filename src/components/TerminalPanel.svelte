<script lang="ts">
  import { store } from '../lib/store.svelte';
  import TerminalView from './TerminalView.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { contains, layout, ratioAt, type Divider, type Rect } from '../lib/termLayout';

  // ターミナルの切り替えは左ペインの一覧（TerminalList）で行う。
  // 分割したターミナルは、選んでいるターミナルのグループ（木）を左右・上下に並べて表示する。
  // 木が変わってもターミナルを作り直さないよう、全ターミナルを同じ親に置き、位置だけを変える

  let width = $state(0);
  let height = $state(0);

  const groupIndex = $derived(store.activeTerminal === null ? -1 : store.termLayouts.findIndex((l) => contains(l, store.activeTerminal!)));
  const placed = $derived.by(() => {
    const rects = new Map<number, Rect>();
    let dividers: Divider[] = [];
    store.termLayouts.forEach((l, i) => {
      const r = layout(l);
      for (const [k, rect] of r.rects) rects.set(k, rect);
      if (i === groupIndex) dividers = r.dividers;
    });
    return { rects, dividers };
  });
  /** 選んでいるグループのターミナル */
  const visibleKeys = $derived.by(() => {
    const g = store.termLayouts[groupIndex];
    const out = new Set<number>();
    if (g) for (const k of layout(g).rects.keys()) out.add(k);
    return out;
  });
  const splitShown = $derived(visibleKeys.size > 1);

  function pct(v: number) {
    return `${v * 100}%`;
  }

  // ---- 境界のドラッグ ----
  let drag: { divider: Divider; start: number; ratio: number } | null = null;

  function down(e: PointerEvent, d: Divider) {
    const g = store.termLayouts[groupIndex];
    if (!g) return;
    drag = { divider: d, start: d.dir === 'row' ? e.clientX : e.clientY, ratio: ratioAt(g, d.path) };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const d = drag.divider;
    const size = d.dir === 'row' ? d.parent.w * width : d.parent.h * height;
    if (size <= 0) return;
    const delta = (d.dir === 'row' ? e.clientX : e.clientY) - drag.start;
    store.setTermRatio(groupIndex, d.path, drag.ratio + delta / size);
  }

  function up() {
    drag = null;
  }
</script>

<div class="panel">
  <div class="views" bind:clientWidth={width} bind:clientHeight={height}>
    {#each store.terminals as t (t.key)}
      {@const r = placed.rects.get(t.key) ?? { x: 0, y: 0, w: 1, h: 1 }}
      <div
        class="view"
        class:hidden={!visibleKeys.has(t.key)}
        class:focused={splitShown && store.activeTerminal === t.key}
        style:left={pct(r.x)}
        style:top={pct(r.y)}
        style:width={pct(r.w)}
        style:height={pct(r.h)}
        onpointerdown={() => (store.activeTerminal = t.key)}
        role="presentation"
      >
        <TerminalView session={t} active={store.activeTerminal === t.key && store.terminalVisible} />
      </div>
    {/each}
    {#each placed.dividers as d (d.path.join('') + d.dir)}
      <div
        class="divider {d.dir}"
        style:left={pct(d.rect.x)}
        style:top={pct(d.rect.y)}
        style:width={d.dir === 'row' ? null : pct(d.rect.w)}
        style:height={d.dir === 'column' ? null : pct(d.rect.h)}
        role="separator"
        aria-orientation={d.dir === 'row' ? 'vertical' : 'horizontal'}
        onpointerdown={(e) => down(e, d)}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
      ></div>
    {/each}
    {#if store.terminals.length === 0}
      <div class="empty">
        <button class="btn" onclick={() => store.newTerminal()}>{tr('ターミナルを開く', 'Open terminal', '打开终端')}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-pane);
  }

  .views {
    flex: 1;
    min-height: 0;
    position: relative;
    /* ターミナルの周りの余白もターミナルの背景色にする */
    background: var(--terminal-bg, var(--bg-pane));
  }

  .view {
    position: absolute;
  }

  .view.hidden {
    visibility: hidden;
  }

  /* 分割しているときは、選んでいるターミナルの上端に強調色の線を出す */
  .view.focused::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
    z-index: 2;
  }

  /* 境界。見た目は 1px の線、つかめる幅は 7px */
  .divider {
    position: absolute;
    z-index: 5;
  }

  .divider::before {
    content: '';
    position: absolute;
    background: var(--border);
  }

  .divider.row {
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
  }

  .divider.row::before {
    left: 3px;
    top: 0;
    bottom: 0;
    width: 1px;
  }

  .divider.column {
    height: 7px;
    margin-top: -3px;
    cursor: row-resize;
  }

  .divider.column::before {
    top: 3px;
    left: 0;
    right: 0;
    height: 1px;
  }

  .divider:hover::before,
  .divider:active::before {
    background: var(--accent);
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
</style>
