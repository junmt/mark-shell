<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';

  /** ようこそ画面に並べる数（履歴は 100 件まで持ち、残りは Ctrl+R で探す） */
  const WELCOME_RECENT = 20;

  async function forget(e: MouseEvent, path: string) {
    e.stopPropagation();
    await api.removeRecent(path);
    store.recent = await api.getRecent();
  }

  function recentMenu(e: MouseEvent, path: string) {
    store.openMenu(e, [
      { label: tr('このウィンドウで開く', 'Open in this window', '在此窗口中打开'), action: () => void store.openRoot(path) },
      { label: tr('新しいウィンドウで開く', 'Open in new window', '在新窗口中打开'), action: () => void api.openNewWindow(path) },
      { label: '', separator: true },
      { label: tr('一覧から消す', 'Remove from list', '从列表中移除'), action: () => void api.removeRecent(path).then(async () => (store.recent = await api.getRecent())) },
    ]);
  }
</script>

<div class="welcome">
  <div class="box">
    <h1>mark-shell</h1>
    <p class="lead">
      {tr(
        'フォルダを開くと、その中のファイルを一覧して、Markdown やテキストファイルを編集できます。',
        'Open a folder to browse its files and edit Markdown and text files.',
        '打开文件夹即可浏览其中的文件，并编辑 Markdown 和文本文件。',
      )}
    </p>
    <button class="btn primary" onclick={() => void store.chooseRoot()}>{tr('フォルダを開く (Ctrl+O)', 'Open folder (Ctrl+O)', '打开文件夹 (Ctrl+O)')}</button>
    <p class="hint">{tr('フォルダをこのウィンドウにドラッグ＆ドロップしても開けます。', 'You can also drag and drop a folder onto this window.', '也可以将文件夹拖放到此窗口中打开。')}</p>

    {#if store.recent.length}
      <h2>{tr('最近開いたフォルダ', 'Recent folders', '最近打开的文件夹')}</h2>
      <ul>
        {#each store.recent.slice(0, WELCOME_RECENT) as r (r)}
          <li>
            <button class="recent" onclick={() => void store.openRoot(r)} oncontextmenu={(e) => recentMenu(e, r)} title={r}>
              <span class="name">{r.split(/[\\/]/).filter(Boolean).pop()}</span>
              <span class="path">{r}</span>
            </button>
            <button class="icon-btn" title={tr('一覧から消す', 'Remove from list', '从列表中移除')} onclick={(e) => forget(e, r)}>✕</button>
          </li>
        {/each}
      </ul>
      {#if store.recent.length > WELCOME_RECENT}
        <p class="hint">
          {tr(
            `ほかに ${store.recent.length - WELCOME_RECENT} 件あります。`,
            `${store.recent.length - WELCOME_RECENT} more.`,
            `还有 ${store.recent.length - WELCOME_RECENT} 项。`,
          )}
          <button class="link" onclick={() => void store.chooseRecentRoot()}>{tr('名前で探す (Ctrl+R)', 'Search by name (Ctrl+R)', '按名称查找 (Ctrl+R)')}</button>
        </p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .welcome {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg);
  }

  .box {
    width: min(560px, 90%);
  }

  h1 {
    font-size: 28px;
    margin: 0 0 8px;
  }

  h2 {
    font-size: 14px;
    margin: 32px 0 8px;
    color: var(--fg-muted);
  }

  .lead {
    color: var(--fg-muted);
    margin: 0 0 20px;
  }

  .hint {
    color: var(--fg-muted);
    font-size: 12px;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--md-link);
    cursor: pointer;
    font-size: inherit;
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  li {
    display: flex;
    align-items: center;
  }

  .recent {
    flex: 1;
    display: flex;
    gap: 12px;
    align-items: baseline;
    text-align: left;
    border: none;
    background: transparent;
    padding: 6px 8px;
    border-radius: 4px;
    cursor: pointer;
    min-width: 0;
  }

  .recent:hover {
    background: var(--bg-hover);
  }

  .name {
    font-weight: 600;
    color: var(--accent);
  }

  .path {
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
