<script lang="ts">
  import { tick } from 'svelte';
  import { store } from '../lib/store.svelte';

  let value = $state('');
  let input: HTMLInputElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();

  $effect(() => {
    const d = store.dialog;
    if (!d) return;
    value = d.input?.value ?? '';
    void tick().then(() => {
      if (input) {
        input.focus();
        // ファイル名は拡張子の手前まで選ぶ
        const dot = value.lastIndexOf('.');
        if (d.input?.selectStem && dot > 0) input.setSelectionRange(0, dot);
        else input.select();
      } else {
        box?.querySelector<HTMLButtonElement>('button.primary, button.danger')?.focus();
      }
    });
  });

  function finish(button: string | null) {
    const d = store.dialog;
    if (!d) return;
    store.dialog = null;
    d.resolve(button === null ? null : { button, value });
  }

  function keydown(e: KeyboardEvent) {
    const d = store.dialog;
    if (!d) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      finish(null);
    } else if (e.key === 'Enter' && e.target instanceof HTMLInputElement) {
      e.preventDefault();
      finish(d.buttons.find((b) => b.primary)?.value ?? 'ok');
    }
  }
</script>

{#if store.dialog}
  {@const d = store.dialog}
  <div class="backdrop" role="presentation" onclick={() => finish(null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={d.title} bind:this={box} onkeydown={keydown} tabindex="-1">
    <h2>{d.title}</h2>
    {#if d.message}<p class="message">{d.message}</p>{/if}
    {#if d.input}
      <input bind:this={input} bind:value placeholder={d.input.placeholder ?? ''} spellcheck="false" />
    {/if}
    <div class="buttons">
      {#each d.buttons as b (b.value)}
        <button class="btn" class:primary={b.primary} class:danger={b.danger} onclick={() => finish(b.value)}>{b.label}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 80;
  }

  .dialog {
    position: fixed;
    top: 20%;
    left: 50%;
    transform: translateX(-50%);
    width: min(480px, 90vw);
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 18px 20px 16px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
    z-index: 81;
    outline: none;
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }

  .message {
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--fg);
    margin: 0 0 12px;
    max-height: 40vh;
    overflow: auto;
    user-select: text;
  }

  input {
    width: 100%;
    margin-bottom: 14px;
    padding: 6px 8px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
