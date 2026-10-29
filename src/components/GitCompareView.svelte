<script lang="ts">
  import { untrack } from 'svelte';
  import { git, type GitTab } from '../lib/git.svelte';
  import { gitApi, pathTreeRows, shortHash, type Commit, type Compare } from '../lib/git';
  import { store } from '../lib/store.svelte';
  import { errorMessage } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Icon from './Icon.svelte';

  // 2 つのブランチ・コミットの比較（取り込むコミット・送るコミット・変わったファイル）
  let { tab: tabProp }: { tab: GitTab } = $props();
  const tab = untrack(() => tabProp);
  const c = tab.compare!;
  const label = (rev: string) => (/^[0-9a-f]{40}$/.test(rev) ? shortHash(rev) : rev === 'HEAD' ? (git.branch?.head ?? 'HEAD') : rev);

  let data = $state<Compare | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<string[]>([]);
  /** 変わったファイルで閉じたフォルダ */
  const closedDirs = new SvelteSet<string>();

  function toggleDir(path: string) {
    if (closedDirs.has(path)) closedDirs.delete(path);
    else closedDirs.add(path);
  }

  async function load() {
    try {
      data = await gitApi.compare(c.repo, c.base, c.target);
      error = null;
    } catch (e) {
      error = errorMessage(e);
    }
  }

  $effect(() => {
    void tab.version;
    untrack(() => void load());
  });

  function select(h: string, e: MouseEvent) {
    if (e.ctrlKey) selected = selected.includes(h) ? selected.filter((x) => x !== h) : [...selected, h];
    else selected = [h];
  }

  function menu(e: MouseEvent, commit: Commit) {
    if (!selected.includes(commit.hash)) selected = [commit.hash];
    const picks = selected;
    store.openMenu(e, [
      {
        label:
          picks.length > 1
            ? tr(`選んだ ${picks.length} 件を現在のブランチにチェリーピック…`, `Cherry-pick ${picks.length} commits into current branch…`, `将所选 ${picks.length} 个提交拣选到当前分支…`)
            : tr('現在のブランチにチェリーピック…', 'Cherry-pick into current branch…', '拣选到当前分支…'),
        action: () => void git.cherryPick(picks),
      },
      { label: tr('ハッシュをコピー', 'Copy hash', '复制哈希'), action: () => void navigator.clipboard.writeText(commit.hash) },
      { label: tr('この時点のファイルを見る', 'Browse files at this commit', '浏览此时的文件'), action: () => git.openTree(commit.hash) },
    ]);
  }
</script>

<div class="compare">
  <div class="bar">
    <span class="title"><code>{label(c.base)}</code> ↔ <code>{label(c.target)}</code></span>
    {#if c.base === 'HEAD' && data?.incoming.length}
      <button class="btn small" onclick={() => void git.merge(c.target, label(c.target))}>{tr(`${label(c.target)} をマージ…`, `Merge ${label(c.target)}…`, `合并 ${label(c.target)}…`)}</button>
      <button class="btn small" onclick={() => data && void git.cherryPick(data.incoming.map((x) => x.hash))}>{tr('チェリーピック…', 'Cherry-pick…', '拣选…')}</button>
    {/if}
    <button class="btn small" onclick={() => void load()}>{tr('読み直す', 'Refresh', '刷新')}</button>
  </div>
  {#if error}
    <div class="msg">{error}</div>
  {:else if !data}
    <div class="msg">{tr('読み込み中…', 'Loading…', '正在加载…')}</div>
  {:else}
    <div class="body">
      <section>
        <h3>{tr(`取り込むコミット（${label(c.base)} にないもの）`, `Incoming (not in ${label(c.base)})`, `传入的提交（${label(c.base)} 中没有）`)} <span class="count">{data.incoming.length}</span></h3>
        {#each data.incoming as cm (cm.hash)}
          <div class="row" class:selected={selected.includes(cm.hash)} role="button" tabindex="0" title={`${cm.author}\n${new Date(cm.date * 1000).toLocaleString()}`} onclick={(e) => select(cm.hash, e)} onkeydown={() => {}} oncontextmenu={(e) => menu(e, cm)}>
            <code>{shortHash(cm.hash)}</code><span class="subject">{cm.subject}</span><span class="when">{cm.author} · {git.relTime(cm.date)}</span>
          </div>
        {:else}
          <div class="none">{tr('なし', 'None', '无')}</div>
        {/each}
      </section>
      <section>
        <h3>{tr(`送るコミット（${label(c.target)} にないもの）`, `Outgoing (not in ${label(c.target)})`, `传出的提交（${label(c.target)} 中没有）`)} <span class="count">{data.outgoing.length}</span></h3>
        {#each data.outgoing as cm (cm.hash)}
          <div class="row" role="button" tabindex="0" title={cm.author} onclick={(e) => select(cm.hash, e)} onkeydown={() => {}} oncontextmenu={(e) => menu(e, cm)} class:selected={selected.includes(cm.hash)}>
            <code>{shortHash(cm.hash)}</code><span class="subject">{cm.subject}</span><span class="when">{cm.author} · {git.relTime(cm.date)}</span>
          </div>
        {:else}
          <div class="none">{tr('なし', 'None', '无')}</div>
        {/each}
      </section>
      <section>
        <h3>{tr('変わったファイル', 'Changed files', '更改的文件')} <span class="count">{data.files.length}</span></h3>
        {#each pathTreeRows(data.files, (p) => closedDirs.has(p)) as r (r.kind === 'dir' ? `d:${r.path}` : r.item.path)}
          {#if r.kind === 'dir'}
            <div class="row folder" role="button" tabindex="0" title={r.path} onclick={() => toggleDir(r.path)} onkeydown={(e) => e.key === 'Enter' && toggleDir(r.path)} style:padding-left="{14 + r.depth * 16}px">
              <Icon name={closedDirs.has(r.path) ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
              <span class="fname">{r.name}</span>
              <span class="when">{r.count}</span>
            </div>
          {:else}
            {@const f = r.item}
            <div class="row" role="button" tabindex="0" title={f.orig ? `${f.orig} → ${f.path}` : f.path} onclick={() => git.openCompareFile(c.base, c.target, f)} onkeydown={(e) => e.key === 'Enter' && git.openCompareFile(c.base, c.target, f)} style:padding-left="{14 + r.depth * 16}px">
              <span class="code c-{f.status}">{f.status}</span><span class="fname">{r.name}</span>
            </div>
          {/if}
        {:else}
          <div class="none">{tr('なし', 'None', '无')}</div>
        {/each}
      </section>
    </div>
  {/if}
</div>

<style>
  .compare {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  code {
    font-family: var(--editor-mono);
    font-size: 12px;
  }

  .btn.small {
    padding: 1px 8px;
    font-size: 12px;
  }

  .body {
    flex: 1;
    overflow: auto;
    padding: 8px 0 24px;
  }

  h3 {
    font-size: 12px;
    margin: 10px 14px 4px;
    color: var(--fg-muted);
  }

  .count {
    font-weight: 400;
    background: var(--bg-active);
    border-radius: 8px;
    padding: 0 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 14px;
    cursor: pointer;
    white-space: nowrap;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row.selected {
    background: var(--bg-active);
  }

  .row.folder {
    gap: 4px;
    color: var(--fg-muted);
  }

  .row code {
    color: var(--fg-muted);
    flex: none;
  }

  .subject,
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .subject {
    flex: 1;
    min-width: 0;
  }

  .when {
    font-size: 12px;
    color: var(--fg-muted);
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .code {
    width: 12px;
    font-size: 11px;
    font-weight: 700;
    flex: none;
  }

  .c-M,
  .c-R,
  .c-C,
  .c-T {
    color: #d29922;
  }

  .c-A {
    color: #2da44e;
  }

  .c-D {
    color: #cf222e;
  }

  .none {
    padding: 2px 28px;
    color: var(--fg-muted);
    font-size: 12px;
  }

  .msg {
    padding: 16px;
    color: var(--fg-muted);
    white-space: pre-wrap;
  }
</style>
