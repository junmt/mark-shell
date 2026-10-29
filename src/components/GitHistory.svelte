<script lang="ts">
  import { git } from '../lib/git.svelte';
  import { layoutGraph, shortHash, type Commit, type ChangedFile } from '../lib/git';
  import { store, type MenuItem } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { basename, dirname } from '../lib/paths';
  import Icon from './Icon.svelte';

  const ROW = 24;
  const LANE = 11;
  const COLORS = ['#3b82f6', '#2da44e', '#d97706', '#a855f7', '#ef4444', '#0ea5e9', '#e11d48', '#65a30d'];

  const filter = $derived(git.logFilter.trim().toLowerCase());
  const commits = $derived(
    filter
      ? git.log.filter((c) => c.subject.toLowerCase().includes(filter) || c.author.toLowerCase().includes(filter) || c.hash.startsWith(filter))
      : git.log,
  );
  // 絞り込み中や、ファイルの履歴では線が途切れるのでグラフを出さない
  const graph = $derived(filter || git.historyPath ? null : layoutGraph(git.log));
  const graphWidth = $derived(graph ? Math.min(12, Math.max(1, ...graph.map((r) => r.width))) * LANE + 6 : 0);

  const refChoices = $derived.by(() => {
    const out: { value: string; label: string }[] = [
      { value: 'HEAD', label: tr('現在のブランチ', 'Current branch', '当前分支') },
      { value: 'all', label: tr('すべてのブランチ', 'All branches', '所有分支') },
    ];
    for (const b of git.refs?.branches ?? []) if (!b.current) out.push({ value: b.name, label: b.name });
    for (const r of git.refs?.remoteBranches ?? []) out.push({ value: `${r.remote}/${r.name}`, label: `${r.remote}/${r.name}` });
    if (!out.some((o) => o.value === git.logRef)) out.push({ value: git.logRef, label: git.logRef.length === 40 ? shortHash(git.logRef) : git.logRef });
    return out;
  });

  function x(lane: number) {
    return 6 + lane * LANE;
  }

  type Badge = { text: string; kind: 'head' | 'local' | 'remote' | 'tag' };
  function badges(c: Commit): Badge[] {
    const out: Badge[] = [];
    for (const r of c.refs) {
      if (r.startsWith('HEAD -> ')) out.push({ text: r.slice(8), kind: 'head' });
      else if (r === 'HEAD') out.push({ text: 'HEAD', kind: 'head' });
      else if (r.startsWith('tag: ')) out.push({ text: r.slice(5), kind: 'tag' });
      else if (git.refs?.remoteBranches.some((b) => `${b.remote}/${b.name}` === r) || r.endsWith('/HEAD')) {
        if (!r.endsWith('/HEAD')) out.push({ text: r, kind: 'remote' });
      } else out.push({ text: r, kind: 'local' });
    }
    return out;
  }

  function toggleSection() {
    if (git.collapsed.has('history')) git.collapsed.delete('history');
    else git.collapsed.add('history');
  }

  function commitMenu(e: MouseEvent, c: Commit) {
    if (!git.selectedCommits.includes(c.hash)) git.selectedCommits = [c.hash];
    const selected = git.selectedCommits;
    const multi = selected.length > 1;
    const items: MenuItem[] = [];
    if (multi) {
      items.push({
        label: tr(`選んだ ${selected.length} 件を現在のブランチにチェリーピック`, `Cherry-pick ${selected.length} selected commits`, `拣选所选的 ${selected.length} 个提交`),
        action: () => void git.cherryPick(selected),
      });
      if (selected.length === 2) {
        items.push({ label: tr('選んだ 2 つを比較', 'Compare the two selected', '比较所选的两个'), action: () => git.openCompare(selected[1], selected[0]) });
      }
      items.push({ label: '', separator: true });
    }
    items.push(
      { label: tr('ハッシュをコピー', 'Copy hash', '复制哈希'), action: () => void navigator.clipboard.writeText(c.hash) },
      { label: tr('メッセージをコピー', 'Copy message', '复制信息'), action: () => void navigator.clipboard.writeText(c.subject) },
      { label: '', separator: true },
      { label: tr('現在のブランチにマージ…', 'Merge into current branch…', '合并到当前分支…'), action: () => void git.merge(c.hash, shortHash(c.hash)) },
      { label: tr('現在のブランチにチェリーピック…', 'Cherry-pick into current branch…', '拣选到当前分支…'), action: () => void git.cherryPick([c.hash]) },
      { label: tr('このコミットを打ち消す（revert）…', 'Revert this commit…', '还原此提交…'), action: () => void git.revert(c) },
      { label: '', separator: true },
      { label: tr('ここからブランチを作る…', 'Create branch here…', '从此处创建分支…'), action: () => void git.createBranch(c.hash) },
      { label: tr('タグを付ける…', 'Create tag…', '创建标签…'), action: () => void git.createTag(c.hash) },
      { label: tr('このコミットをチェックアウト', 'Check out this commit', '检出此提交'), action: () => void git.checkoutDetached(c.hash) },
      { label: '', separator: true },
      { label: tr('現在のブランチと比較', 'Compare with current branch', '与当前分支比较'), action: () => git.openCompare('HEAD', c.hash) },
      { label: tr('この時点のファイルを見る', 'Browse files at this commit', '浏览此时的文件'), action: () => git.openTree(c.hash) },
    );
    if (git.historyPath) {
      const p = git.historyPath;
      items.push({ label: tr('この時点の内容を開く', 'Open file at this commit', '打开此时的文件内容'), action: () => git.openRevision(c.hash, p) });
    }
    store.openMenu(e, items);
  }

  function fileMenu(e: MouseEvent, c: Commit, f: ChangedFile) {
    store.openMenu(e, [
      { label: tr('変更を表示', 'Show changes', '显示更改'), action: () => git.openCommitFile(c, f, true) },
      { label: tr('この時点の内容を開く', 'Open file at this commit', '打开此时的文件内容'), disabled: f.status === 'D', action: () => git.openRevision(c.hash, f.path) },
      { label: tr('このファイルの履歴', 'File history', '此文件的历史'), action: () => void historyOf(f.path) },
      { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void navigator.clipboard.writeText(f.path) },
    ]);
  }

  async function historyOf(path: string) {
    git.historyPath = path;
    git.expandedCommit = null;
    await git.loadLog(true);
  }

  let sentinel: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (!sentinel) return;
    const io = new IntersectionObserver((es) => {
      if (es.some((e) => e.isIntersecting) && !git.logDone && git.log.length) void git.loadLog(false);
    });
    io.observe(sentinel);
    return () => io.disconnect();
  });

  function date(sec: number) {
    return new Date(sec * 1000).toLocaleString();
  }
</script>

<div class="sec-head" role="button" tabindex="0" onclick={toggleSection} onkeydown={(e) => e.key === 'Enter' && toggleSection()}>
  <Icon name={git.collapsed.has('history') ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
  <span class="title">{tr('履歴', 'History', '历史')}</span>
</div>
{#if !git.collapsed.has('history')}
  <div class="tools">
    <select
      value={git.logRef}
      onchange={(e) => void git.showHistory((e.currentTarget as HTMLSelectElement).value)}
      title={tr('表示するブランチ', 'Branch to show', '要显示的分支')}
    >
      {#each refChoices as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
    </select>
    <input
      type="search"
      placeholder={tr('メッセージ・作者・ハッシュで絞り込む', 'Filter by message, author, hash', '按信息、作者、哈希筛选')}
      bind:value={git.logFilter}
      onchange={() => void git.loadLog(true)}
      spellcheck="false"
    />
  </div>
  {#if git.historyPath}
    <div class="filebar" title={git.historyPath}>
      <span>{tr('ファイルの履歴:', 'File history:', '文件历史：')} {git.historyPath}</span>
      <button class="icon-btn" onclick={() => void git.clearFileHistory()} title={tr('解除', 'Clear', '清除')}><Icon name="close" size={12} /></button>
    </div>
  {/if}
  {#if git.logRef !== 'HEAD' && git.logRef !== 'all' && git.notInHead.size}
    <div class="hint">{tr(`現在のブランチにないコミット: ${git.notInHead.size} 件（強調表示）`, `${git.notInHead.size} commit(s) not in the current branch (highlighted)`, `当前分支中没有的提交：${git.notInHead.size} 个（已高亮）`)}</div>
  {/if}
  <div class="log">
    {#if commits.length === 0}
      <div class="none">{tr('コミットがありません', 'No commits', '没有提交')}</div>
    {/if}
    {#each commits as c, i (c.hash)}
      {@const row = graph?.[i]}
      <div
        class="commit"
        class:selected={git.selectedCommits.includes(c.hash)}
        class:incoming={git.notInHead.has(c.hash)}
        role="button"
        tabindex="0"
        title={`${shortHash(c.hash)}  ${c.author} <${c.email}>\n${date(c.date)}\n\n${c.subject}`}
        onclick={(e) => git.selectCommit(c.hash, e)}
        onkeydown={(e) => e.key === 'Enter' && void git.toggleCommit(c.hash)}
        oncontextmenu={(e) => commitMenu(e, c)}
      >
        {#if row}
          <svg class="graph" width={graphWidth} height={ROW} aria-hidden="true">
            {#each row.top as [a, b2], k (k)}
              <path d={`M${x(a)} 0 C${x(a)} ${ROW / 4} ${x(b2)} ${ROW / 4} ${x(b2)} ${ROW / 2}`} stroke={COLORS[a % COLORS.length]} />
            {/each}
            {#each row.bottom as [a, b2], k (k)}
              <path d={`M${x(a)} ${ROW / 2} C${x(a)} ${(ROW * 3) / 4} ${x(b2)} ${(ROW * 3) / 4} ${x(b2)} ${ROW}`} stroke={COLORS[b2 % COLORS.length]} />
            {/each}
            <circle cx={x(row.col)} cy={ROW / 2} r={c.parents.length > 1 ? 3 : 3.5} fill={c.parents.length > 1 ? 'var(--bg)' : COLORS[row.col % COLORS.length]} stroke={COLORS[row.col % COLORS.length]} />
          </svg>
        {/if}
        <span class="subject">{c.subject}</span>
        {#each badges(c) as bd (bd.text)}<span class="badge {bd.kind}">{bd.text}</span>{/each}
        <span class="when">{git.relTime(c.date)}</span>
      </div>
      {#if git.expandedCommit === c.hash}
        <div class="detail">
          {#if !git.commitDetail}
            <div class="none">{tr('読み込み中…', 'Loading…', '正在加载…')}</div>
          {:else}
            {@const d = git.commitDetail}
            <div class="meta">
              <code>{shortHash(d.commit.hash)}</code>
              {d.commit.author} · {date(d.commit.date)}
              {#if d.commit.parents.length > 1}<span class="muted">{tr('（マージ。1 つめの親との差分）', '(merge; diff against first parent)', '（合并；与第一个父提交的差异）')}</span>{/if}
            </div>
            <pre class="message">{d.message}</pre>
            {#each d.files as f (f.path)}
              <div
                class="dfile"
                role="button"
                tabindex="0"
                title={f.orig ? `${f.orig} → ${f.path}` : f.path}
                onclick={(e) => git.openCommitFile(d.commit, f, e.detail >= 2)}
                onkeydown={(e) => e.key === 'Enter' && git.openCommitFile(d.commit, f, true)}
                oncontextmenu={(e) => fileMenu(e, d.commit, f)}
              >
                <span class="fname" class:deleted={f.status === 'D'}>{basename(f.path)}</span>
                <span class="fdir">{dirname(f.path)}</span>
                <span class="code c-{f.status}">{f.status}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    {/each}
    {#if !git.logDone && git.log.length}
      <div class="more" bind:this={sentinel}>
        <button class="link" onclick={() => void git.loadLog(false)}>{tr('さらに読む', 'Load more', '加载更多')}</button>
      </div>
    {/if}
  </div>
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

  .tools {
    display: flex;
    gap: 4px;
    padding: 2px 8px 4px;
  }

  .tools select {
    max-width: 45%;
  }

  .tools input {
    flex: 1;
    min-width: 0;
    padding: 2px 6px;
  }

  .filebar {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0 8px 4px;
    padding: 2px 4px 2px 8px;
    font-size: 12px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 12%, var(--bg));
  }

  .filebar span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    padding: 0 10px 4px;
    font-size: 11px;
    color: var(--fg-muted);
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

  .commit {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px 0 4px;
    cursor: pointer;
    white-space: nowrap;
  }

  .commit:hover {
    background: var(--bg-hover);
  }

  .commit.selected {
    background: var(--bg-active);
  }

  .commit.incoming .subject {
    color: var(--accent);
    font-weight: 600;
  }

  .graph {
    flex: none;
    overflow: visible;
  }

  .graph path {
    fill: none;
    stroke-width: 1.6;
  }

  .graph circle {
    stroke-width: 1.6;
  }

  .subject {
    flex: 1;
    min-width: 5em;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    flex: 0 1 auto;
    min-width: 2.5em;
    font-size: 10px;
    padding: 0 5px;
    border-radius: 8px;
    border: 1px solid var(--border);
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge.head {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }

  .badge.local {
    border-color: var(--accent);
    color: var(--accent);
  }

  .badge.remote {
    color: #2da44e;
    border-color: color-mix(in srgb, #2da44e 60%, transparent);
  }

  .badge.tag {
    color: #d97706;
    border-color: color-mix(in srgb, #d97706 60%, transparent);
  }

  .when {
    flex: none;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .detail {
    margin: 0 8px 6px 18px;
    padding: 6px 0 4px 8px;
    border-left: 2px solid var(--border);
  }

  .meta {
    font-size: 11px;
    color: var(--fg-muted);
    padding-right: 4px;
  }

  .meta code {
    font-family: var(--editor-mono);
  }

  .message {
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--ui-font);
    font-size: 12px;
    margin: 4px 0 6px;
    user-select: text;
  }

  .dfile {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 21px;
    padding-right: 4px;
    cursor: pointer;
    white-space: nowrap;
  }

  .dfile:hover {
    background: var(--bg-hover);
  }

  .fname {
    flex: none;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fname.deleted {
    text-decoration: line-through;
  }

  .fdir {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .code {
    font-size: 11px;
    font-weight: 700;
    width: 12px;
    text-align: center;
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
    padding: 2px 24px 4px;
    color: var(--fg-muted);
    font-size: 12px;
  }

  .muted {
    color: var(--fg-muted);
  }

  .more {
    padding: 4px 24px;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
  }
</style>
