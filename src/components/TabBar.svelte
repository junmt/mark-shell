<script lang="ts">
  import { store, type Doc } from '../lib/store.svelte';
  import { basename, dirname } from '../lib/paths';
  import { tr } from '../lib/i18n.svelte';

  function label(doc: Doc): string {
    const name = basename(doc.path);
    // 同じ名前のタブがあるときはフォルダ名も出す
    const same = store.docs.filter((d) => basename(d.path) === name);
    return same.length > 1 ? `${name} — ${dirname(doc.path) || '/'}` : name;
  }

  function menu(e: MouseEvent, doc: Doc) {
    store.openMenu(e, [
      { label: tr('閉じる', 'Close', '关闭'), shortcut: 'Ctrl+W', action: () => void store.closeTab(doc.path) },
      {
        label: tr('ほかのタブを閉じる', 'Close other tabs', '关闭其他标签页'),
        action: async () => {
          for (const d of store.docs.filter((x) => x !== doc)) await store.closeTab(d.path);
        },
      },
      {
        label: tr('すべて閉じる', 'Close all', '全部关闭'),
        action: async () => {
          for (const d of [...store.docs]) await store.closeTab(d.path);
        },
      },
      { label: '', separator: true },
      { label: tr('ツリーで表示', 'Reveal in tree', '在树中显示'), action: () => store.showLeft('files') },
      { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void store.copyPath(doc.path, false) },
    ]);
  }
</script>

<div class="tabbar" role="tablist">
  {#each store.docs as doc (doc)}
    <div
      class="tab"
      class:active={doc.path === store.activePath}
      class:preview={!doc.pinned}
      role="tab"
      tabindex="0"
      aria-selected={doc.path === store.activePath}
      title={doc.path}
      onclick={() => store.activate(doc.path)}
      ondblclick={() => (doc.pinned = true)}
      onauxclick={(e) => e.button === 1 && void store.closeTab(doc.path)}
      onkeydown={(e) => e.key === 'Enter' && store.activate(doc.path)}
      oncontextmenu={(e) => menu(e, doc)}
    >
      <span class="name">{label(doc)}</span>
      <button
        class="close"
        class:dirty={doc.dirty}
        title={doc.dirty ? tr('未保存（閉じると保存します）', 'Unsaved (saved when closed)', '未保存（关闭时保存）') : tr('閉じる', 'Close', '关闭')}
        onclick={(e) => {
          e.stopPropagation();
          void store.closeTab(doc.path);
        }}
      >
        <span class="x">✕</span><span class="d">●</span>
      </button>
    </div>
  {/each}
</div>

<style>
  .tabbar {
    display: flex;
    flex: none;
    overflow-x: auto;
    background: var(--bg-pane);
    border-bottom: 1px solid var(--border);
    min-height: 34px;
  }

  .tabbar::-webkit-scrollbar {
    height: 3px;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 6px 0 12px;
    height: 34px;
    border-right: 1px solid var(--border);
    cursor: pointer;
    white-space: nowrap;
    color: var(--fg-muted);
    max-width: 260px;
  }

  .tab.active {
    background: var(--bg);
    color: var(--fg);
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .tab.preview .name {
    font-style: italic;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .close {
    border: none;
    background: transparent;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    color: var(--fg-muted);
    flex: none;
  }

  .close:hover {
    background: var(--bg-hover);
  }

  .close .d {
    display: none;
  }

  .close.dirty .x {
    display: none;
  }

  .close.dirty .d {
    display: inline;
  }

  .close.dirty:hover .x {
    display: inline;
  }

  .close.dirty:hover .d {
    display: none;
  }

  .tab:not(.active):not(:hover) .close:not(.dirty) {
    visibility: hidden;
  }
</style>
