<script lang="ts">
  import { tick } from 'svelte';
  import { store } from '../lib/store.svelte';
  import { basename, dirname } from '../lib/paths';

  type Item = { key: string; label: string; detail?: string; shortcut?: string; run: () => void };

  let query = $state('');
  let index = $state(0);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (store.palette) {
      query = '';
      index = 0;
      void tick().then(() => input?.focus());
    }
  });

  /** 文字が順に含まれていれば一致。連続・先頭・語頭の一致ほど高得点 */
  function score(text: string, q: string): number {
    if (!q) return 1;
    const t = text.toLowerCase();
    let pos = -1;
    let s = 0;
    let prev = -2;
    for (const ch of q.toLowerCase()) {
      if (ch === ' ') continue;
      const i = t.indexOf(ch, pos + 1);
      if (i < 0) return 0;
      s += i === prev + 1 ? 5 : 1;
      if (i === 0 || '/ -_.'.includes(t[i - 1])) s += 3;
      prev = i;
      pos = i;
    }
    return s - text.length * 0.01;
  }

  const items = $derived.by((): Item[] => {
    if (store.palette === 'files') {
      const q = query.trim();
      return store
        .markdownFiles()
        .map((f) => ({ f, s: Math.max(score(basename(f), q) * 2, score(f, q)) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => b.s - a.s || a.f.localeCompare(b.f, 'ja'))
        .slice(0, 200)
        .map(({ f }) => ({
          key: f,
          label: basename(f),
          detail: dirname(f),
          run: () => void store.openFile(f, { pin: true }),
        }));
    }
    if (store.palette === 'commands') {
      return store
        .commands()
        .map((c) => ({ c, s: score(c.label, query.trim()) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => (query ? b.s - a.s : 0))
        .map(({ c }) => ({ key: c.id, label: c.label, shortcut: c.shortcut, run: c.run }));
    }
    return [];
  });

  function close() {
    store.palette = null;
  }

  function choose(item: Item | undefined) {
    if (!item) return;
    close();
    item.run();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowDown') index = Math.min(items.length - 1, index + 1);
    else if (e.key === 'ArrowUp') index = Math.max(0, index - 1);
    else if (e.key === 'Enter') choose(items[index]);
    else return;
    e.preventDefault();
  }

  $effect(() => {
    void query;
    index = 0;
  });

  $effect(() => {
    document.querySelector('.palette .item.on')?.scrollIntoView({ block: 'nearest' });
    void index;
  });
</script>

{#if store.palette}
  <div class="backdrop" role="presentation" onclick={close}></div>
  <div class="palette" role="dialog" aria-label={store.palette === 'files' ? 'ファイルを開く' : 'コマンド'}>
    <input
      bind:this={input}
      bind:value={query}
      onkeydown={keydown}
      placeholder={store.palette === 'files' ? 'ファイル名で検索' : 'コマンドを検索'}
      spellcheck="false"
    />
    <div class="list">
      {#each items as item, i (item.key)}
        <button class="item" class:on={i === index} onmouseenter={() => (index = i)} onclick={() => choose(item)}>
          <span class="label">{item.label}</span>
          {#if item.detail}<span class="detail">{item.detail}</span>{/if}
          {#if item.shortcut}<span class="shortcut">{item.shortcut}</span>{/if}
        </button>
      {:else}
        <div class="none">見つかりません</div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }

  .palette {
    position: fixed;
    top: 48px;
    left: 50%;
    transform: translateX(-50%);
    width: min(640px, 90vw);
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.25);
    z-index: 51;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  input {
    margin: 8px;
    padding: 7px 10px;
    font-size: 14px;
  }

  .list {
    max-height: 50vh;
    overflow: auto;
    padding-bottom: 6px;
  }

  .item {
    display: flex;
    width: 100%;
    align-items: baseline;
    gap: 10px;
    border: none;
    background: transparent;
    text-align: left;
    padding: 6px 14px;
    cursor: pointer;
  }

  .item.on {
    background: var(--bg-active);
  }

  .label {
    white-space: nowrap;
  }

  .detail {
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .shortcut {
    margin-left: auto;
    color: var(--fg-muted);
    font-size: 12px;
    white-space: nowrap;
  }

  .none {
    padding: 10px 14px;
    color: var(--fg-muted);
  }
</style>
