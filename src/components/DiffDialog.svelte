<script lang="ts">
  import { store } from '../lib/store.svelte';
  import DiffLines from './DiffLines.svelte';
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && store.diff && (store.diff = null)} />

{#if store.diff}
  {@const d = store.diff}
  <div class="backdrop" role="presentation" onclick={() => (store.diff = null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={d.title}>
    <h2>{d.title}</h2>
    <DiffLines before={d.before} after={d.after} beforeLabel="変更前" afterLabel="変更後" />
    <div class="buttons">
      <button class="btn primary" onclick={() => (store.diff = null)}>閉じる</button>
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
    margin: 0 0 12px;
    font-size: 15px;
    word-break: break-all;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 14px;
  }
</style>
