<script lang="ts">
  import { store, type MenuItem } from '../lib/store.svelte';
  import { api } from '../lib/api';

  function openFolderMenu(e: MouseEvent) {
    void api.getRecent().then((recent) => {
      const items: MenuItem[] = [
        { label: 'フォルダを開く…', shortcut: 'Ctrl+O', action: () => void store.chooseRoot() },
        { label: '最近開いたフォルダから選ぶ…', shortcut: 'Ctrl+R', action: () => void store.chooseRecentRoot() },
        { label: '新しいウィンドウ', action: () => void api.openNewWindow() },
      ];
      if (recent.length) {
        items.push({ label: '', separator: true });
        for (const r of recent.slice(0, 10)) items.push({ label: r, action: () => void store.openRoot(r) });
      }
      store.menu = { x: (e.currentTarget as HTMLElement).getBoundingClientRect().left, y: 34, items };
    });
  }

  function openAiMenu(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const found = store.aiTools.filter((t) => t.found);
    const missing = store.aiTools.filter((t) => !t.found);
    const items: MenuItem[] = [];
    if (found.length === 0) items.push({ label: 'インストール済みの AI ツールが見つかりません', disabled: true });
    for (const t of found) {
      items.push({ label: `${t.name} を起動`, action: () => void store.launchTool(t) });
      if (store.activeDoc) {
        for (const a of t.actions) items.push({ label: `　${a.label}`, action: () => void store.launchTool(t, a.id) });
      }
    }
    if (missing.length) {
      items.push({ label: '', separator: true });
      for (const t of missing) items.push({ label: `${t.name}（未インストール）`, action: () => void store.launchTool(t) });
    }
    items.push({ label: '', separator: true });
    items.push({ label: 'AI 向けの説明ファイルを作成', action: () => void store.createAiGuide() });
    items.push({ label: 'AI ツールを探し直す', action: () => void store.refreshAiTools() });
    store.menu = { x: Math.min(rect.left, window.innerWidth - 300), y: rect.bottom + 2, items };
  }
</script>

<header class="toolbar">
  <button class="icon-btn" title="左ペイン (Ctrl+Shift+E)" class:on={store.leftVisible} onclick={() => store.toggleLeft()} disabled={!store.root}>☰</button>
  <button class="folder" onclick={openFolderMenu} title="フォルダを開く (Ctrl+O)">
    📁 {store.root ? store.rootName : 'フォルダを開く'} ▾
  </button>
  <div class="spacer"></div>
  {#if store.root}
    <button class="icon-btn" title="新しいファイル (Ctrl+N)" onclick={() => void store.newFile()}>＋ 新規</button>
    <button class="icon-btn" title="コマンドパレット (Ctrl+Shift+P)" onclick={() => (store.palette = 'commands')}>⌘ コマンド</button>
    <button class="icon-btn" class:on={store.terminalVisible} title="ターミナル (Ctrl+`)" onclick={() => store.toggleTerminal()}>&gt;_ ターミナル</button>
    <button class="icon-btn ai" title="AI ツール" onclick={openAiMenu}>✦ AI ▾</button>
  {/if}
  <button class="icon-btn" title="設定 (Ctrl+,)" onclick={() => (store.settingsOpen = true)}>⚙</button>
</header>

<style>
  .toolbar {
    height: 36px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-pane);
  }

  .folder {
    border: none;
    background: transparent;
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 600;
    max-width: 40vw;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder:hover {
    background: var(--bg-hover);
  }

  .spacer {
    flex: 1;
  }

  .ai {
    color: var(--accent);
    font-weight: 600;
  }
</style>
