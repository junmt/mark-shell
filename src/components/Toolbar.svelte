<script lang="ts">
  import { store, type MenuItem } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  function openFolderMenu(e: MouseEvent) {
    void api.getRecent().then((recent) => {
      const items: MenuItem[] = [
        { label: tr('フォルダを開く…', 'Open folder…', '打开文件夹…'), shortcut: 'Ctrl+O', action: () => void store.chooseRoot() },
        { label: tr('最近開いたフォルダから選ぶ…', 'Open recent folder…', '从最近打开的文件夹中选择…'), shortcut: 'Ctrl+R', action: () => void store.chooseRecentRoot() },
        { label: tr('新しいウィンドウ', 'New window', '新建窗口'), action: () => void api.openNewWindow() },
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
    if (found.length === 0) items.push({ label: tr('インストール済みの AI ツールが見つかりません', 'No installed AI tools found', '未找到已安装的 AI 工具'), disabled: true });
    for (const t of found) {
      items.push({ label: tr(`${t.name} を起動`, `Launch ${t.name}`, `启动 ${t.name}`), action: () => void store.launchTool(t) });
      if (store.activeDoc) {
        for (const a of t.actions) items.push({ label: `　${a.label}`, action: () => void store.launchTool(t, a.id) });
      }
    }
    if (missing.length) {
      items.push({ label: '', separator: true });
      for (const t of missing) items.push({ label: tr(`${t.name}（未インストール）`, `${t.name} (not installed)`, `${t.name}（未安装）`), action: () => void store.launchTool(t) });
    }
    items.push({ label: '', separator: true });
    items.push({ label: tr('AI 向けの説明ファイルを作成', 'Create AI guide file', '创建面向 AI 的说明文件'), action: () => void store.createAiGuide() });
    items.push({ label: tr('AI ツールを探し直す', 'Rescan AI tools', '重新查找 AI 工具'), action: () => void store.refreshAiTools() });
    store.menu = { x: Math.min(rect.left, window.innerWidth - 300), y: rect.bottom + 2, items };
  }
</script>

<header class="toolbar">
  <button class="icon-btn" title={tr('左ペイン (Ctrl+Shift+E)', 'Side panel (Ctrl+Shift+E)', '左侧面板 (Ctrl+Shift+E)')} class:on={store.leftVisible} onclick={() => store.toggleLeft()} disabled={!store.root}>
    <Icon name="sidebar" />
  </button>
  <button class="folder" onclick={openFolderMenu} title={tr('フォルダを開く (Ctrl+O)', 'Open folder (Ctrl+O)', '打开文件夹 (Ctrl+O)')}>
    <Icon name="folder" />
    <span class="label">{store.root ? store.rootName : tr('フォルダを開く', 'Open folder', '打开文件夹')}</span>
    <Icon name="chevronDown" size={12} stroke={2} />
  </button>
  <div class="spacer"></div>
  {#if store.root}
    <button class="icon-btn" title={tr('新しいファイル (Ctrl+N)', 'New file (Ctrl+N)', '新建文件 (Ctrl+N)')} onclick={() => void store.newFile()}>
      <Icon name="plus" /><span>{tr('新規', 'New', '新建')}</span>
    </button>
    <button class="icon-btn" title={tr('コマンドパレット (Ctrl+Shift+P)', 'Command palette (Ctrl+Shift+P)', '命令面板 (Ctrl+Shift+P)')} onclick={() => (store.palette = 'commands')}>
      <Icon name="command" /><span>{tr('コマンド', 'Commands', '命令')}</span>
    </button>
    <button class="icon-btn" class:on={store.terminalVisible} title={tr('ターミナル (Ctrl+`)', 'Terminal (Ctrl+`)', '终端 (Ctrl+`)')} onclick={() => store.toggleTerminal()}>
      <Icon name="terminal" /><span>{tr('ターミナル', 'Terminal', '终端')}</span>
    </button>
    <button class="icon-btn ai" title={tr('AI ツール', 'AI tools', 'AI 工具')} onclick={openAiMenu}>
      <Icon name="sparkle" /><span>AI</span><Icon name="chevronDown" size={12} stroke={2} />
    </button>
  {/if}
  <button class="icon-btn" title={tr('設定 (Ctrl+,)', 'Settings (Ctrl+,)', '设置 (Ctrl+,)')} onclick={() => (store.settingsOpen = true)}>
    <Icon name="settings" />
  </button>
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

  .icon-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .folder {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: none;
    background: transparent;
    color: var(--fg);
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 600;
    max-width: 40vw;
    white-space: nowrap;
  }

  .folder .label {
    overflow: hidden;
    text-overflow: ellipsis;
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
