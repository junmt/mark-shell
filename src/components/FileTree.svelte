<script lang="ts">
  import { store, type MenuItem } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { basename, dirname, isInside, join } from '../lib/paths';
  import type { TreeNode } from '../lib/tree';

  const ROW = 24;
  let listEl: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let height = $state(400);

  // 行数が多くても軽いように、見えている範囲だけを描く
  const visible = $derived.by(() => {
    const rows = store.rows;
    const start = Math.max(0, Math.floor(scrollTop / ROW) - 10);
    const end = Math.min(rows.length, Math.ceil((scrollTop + height) / ROW) + 10);
    return { start, items: rows.slice(start, end), total: rows.length };
  });

  function displayName(node: TreeNode): string {
    if (node.dir) return node.name;
    return node.name.replace(/\.(md|markdown|mdx)$/i, '');
  }

  function click(node: TreeNode, e: MouseEvent) {
    store.selected = node.path;
    if (node.dir) {
      toggle(node.path);
    } else {
      void store.openFile(node.path, { pin: e.detail >= 2 });
    }
  }

  function toggle(path: string) {
    if (store.expanded.has(path)) store.expanded.delete(path);
    else store.expanded.add(path);
    store.saveWorkspaceState();
  }

  function menuFor(node: TreeNode | null): MenuItem[] {
    const dir = node ? (node.dir ? node.path : dirname(node.path)) : '';
    const items: MenuItem[] = [
      { label: '新しいファイル', shortcut: 'Ctrl+N', action: () => void store.newFile(dir) },
      { label: '新しいフォルダ', action: () => void store.newFolder(dir) },
    ];
    if (node) {
      items.push(
        { label: '', separator: true },
        { label: '名前の変更', shortcut: 'F2', action: () => void store.rename(node.path) },
        { label: '複製', action: () => void store.duplicate(node.path) },
        { label: '削除', shortcut: 'Delete', action: () => void store.remove(node.path) },
        { label: '', separator: true },
        { label: 'パスをコピー', action: () => void store.copyPath(node.path, false) },
        { label: '相対パスをコピー', action: () => void store.copyPath(node.path, true) },
      );
      if (!node.dir) items.push({ label: '新しいタブで開く', action: () => void store.openFile(node.path, { pin: true }) });
    }
    items.push(
      { label: '', separator: true },
      { label: 'エクスプローラーで表示', action: () => void api.revealInExplorer(node?.path ?? '') },
      { label: 'ここでターミナルを開く', action: () => store.newTerminal({ cwd: dir }) },
      { label: '', separator: true },
      { label: 'フォルダを読み込み直す', action: () => void store.rescan() },
    );
    return items;
  }

  function contextMenu(e: MouseEvent, node: TreeNode | null) {
    e.stopPropagation();
    if (node) store.selected = node.path;
    store.openMenu(e, menuFor(node));
  }

  function keydown(e: KeyboardEvent) {
    const rows = store.rows;
    const i = rows.findIndex((r) => r.node.path === store.selected);
    const cur = rows[i]?.node;
    const select = (j: number) => {
      const r = rows[Math.max(0, Math.min(rows.length - 1, j))];
      if (!r) return;
      store.selected = r.node.path;
      ensureVisible(Math.max(0, Math.min(rows.length - 1, j)));
    };
    switch (e.key) {
      case 'ArrowDown':
        select(i + 1);
        break;
      case 'ArrowUp':
        select(i - 1);
        break;
      case 'ArrowRight':
        if (cur?.dir && !store.expanded.has(cur.path)) toggle(cur.path);
        else select(i + 1);
        break;
      case 'ArrowLeft':
        if (cur?.dir && store.expanded.has(cur.path)) toggle(cur.path);
        else if (cur) {
          const parent = dirname(cur.path);
          const j = rows.findIndex((r) => r.node.path === parent);
          if (j >= 0) select(j);
        }
        break;
      case 'Enter':
        if (cur?.dir) toggle(cur.path);
        else if (cur) void store.openFile(cur.path, { pin: true });
        break;
      case 'F2':
        if (cur) void store.rename(cur.path);
        break;
      case 'Delete':
        if (cur) void store.remove(cur.path);
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  function ensureVisible(index: number) {
    if (!listEl) return;
    const top = index * ROW;
    if (top < listEl.scrollTop) listEl.scrollTop = top;
    else if (top + ROW > listEl.scrollTop + listEl.clientHeight) listEl.scrollTop = top + ROW - listEl.clientHeight;
  }

  // ---- ドラッグで移動（HTML5 の DnD は Tauri のファイルドロップと競合するのでポインターで実装） ----
  let drag = $state<{ path: string; x: number; y: number; active: boolean } | null>(null);
  let dropTarget = $state<string | null>(null);

  function pointerDown(e: PointerEvent, node: TreeNode) {
    if (e.button !== 0) return;
    drag = { path: node.path, x: e.clientX, y: e.clientY, active: false };
  }

  function pointerMove(e: PointerEvent) {
    if (!drag) return;
    if (!drag.active && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) > 5) drag.active = true;
    if (!drag.active) return;
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-path]');
    const path = el?.dataset.path;
    if (path === undefined) {
      dropTarget = el || listEl?.contains(document.elementFromPoint(e.clientX, e.clientY)) ? '' : null;
      return;
    }
    const entry = store.entries.get(path);
    const dir = entry?.dir ? path : dirname(path);
    dropTarget = isInside(dir, drag.path) || dir === dirname(drag.path) ? null : dir;
  }

  function pointerUp() {
    const d = drag;
    const target = dropTarget;
    drag = null;
    dropTarget = null;
    if (d?.active && target !== null) void store.move(d.path, join(target, basename(d.path)));
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} />

<div
  class="tree"
  class:drop-root={dropTarget === ''}
  bind:this={listEl}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = listEl?.scrollTop ?? 0)}
  oncontextmenu={(e) => contextMenu(e, null)}
  onkeydown={keydown}
  tabindex="0"
  role="tree"
>
  {#if store.scanning && visible.total === 0}
    <div class="empty">読み込み中…</div>
  {:else if visible.total === 0}
    <div class="empty">
      Markdown ファイルがありません。
      <button class="link" onclick={() => void store.newFile('')}>新しいファイルを作る</button>
    </div>
  {/if}
  <div class="spacer" style:height="{visible.total * ROW}px">
    {#each visible.items as row, i (row.node.path)}
      {@const node = row.node}
      <div
        class="row"
        class:selected={store.selected === node.path}
        class:active={store.activePath === node.path}
        class:drop={dropTarget !== null && dropTarget !== '' && dropTarget === node.path}
        class:dragging={drag?.active && drag.path === node.path}
        style:top="{(visible.start + i) * ROW}px"
        style:padding-left="{8 + row.depth * 14}px"
        data-path={node.path}
        role="treeitem"
        aria-selected={store.selected === node.path}
        aria-expanded={node.dir ? store.expanded.has(node.path) : undefined}
        tabindex="-1"
        title={node.path}
        onclick={(e) => click(node, e)}
        onkeydown={() => {}}
        oncontextmenu={(e) => contextMenu(e, node)}
        onpointerdown={(e) => pointerDown(e, node)}
      >
        <span class="twisty">{node.dir ? (store.expanded.has(node.path) ? '▾' : '▸') : ''}</span>
        <span class="icon">{node.dir ? '📁' : '📄'}</span>
        <span class="name">{displayName(node)}</span>
        {#if store.aiChanged.has(node.path)}<span class="badge" title="AI セッション中に変更されました">AI</span>{/if}
        {#if store.docs.find((d) => d.path === node.path)?.dirty}<span class="dot">●</span>{/if}
      </div>
    {/each}
  </div>
</div>
{#if store.scanning && visible.total > 0}
  <div class="status">読み込み中… {store.entries.size.toLocaleString()} 件</div>
{/if}

<style>
  .tree {
    flex: 1;
    overflow: auto;
    position: relative;
    outline: none;
  }

  .tree.drop-root {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }

  .spacer {
    position: relative;
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 24px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 8px;
    cursor: pointer;
    white-space: nowrap;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row.selected {
    background: var(--bg-active);
  }

  .row.active .name {
    font-weight: 600;
  }

  .row.drop {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row.dragging {
    opacity: 0.5;
  }

  .twisty {
    width: 10px;
    color: var(--fg-muted);
    font-size: 10px;
    flex: none;
  }

  .icon {
    font-size: 12px;
    flex: none;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }

  .badge {
    font-size: 10px;
    padding: 0 4px;
    border-radius: 3px;
    background: var(--accent);
    color: var(--accent-fg);
  }

  .dot {
    color: var(--fg-muted);
    font-size: 10px;
  }

  .empty {
    padding: 16px;
    color: var(--fg-muted);
    line-height: 1.8;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
  }

  .status {
    flex: none;
    padding: 4px 8px;
    font-size: 11px;
    color: var(--fg-muted);
    border-top: 1px solid var(--border);
  }
</style>
