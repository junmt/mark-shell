<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { api } from '../lib/api';

  async function forget(e: MouseEvent, path: string) {
    e.stopPropagation();
    await api.removeRecent(path);
    store.recent = await api.getRecent();
  }
</script>

<div class="welcome">
  <div class="box">
    <h1>mark-shell</h1>
    <p class="lead">フォルダを開くと、その中の Markdown ファイルを一覧して編集できます。</p>
    <button class="btn primary" onclick={() => void store.chooseRoot()}>フォルダを開く (Ctrl+O)</button>
    <p class="hint">フォルダをこのウィンドウにドラッグ＆ドロップしても開けます。</p>

    {#if store.recent.length}
      <h2>最近開いたフォルダ</h2>
      <ul>
        {#each store.recent as r (r)}
          <li>
            <button class="recent" onclick={() => void store.openRoot(r)} title={r}>
              <span class="name">{r.split(/[\\/]/).filter(Boolean).pop()}</span>
              <span class="path">{r}</span>
            </button>
            <button class="icon-btn" title="一覧から消す" onclick={(e) => forget(e, r)}>✕</button>
          </li>
        {/each}
      </ul>
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
