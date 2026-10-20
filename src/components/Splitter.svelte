<script lang="ts">
  // ペインの境界。ドラッグした差分を onresize に渡す
  let {
    direction,
    onresize,
    ondone,
  }: { direction: 'vertical' | 'horizontal'; onresize: (delta: number) => void; ondone?: () => void } = $props();

  let last = 0;
  let dragging = $state(false);

  function down(e: PointerEvent) {
    dragging = true;
    last = direction === 'vertical' ? e.clientX : e.clientY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    const pos = direction === 'vertical' ? e.clientX : e.clientY;
    const d = pos - last;
    last = pos;
    if (d !== 0) onresize(d);
  }

  function up() {
    if (!dragging) return;
    dragging = false;
    ondone?.();
  }
</script>

<div
  class="splitter {direction}"
  class:dragging
  role="separator"
  aria-orientation={direction}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
></div>

<style>
  .splitter {
    flex: none;
    background: var(--border);
    position: relative;
    z-index: 5;
  }

  .splitter.vertical {
    width: 1px;
    cursor: col-resize;
  }

  .splitter.horizontal {
    height: 1px;
    cursor: row-resize;
  }

  .splitter::after {
    content: '';
    position: absolute;
    inset: -3px;
  }

  .splitter:hover,
  .splitter.dragging {
    background: var(--accent);
  }
</style>
