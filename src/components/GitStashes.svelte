<script lang="ts">
  import { git } from '../lib/git.svelte';
  import { gitApi, type ChangedFile, type Stash } from '../lib/git';
  import { store } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { errorMessage } from '../lib/api';
  import { basename, dirname } from '../lib/paths';
  import Icon from './Icon.svelte';

  let open = $state<number | null>(null);
  let files = $state<ChangedFile[]>([]);

  function toggleSection() {
    if (git.collapsed.has('stashes')) git.collapsed.delete('stashes');
    else git.collapsed.add('stashes');
  }

  async function toggle(s: Stash) {
    if (open === s.index) {
      open = null;
      return;
    }
    open = s.index;
    files = [];
    try {
      files = await gitApi.stashFiles(git.repoPath!, s.index);
    } catch (e) {
      store.toast(errorMessage(e), 'error');
    }
  }

  function menu(e: MouseEvent, s: Stash) {
    store.openMenu(e, [
      { label: tr('適用（残す）', 'Apply (keep)', '应用（保留）'), action: () => void git.stashApply(s.index, false) },
      { label: tr('取り出す（適用して削除）', 'Pop (apply and drop)', '弹出（应用并删除）'), action: () => void git.stashApply(s.index, true) },
      { label: '', separator: true },
      { label: tr('削除…', 'Drop…', '删除…'), action: () => void git.stashDrop(s.index) },
    ]);
  }
</script>

<div class="sec-head" role="button" tabindex="0" onclick={toggleSection} onkeydown={(e) => e.key === 'Enter' && toggleSection()}>
  <Icon name={git.collapsed.has('stashes') ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
  <span class="title">{tr('スタッシュ', 'Stashes', '贮藏')}</span>
  <span class="count">{git.stashes.length}</span>
  <span class="sec-actions">
    <button class="icon-btn" title={tr('スタッシュ', 'Stash changes', '贮藏更改')} onclick={(e) => (e.stopPropagation(), void git.stashPush())}><Icon name="plus" size={13} /></button>
  </span>
</div>
{#if !git.collapsed.has('stashes')}
  {#if git.stashes.length === 0}
    <div class="none">{tr('なし', 'None', '无')}</div>
  {/if}
  {#each git.stashes as s (s.index)}
    <div class="row" role="button" tabindex="0" onclick={() => void toggle(s)} onkeydown={(e) => e.key === 'Enter' && void toggle(s)} oncontextmenu={(e) => menu(e, s)} title={s.message}>
      <Icon name={open === s.index ? 'chevronDown' : 'chevronRight'} size={11} stroke={2} />
      <span class="name">{s.message}</span>
      <span class="when">{git.relTime(s.date)}</span>
      <span class="acts">
        <button class="icon-btn" title={tr('取り出す', 'Pop', '弹出')} onclick={(e) => (e.stopPropagation(), void git.stashApply(s.index, true))}><Icon name="arrowUp" size={12} /></button>
      </span>
    </div>
    {#if open === s.index}
      {#each files as f (f.path)}
        <div class="row file" role="button" tabindex="0" onclick={() => git.openStashFile(s.index, f)} onkeydown={(e) => e.key === 'Enter' && git.openStashFile(s.index, f)}>
          <span class="name">{basename(f.path)}</span>
          <span class="dir">{dirname(f.path)}</span>
          <span class="code">{f.status}</span>
        </div>
      {/each}
    {/if}
  {/each}
{/if}

<style>
  .sec-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    margin-top: 6px;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--fg-muted);
    user-select: none;
  }

  .sec-head:hover {
    background: var(--bg-hover);
  }

  .title {
    flex: 1;
  }

  .count {
    font-weight: 400;
    background: var(--bg-active);
    border-radius: 8px;
    padding: 0 6px;
  }

  .sec-actions,
  .acts {
    display: none;
  }

  .sec-head:hover .sec-actions,
  .row:hover .acts {
    display: flex;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px 0 18px;
    cursor: pointer;
    white-space: nowrap;
  }

  .row.file {
    padding-left: 36px;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row.file .name {
    flex: none;
    max-width: 60%;
  }

  .dir {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .when,
  .code {
    font-size: 11px;
    color: var(--fg-muted);
    flex: none;
  }

  .icon-btn {
    border: none;
    background: transparent;
    width: 20px;
    height: 20px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--fg-muted);
    padding: 0;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .none {
    padding: 2px 24px 4px;
    color: var(--fg-muted);
    font-size: 12px;
  }
</style>
