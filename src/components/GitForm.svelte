<script lang="ts">
  import { tick } from 'svelte';
  import { git } from '../lib/git.svelte';

  // 入力欄・チェックボックス・選択肢のある Git の操作のダイアログ（マージ、チェリーピックなど）
  let values = $state<Record<string, string | boolean>>({});
  let box: HTMLDivElement | undefined = $state();

  $effect(() => {
    const f = git.form;
    if (!f) return;
    const v: Record<string, string | boolean> = {};
    for (const field of f.fields ?? []) v[field.id] = field.value;
    values = v;
    void tick().then(() => {
      const first = box?.querySelector<HTMLElement>('input[type=text], textarea');
      if (first) first.focus();
      else box?.querySelector<HTMLButtonElement>('button.primary, button.danger')?.focus();
    });
  });

  function finish(button: string | null) {
    const f = git.form;
    if (!f) return;
    git.form = null;
    f.resolve(button === null ? null : { button, values: { ...values } });
  }

  function keydown(e: KeyboardEvent) {
    const f = git.form;
    if (!f) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      finish(null);
    } else if (e.key === 'Enter' && e.target instanceof HTMLInputElement && e.target.type === 'text') {
      e.preventDefault();
      finish(f.buttons.find((b) => b.primary)?.value ?? 'ok');
    }
  }
</script>

{#if git.form}
  {@const f = git.form}
  <div class="backdrop" role="presentation" onclick={() => finish(null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={f.title} bind:this={box} onkeydown={keydown} tabindex="-1">
    <h2>{f.title}</h2>
    {#if f.message}<p class="message">{f.message}</p>{/if}
    {#if f.items?.length}
      <ul class="items">
        {#each f.items as item, i (i)}
          <li class:muted={item.muted}>{#if item.mark}<code>{item.mark}</code>{/if}<span>{item.text}</span></li>
        {/each}
      </ul>
    {/if}
    {#if f.output}<pre class="output">{f.output}</pre>{/if}
    {#each f.fields ?? [] as field (field.id)}
      {#if field.kind === 'text'}
        <label class="text">
          <span>{field.label}</span>
          {#if field.multiline}
            <textarea rows="4" bind:value={values[field.id] as string} placeholder={field.placeholder ?? ''} spellcheck="false"></textarea>
          {:else}
            <input type="text" bind:value={values[field.id] as string} placeholder={field.placeholder ?? ''} spellcheck="false" />
          {/if}
        </label>
      {:else if field.kind === 'check'}
        <label class="check"><input type="checkbox" bind:checked={values[field.id] as boolean} /> <span>{field.label}</span></label>
      {:else}
        <fieldset>
          {#if field.label}<legend>{field.label}</legend>{/if}
          {#each field.options as o (o.value)}
            <label class="check"><input type="radio" name={field.id} value={o.value} bind:group={values[field.id]} /> <span>{o.label}</span></label>
          {/each}
        </fieldset>
      {/if}
    {/each}
    <div class="buttons">
      {#each f.buttons as b (b.value)}
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
    top: 12%;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, 92vw);
    max-height: 76vh;
    overflow: auto;
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
    margin: 0 0 12px;
    user-select: text;
  }

  .items {
    list-style: none;
    margin: 0 0 12px;
    padding: 4px 0;
    max-height: 180px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
  }

  .items li {
    display: flex;
    gap: 8px;
    padding: 2px 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .items li.muted {
    color: var(--fg-muted);
  }

  .items code {
    color: var(--fg-muted);
    font-family: var(--editor-mono);
    font-size: 12px;
  }

  .output {
    font-family: var(--editor-mono);
    font-size: 12px;
    white-space: pre-wrap;
    word-break: break-all;
    margin: 0 0 12px;
    padding: 8px 10px;
    max-height: 30vh;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    user-select: text;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  .text span {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .text input,
  .text textarea {
    width: 100%;
    padding: 6px 8px;
    box-sizing: border-box;
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 4px 0;
    cursor: pointer;
    word-break: break-all;
  }

  .check input {
    margin-top: 3px;
  }

  fieldset {
    border: none;
    padding: 0;
    margin: 8px 0;
  }

  legend {
    font-size: 12px;
    color: var(--fg-muted);
    padding: 0;
    margin-bottom: 2px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
