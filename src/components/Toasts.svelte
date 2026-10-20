<script lang="ts">
  import { store } from '../lib/store.svelte';
</script>

<div class="toasts" aria-live="polite">
  {#each store.toasts as t (t.id)}
    <div class="toast" class:error={t.kind === 'error'} role={t.kind === 'error' ? 'alert' : 'status'}>
      <span>{t.message}</span>
      <button class="icon-btn" title="閉じる" onclick={() => (store.toasts = store.toasts.filter((x) => x.id !== t.id))}>✕</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 36px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    max-width: min(460px, 80vw);
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 8px 10px 14px;
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-left: 4px solid var(--accent);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
    white-space: pre-wrap;
    word-break: break-all;
    user-select: text;
  }

  .toast.error {
    border-left-color: var(--danger);
  }

  .toast span {
    flex: 1;
  }
</style>
