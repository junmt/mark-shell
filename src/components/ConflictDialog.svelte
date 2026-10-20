<script lang="ts">
  import { store } from '../lib/store.svelte';
  import DiffLines from './DiffLines.svelte';
</script>

{#if store.conflict}
  {@const c = store.conflict}
  <div class="backdrop" role="presentation"></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="外部での変更">
    <h2>{c.doc.path} が外部で変更されました</h2>
    <p>編集中の内容（保存前）と、ファイルの新しい内容が食い違っています。どちらを残すか選んでください。</p>
    <DiffLines before={c.disk.content} after={c.doc.text} beforeLabel="ファイルの内容（外部での変更）" afterLabel="編集中の内容" />
    <div class="buttons">
      <button class="btn" onclick={() => store.resolveConflict('both')} title="自分の変更は別名のファイルに保存し、エディタには外部の内容を読み込みます">両方を残す</button>
      <button class="btn" onclick={() => store.resolveConflict('theirs')}>外部の変更を取り込む</button>
      <button class="btn primary" onclick={() => store.resolveConflict('mine')}>自分の変更を残す</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 70;
  }

  .dialog {
    position: fixed;
    top: 8%;
    left: 50%;
    transform: translateX(-50%);
    width: min(820px, 92vw);
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 18px 20px 16px;
    z-index: 71;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    word-break: break-all;
  }

  p {
    margin: 0 0 12px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
