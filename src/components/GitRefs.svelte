<script lang="ts">
  import { git } from '../lib/git.svelte';
  import type { Branch, RemoteBranch, Tag } from '../lib/git';
  import { store, type MenuItem } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  function toggle(id: string) {
    if (git.collapsed.has(id)) git.collapsed.delete(id);
    else git.collapsed.add(id);
  }

  /** `feature/x` のような / 区切りをフォルダとして並べる */
  type Row<T> = { kind: 'folder'; key: string; name: string; depth: number } | { kind: 'item'; item: T; name: string; depth: number };
  function treeRows<T>(items: T[], nameOf: (t: T) => string, prefix: string): Row<T>[] {
    const sorted = [...items].sort((a, b) => nameOf(a).localeCompare(nameOf(b), 'ja'));
    const rows: Row<T>[] = [];
    const seen = new Set<string>();
    for (const it of sorted) {
      const parts = nameOf(it).split('/');
      let hidden = false;
      for (let d = 0; d < parts.length - 1; d++) {
        const key = prefix + parts.slice(0, d + 1).join('/');
        if (!seen.has(key)) {
          seen.add(key);
          if (!hidden) rows.push({ kind: 'folder', key, name: parts[d], depth: d });
        }
        if (!git.collapsed.has('open:' + key)) hidden = true;
      }
      if (!hidden) rows.push({ kind: 'item', item: it, name: parts[parts.length - 1], depth: parts.length - 1 });
    }
    return rows;
  }

  const branchRows = $derived(treeRows(git.refs?.branches ?? [], (b) => b.name, 'b:'));
  const tagRows = $derived([...(git.refs?.tags ?? [])].sort((a, b) => b.name.localeCompare(a.name, 'ja', { numeric: true })));

  function remoteRows(remote: string) {
    return treeRows(
      (git.refs?.remoteBranches ?? []).filter((r) => r.remote === remote),
      (r) => r.name,
      `r:${remote}:`,
    );
  }

  /** リモートのブランチを上流にしているローカルのブランチ */
  function localOf(r: RemoteBranch): Branch | undefined {
    return git.refs?.branches.find((b) => b.upstream === `${r.remote}/${r.name}`);
  }

  function lastFetch(): string {
    const t = git.status?.lastFetch;
    return t ? git.relTime(t / 1000) : tr('未取得', 'never', '从未');
  }

  function branchMenu(e: MouseEvent, b: Branch) {
    const items: MenuItem[] = [
      { label: tr('切り替え', 'Switch to', '切换到'), disabled: b.current, action: () => void git.checkout(b.name) },
      { label: tr('現在のブランチにマージ…', 'Merge into current branch…', '合并到当前分支…'), disabled: b.current, action: () => void git.merge(b.name) },
      { label: tr('現在のブランチと比較', 'Compare with current branch', '与当前分支比较'), disabled: b.current, action: () => git.openCompare('HEAD', b.name) },
      { label: tr('履歴を見る', 'Show history', '查看历史'), action: () => void git.showHistory(b.current ? 'HEAD' : b.name) },
      { label: tr('この時点のファイルを見る', 'Browse files', '浏览文件'), action: () => git.openTree(b.name) },
      { label: '', separator: true },
      { label: tr('ここからブランチを作る…', 'Create branch from here…', '从此处创建分支…'), action: () => void git.createBranch(b.name) },
      { label: tr('名前の変更…', 'Rename…', '重命名…'), action: () => void git.renameBranch(b.name) },
      { label: tr('削除…', 'Delete…', '删除…'), disabled: b.current, action: () => void git.deleteBranch(b.name) },
      { label: '', separator: true },
      { label: tr('名前をコピー', 'Copy name', '复制名称'), action: () => void navigator.clipboard.writeText(b.name) },
    ];
    store.openMenu(e, items);
  }

  function remoteBranchMenu(e: MouseEvent, r: RemoteBranch) {
    const full = `${r.remote}/${r.name}`;
    store.openMenu(e, [
      { label: tr('このブランチだけフェッチ', 'Fetch this branch', '仅获取此分支'), action: () => void git.fetch(r.remote, r.name) },
      { label: tr('チェックアウト', 'Check out', '检出'), action: () => void git.checkoutRemote(r.remote, r.name) },
      { label: tr('現在のブランチにマージ…', 'Merge into current branch…', '合并到当前分支…'), action: () => void git.merge(full) },
      { label: tr('現在のブランチとの比較', 'Compare with current branch', '与当前分支比较'), action: () => git.openCompare('HEAD', full) },
      { label: tr('履歴を見る', 'Show history', '查看历史'), action: () => void git.showHistory(full) },
      { label: tr('この時点のファイルを見る', 'Browse files', '浏览文件'), action: () => git.openTree(full) },
      { label: '', separator: true },
      { label: tr('ここからブランチを作る…', 'Create branch from here…', '从此处创建分支…'), action: () => void git.createBranch(full) },
      { label: tr('名前をコピー', 'Copy name', '复制名称'), action: () => void navigator.clipboard.writeText(full) },
      { label: '', separator: true },
      { label: tr('リモートのブランチを削除…', 'Delete remote branch…', '删除远程分支…'), action: () => void git.deleteRemoteBranch(r.remote, r.name) },
    ]);
  }

  function remoteMenu(e: MouseEvent, name: string, url: string) {
    store.openMenu(e, [
      { label: tr('フェッチ', 'Fetch', '获取'), action: () => void git.fetch(name) },
      { label: tr('URL の変更…', 'Change URL…', '更改 URL…'), action: () => void git.editRemoteUrl(name, url) },
      { label: tr('URL をコピー', 'Copy URL', '复制 URL'), action: () => void navigator.clipboard.writeText(url) },
      { label: tr('削除…', 'Remove…', '删除…'), action: () => void git.removeRemote(name) },
    ]);
  }

  function tagMenu(e: MouseEvent, t: Tag) {
    store.openMenu(e, [
      { label: tr('履歴を見る', 'Show history', '查看历史'), action: () => void git.showHistory(t.name) },
      { label: tr('現在のブランチにマージ…', 'Merge into current branch…', '合并到当前分支…'), action: () => void git.merge(t.name) },
      { label: tr('この時点のファイルを見る', 'Browse files', '浏览文件'), action: () => git.openTree(t.name) },
      { label: tr('ここからブランチを作る…', 'Create branch from here…', '从此处创建分支…'), action: () => void git.createBranch(t.name) },
      { label: '', separator: true },
      { label: tr('プッシュ', 'Push', '推送'), disabled: !git.refs?.remotes.length, action: () => void git.pushTag(t.name) },
      { label: tr('削除…', 'Delete…', '删除…'), action: () => void git.deleteTag(t.name) },
    ]);
  }
</script>

<!-- ブランチ -->
<div class="sec-head" role="button" tabindex="0" onclick={() => toggle('branches')} onkeydown={(e) => e.key === 'Enter' && toggle('branches')}>
  <Icon name={git.collapsed.has('branches') ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
  <span class="title">{tr('ブランチ', 'Branches', '分支')}</span>
  <span class="count">{git.refs?.branches.length ?? 0}</span>
  <span class="sec-actions">
    <button class="icon-btn" title={tr('ブランチの作成', 'Create branch', '创建分支')} onclick={(e) => (e.stopPropagation(), void git.createBranch())}><Icon name="plus" size={13} /></button>
  </span>
</div>
{#if !git.collapsed.has('branches')}
  {#each branchRows as row (row.kind === 'folder' ? row.key : 'b:' + row.item.name)}
    {#if row.kind === 'folder'}
      <div class="row folder" style:padding-left="{20 + row.depth * 12}px" role="button" tabindex="0" onclick={() => toggle('open:' + row.key)} onkeydown={(e) => e.key === 'Enter' && toggle('open:' + row.key)}>
        <Icon name={git.collapsed.has('open:' + row.key) ? 'chevronDown' : 'chevronRight'} size={11} stroke={2} />
        {row.name}
      </div>
    {:else}
      {@const b = row.item}
      <div
        class="row"
        class:current={b.current}
        style:padding-left="{20 + row.depth * 12}px"
        role="button"
        tabindex="0"
        title={`${b.name}${b.upstream ? ' → ' + b.upstream : ''}\n${b.subject}`}
        ondblclick={() => !b.current && void git.checkout(b.name)}
        onclick={() => void git.showHistory(b.current ? 'HEAD' : b.name)}
        onkeydown={(e) => e.key === 'Enter' && !b.current && void git.checkout(b.name)}
        oncontextmenu={(e) => branchMenu(e, b)}
      >
        <span class="mark">{#if b.current}<Icon name="check" size={12} stroke={2.2} />{/if}</span>
        <span class="name">{row.name}</span>
        {#if b.gone}<span class="muted small">{tr('上流なし', 'gone', '上游已删除')}</span>{/if}
        {#if b.ahead || b.behind}<span class="ab">{#if b.ahead}↑{b.ahead}{/if}{#if b.behind} ↓{b.behind}{/if}</span>{/if}
      </div>
    {/if}
  {/each}
{/if}

<!-- リモート -->
<div class="sec-head" role="button" tabindex="0" onclick={() => toggle('remotes')} onkeydown={(e) => e.key === 'Enter' && toggle('remotes')}>
  <Icon name={git.collapsed.has('remotes') ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
  <span class="title">{tr('リモート', 'Remotes', '远程')}</span>
  <span class="count">{git.refs?.remotes.length ?? 0}</span>
  <span class="sec-actions">
    <button class="icon-btn" title={tr('すべてフェッチ', 'Fetch all', '全部获取')} onclick={(e) => (e.stopPropagation(), void git.fetch())}><Icon name="refresh" size={13} /></button>
    <button class="icon-btn" title={tr('リモートの追加', 'Add remote', '添加远程')} onclick={(e) => (e.stopPropagation(), void git.addRemote())}><Icon name="plus" size={13} /></button>
  </span>
</div>
{#if !git.collapsed.has('remotes')}
  {#if !git.refs?.remotes.length}
    <div class="none">
      {tr('リモートがありません。', 'No remotes.', '没有远程仓库。')}
      <button class="link" onclick={() => void git.addRemote()}>{tr('追加する', 'Add one', '添加')}</button>
    </div>
  {/if}
  {#each git.refs?.remotes ?? [] as rm (rm.name)}
    {@const key = 'remote:' + rm.name}
    <div class="row remote" role="button" tabindex="0" onclick={() => toggle(key)} onkeydown={(e) => e.key === 'Enter' && toggle(key)} oncontextmenu={(e) => remoteMenu(e, rm.name, rm.url)} title={rm.url}>
      <Icon name={git.collapsed.has(key) ? 'chevronRight' : 'chevronDown'} size={11} stroke={2} />
      <span class="name strong">{rm.name}</span>
      <span class="url">{rm.url.replace(/^https?:\/\//, '').replace(/\.git$/, '')}</span>
      <span class="muted small when" title={tr('最後にフェッチした日時', 'Last fetched', '上次获取时间')}>{lastFetch()}</span>
      <button class="icon-btn" title={tr('フェッチ', 'Fetch', '获取')} onclick={(e) => (e.stopPropagation(), void git.fetch(rm.name))}><Icon name="refresh" size={12} /></button>
    </div>
    {#if !git.collapsed.has(key)}
      {#each remoteRows(rm.name) as row (row.kind === 'folder' ? row.key : 'rb:' + row.item.name)}
        {#if row.kind === 'folder'}
          <div class="row folder" style:padding-left="{32 + row.depth * 12}px" role="button" tabindex="0" onclick={() => toggle('open:' + row.key)} onkeydown={(e) => e.key === 'Enter' && toggle('open:' + row.key)}>
            <Icon name={git.collapsed.has('open:' + row.key) ? 'chevronDown' : 'chevronRight'} size={11} stroke={2} />
            {row.name}
          </div>
        {:else}
          {@const r = row.item}
          {@const local = localOf(r)}
          {@const full = `${r.remote}/${r.name}`}
          <div
            class="row"
            class:viewing={git.logRef === full}
            style:padding-left="{32 + row.depth * 12}px"
            role="button"
            tabindex="0"
            title={`${full}\n${r.subject}`}
            onclick={() => void git.showHistory(full)}
            ondblclick={() => void git.checkoutRemote(r.remote, r.name)}
            onkeydown={(e) => e.key === 'Enter' && void git.showHistory(full)}
            oncontextmenu={(e) => remoteBranchMenu(e, r)}
          >
            <span class="name">{row.name}</span>
            {#if local}
              {#if local.behind || local.ahead}
                <span class="ab" title={tr(`ローカル: ${local.name}`, `Local: ${local.name}`, `本地：${local.name}`)}
                  >{#if local.behind}↓{local.behind}{/if}{#if local.ahead} ↑{local.ahead}{/if}</span
                >
              {:else}
                <span class="muted small">{local.current ? tr('現在の上流', 'upstream', '当前上游') : ''}</span>
              {/if}
            {:else}
              <span class="muted small">{tr('（ローカルなし）', '(no local)', '（无本地）')}</span>
            {/if}
          </div>
        {/if}
      {/each}
    {/if}
  {/each}
{/if}

<!-- タグ -->
{#if git.refs?.tags.length}
  <div class="sec-head" role="button" tabindex="0" onclick={() => toggle('tags')} onkeydown={(e) => e.key === 'Enter' && toggle('tags')}>
    <Icon name={git.collapsed.has('tags') ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
    <span class="title">{tr('タグ', 'Tags', '标签')}</span>
    <span class="count">{git.refs.tags.length}</span>
  </div>
  {#if !git.collapsed.has('tags')}
    {#each tagRows as t (t.name)}
      <div class="row" style:padding-left="20px" role="button" tabindex="0" onclick={() => void git.showHistory(t.name)} onkeydown={(e) => e.key === 'Enter' && void git.showHistory(t.name)} oncontextmenu={(e) => tagMenu(e, t)}>
        <span class="name">{t.name}</span>
      </div>
    {/each}
  {/if}
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

  .sec-actions {
    display: none;
  }

  .sec-head:hover .sec-actions {
    display: flex;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding-right: 8px;
    cursor: pointer;
    white-space: nowrap;
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row.viewing {
    background: var(--bg-active);
  }

  .row.folder {
    color: var(--fg-muted);
  }

  .row.remote {
    padding-left: 18px;
  }

  .mark {
    width: 12px;
    flex: none;
    color: var(--accent);
    display: flex;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .row.current .name,
  .strong {
    font-weight: 600;
  }

  .url {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .when {
    flex: none;
  }

  .ab {
    margin-left: auto;
    font-size: 11px;
    color: var(--accent);
    flex: none;
  }

  .muted {
    color: var(--fg-muted);
  }

  .small {
    font-size: 11px;
    margin-left: auto;
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
    flex: none;
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

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
  }
</style>
