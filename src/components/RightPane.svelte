<script lang="ts">
  import { store } from '../lib/store.svelte';
  import TabBar from './TabBar.svelte';
  import DocView from './DocView.svelte';
  import TerminalPanel from './TerminalPanel.svelte';
  import Splitter from './Splitter.svelte';
  import { tr } from '../lib/i18n.svelte';

  let paneHeight = $state(600);
</script>

<div class="pane" bind:clientHeight={paneHeight}>
  <div class="docs">
    <TabBar />
    {#if store.activeDoc}
      {#key store.activeDoc}
        <DocView doc={store.activeDoc} />
      {/key}
    {:else}
      <div class="empty">
        <p>{tr('左のツリーからファイルを選ぶと、ここで編集できます。', 'Select a file in the tree on the left to edit it here.', '从左侧树中选择文件即可在此编辑。')}</p>
        <p class="keys">
          <kbd>Ctrl</kbd>+<kbd>P</kbd> {tr('ファイルを名前で開く', 'Open file by name', '按名称打开文件')}
          <kbd>Ctrl</kbd>+<kbd>N</kbd> {tr('新しいファイル', 'New file', '新建文件')}
          <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> {tr('全文検索', 'Search in files', '全文搜索')}
          <kbd>Ctrl</kbd>+<kbd>`</kbd> {tr('ターミナル', 'Terminal', '终端')}
        </p>
      </div>
    {/if}
  </div>
  {#if store.terminalVisible}
    <Splitter
      direction="horizontal"
      onresize={(d) => (store.terminalHeight = Math.min(Math.max(80, store.terminalHeight - d), paneHeight - 120))}
      ondone={() => store.saveWorkspaceState()}
    />
  {/if}
  <div class="terminal" style:height="{store.terminalHeight}px" class:hidden={!store.terminalVisible}>
    <TerminalPanel />
  </div>
</div>

<style>
  .pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .docs {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .terminal {
    flex: none;
    min-height: 80px;
    display: flex;
    flex-direction: column;
  }

  .terminal.hidden {
    display: none;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--fg-muted);
  }

  .keys {
    font-size: 12px;
  }

  kbd {
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 3px;
    padding: 0 4px;
    font-family: var(--ui-font);
    font-size: 11px;
  }
</style>
