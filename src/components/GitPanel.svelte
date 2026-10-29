<script lang="ts">
  import { git } from '../lib/git.svelte';
  import { pathTreeRows, sectionCode, type FileStatus, type PathTreeRow, type Section } from '../lib/git';
  import { store, type MenuItem } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';
  import { basename, dirname } from '../lib/paths';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Icon from './Icon.svelte';
  import GitHistory from './GitHistory.svelte';
  import GitRefs from './GitRefs.svelte';
  import GitStashes from './GitStashes.svelte';

  const b = $derived(git.branch);
  const st = $derived(git.repoState);
  const sections = $derived([
    { id: 'conflict' as Section, label: tr('競合', 'Conflicts', '冲突'), files: git.groups.conflict },
    { id: 'staged' as Section, label: tr('ステージ済み', 'Staged', '已暂存'), files: git.groups.staged },
    { id: 'unstaged' as Section, label: tr('変更', 'Changes', '更改'), files: git.groups.unstaged },
    { id: 'untracked' as Section, label: tr('未追跡', 'Untracked', '未跟踪'), files: git.groups.untracked },
  ]);

  const stateLabel = $derived.by(() => {
    if (!st?.kind) return '';
    const progress =
      git.sequenceTotal > 0 && st.remaining !== null
        ? `（${Math.max(1, git.sequenceTotal - st.remaining + 1)}/${git.sequenceTotal}）`
        : st.remaining
          ? tr(`（残り ${st.remaining}）`, ` (${st.remaining} left)`, `（剩余 ${st.remaining}）`)
          : '';
    switch (st.kind) {
      case 'merge':
        return tr('マージ中', 'Merging', '正在合并');
      case 'cherry-pick':
        return tr('チェリーピック中', 'Cherry-picking', '正在拣选') + progress;
      case 'revert':
        return tr('打ち消し（revert）中', 'Reverting', '正在还原') + progress;
      default:
        return tr('リベース中（ターミナルで続けてください）', 'Rebasing (continue in a terminal)', '正在变基（请在终端中继续）');
    }
  });

  function toggle(id: string) {
    if (git.collapsed.has(id)) git.collapsed.delete(id);
    else git.collapsed.add(id);
  }

  function code(f: FileStatus, s: Section) {
    return sectionCode(f, s);
  }

  function openFile(f: FileStatus) {
    const rel = git.rootPath(f.path);
    if (rel !== null) void store.openFile(rel, { pin: true });
  }

  function fileMenu(e: MouseEvent, f: FileStatus, s: Section) {
    const rel = git.rootPath(f.path);
    const items: MenuItem[] = [
      { label: tr('変更を表示', 'Show changes', '显示更改'), action: () => git.openChange(f, s, true) },
      { label: tr('ファイルを開く', 'Open file', '打开文件'), disabled: rel === null || (s !== 'conflict' && f.y === 'D'), action: () => openFile(f) },
      { label: '', separator: true },
    ];
    if (s === 'staged') items.push({ label: tr('ステージを外す', 'Unstage', '取消暂存'), action: () => void git.unstage([f]) });
    else if (s === 'conflict') items.push({ label: tr('解決済みにする', 'Mark as resolved', '标记为已解决'), action: () => void git.markResolved([f]) });
    else {
      items.push(
        { label: tr('ステージ', 'Stage', '暂存'), action: () => void git.stage([f]) },
        { label: tr('変更を破棄', 'Discard changes', '放弃更改'), action: () => void git.discard([f]) },
      );
    }
    items.push({ label: '', separator: true });
    if (rel !== null) {
      items.push(
        { label: tr('このファイルの履歴', 'File history', '此文件的历史'), disabled: f.untracked, action: () => void git.showFileHistory(rel) },
        { label: tr('.gitignore に追加', 'Add to .gitignore', '添加到 .gitignore'), action: () => void git.addToGitignore(rel) },
        {
          label: tr('ツリーで表示', 'Reveal in tree', '在树中显示'),
          action: () => {
            store.revealInTree(rel);
            store.selected = rel;
            store.showLeft('files');
          },
        },
      );
    }
    items.push({ label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void navigator.clipboard.writeText(f.path) });
    store.openMenu(e, items);
  }

  function moreMenu(e: MouseEvent) {
    const items: MenuItem[] = [
      { label: tr('フェッチ（すべてのリモート）', 'Fetch (all remotes)', '获取（所有远程）'), action: () => void git.fetch() },
      { label: tr('プル', 'Pull', '拉取'), action: () => void git.pull() },
      { label: tr('プッシュ', 'Push', '推送'), action: () => void git.push() },
      { label: tr('強制プッシュ（--force-with-lease）', 'Force push (--force-with-lease)', '强制推送（--force-with-lease）'), action: () => void git.push(true) },
      { label: '', separator: true },
      { label: tr('ブランチの切り替え…', 'Switch branch…', '切换分支…'), action: () => (store.palette = 'branches') },
      { label: tr('ブランチの作成…', 'Create branch…', '创建分支…'), action: () => void git.createBranch() },
      { label: tr('ブランチをマージ…', 'Merge branch…', '合并分支…'), action: () => (store.palette = 'merge') },
      { label: '', separator: true },
      { label: tr('スタッシュ…', 'Stash…', '贮藏…'), action: () => void git.stashPush() },
      { label: tr('すべてのブランチの履歴', 'History of all branches', '所有分支的历史'), action: () => void git.showHistory('all') },
      { label: tr('リモートの追加…', 'Add remote…', '添加远程…'), action: () => void git.addRemote() },
      { label: '', separator: true },
      { label: git.flat ? tr('ツリーで表示', 'Show as tree', '以树形显示') : tr('一覧で表示', 'Show as list', '以列表显示'), action: () => (git.flat = !git.flat) },
      { label: tr('読み直す', 'Refresh', '刷新'), action: () => void git.refresh() },
    ];
    store.openMenu(e, items);
  }

  function commitMenu(e: MouseEvent) {
    store.openMenu(e, [
      { label: tr('コミット', 'Commit', '提交'), shortcut: 'Ctrl+Enter', action: () => void git.commit() },
      { label: tr('修正コミット（直前のコミットを直す）', 'Amend (change the last commit)', '修改提交（修改上一次提交）'), action: () => void git.commit({ amend: true }) },
      { label: tr('コミットしてプッシュ', 'Commit and push', '提交并推送'), action: () => void git.commit({ push: true }) },
      { label: tr('サインオフ付きでコミット', 'Commit with sign-off', '带签名提交'), action: () => void git.commit({ signoff: true }) },
    ]);
  }

  function messageKey(e: KeyboardEvent) {
    const ta = e.currentTarget as HTMLTextAreaElement;
    if (e.key === 'Enter' && e.ctrlKey) {
      e.preventDefault();
      void git.commit();
    } else if ((e.key === 'ArrowUp' && ta.selectionStart === 0) || (e.key === 'ArrowDown' && ta.selectionEnd === ta.value.length)) {
      if (git.browseHistory(e.key === 'ArrowUp' ? 1 : -1)) e.preventDefault();
    }
  }

  const firstLineLong = $derived((git.message.split('\n')[0] ?? '').length > 72);

  /** 一覧（flat）か、フォルダのツリーで並べた行。閉じたフォルダは「区分:パス」で覚える */
  function rowsOf(id: Section, files: FileStatus[]): PathTreeRow<FileStatus>[] {
    if (git.flat) return files.map((item) => ({ kind: 'file', item, name: basename(item.path), depth: 0 }));
    return pathTreeRows(files, (d) => git.collapsed.has(`${id}:${d}`));
  }

  function filesIn(files: FileStatus[], dir: string): FileStatus[] {
    return files.filter((f) => f.path.startsWith(dir + '/'));
  }
</script>

<div class="git">
  {#if !git.enabled}
    <div class="empty">{tr('Git 連携は設定で無効になっています。', 'Git integration is turned off in Settings.', 'Git 集成已在设置中关闭。')}</div>
  {:else if !git.info}
    <div class="empty">{git.error ?? tr('読み込み中…', 'Loading…', '正在加载…')}</div>
  {:else if !git.info.found}
    <div class="empty">
      <p>{tr('git が見つかりません。', 'git was not found.', '找不到 git。')}</p>
      {#if git.info.error}<p class="muted small">{git.info.error}</p>{/if}
      {#if !store.wsl}
        <button class="link" onclick={() => void openUrl('https://git-scm.com/download/win')}>{tr('Git for Windows を入手する', 'Get Git for Windows', '获取 Git for Windows')}</button>
      {/if}
    </div>
  {:else if !git.repo}
    <div class="empty">
      <p>{tr('Git リポジトリではありません。', 'Not a Git repository.', '不是 Git 仓库。')}</p>
      <button class="btn" onclick={() => void git.initRepo()}>{tr('リポジトリを作成', 'Create repository', '创建仓库')}</button>
    </div>
  {:else}
    <div class="head">
      {#if (git.info.repos.length ?? 0) > 1}
        <select class="repo" value={git.repoPath} onchange={(e) => void git.selectRepo((e.currentTarget as HTMLSelectElement).value)} title={tr('リポジトリ', 'Repository', '仓库')}>
          {#each git.info.repos as r (r.path)}
            <option value={r.path}>{r.prefix ? r.prefix : r.name}</option>
          {/each}
        </select>
      {/if}
      <div class="branchbar">
        <button class="branch" onclick={() => (store.palette = 'branches')} title={tr('ブランチの切り替え', 'Switch branch', '切换分支')}>
          <Icon name="branch" size={14} />
          <span class="name">{b?.head ?? (b?.oid ? `(${b.oid.slice(0, 7)})` : tr('（コミットなし）', '(no commits)', '（无提交）'))}</span>
          {#if b?.upstream}
            <span class="ab" title={b.upstream}>{#if b.ahead}↑{b.ahead}{/if}{#if b.behind} ↓{b.behind}{/if}</span>
          {/if}
        </button>
        <span class="spacer"></span>
        <button class="icon-btn" onclick={() => void git.fetch()} title={tr('フェッチ', 'Fetch', '获取')} disabled={!!git.busy}><Icon name="refresh" size={14} /></button>
        <button class="icon-btn" onclick={() => void git.pull()} title={tr('プル', 'Pull', '拉取')} disabled={!!git.busy}><Icon name="arrowDown" size={14} /></button>
        <button class="icon-btn" onclick={() => void git.push()} title={tr('プッシュ', 'Push', '推送')} disabled={!!git.busy}><Icon name="arrowUp" size={14} /></button>
        <button class="icon-btn" onclick={moreMenu} title={tr('その他の操作', 'More actions', '更多操作')}><Icon name="more" size={14} stroke={2.6} /></button>
      </div>
      {#if git.repo.prefix === null && git.repo.rootInRepo !== null}
        <div class="note">{tr(`リポジトリ: ${git.repo.path}`, `Repository: ${git.repo.path}`, `仓库：${git.repo.path}`)}</div>
      {/if}
    </div>

    {#if git.busy}
      <div class="busy">
        <span class="spin"></span>
        <span class="label">{git.busy}{git.progress ? `: ${git.progress}` : '…'}</span>
        {#if git.opId !== null}<button class="link" onclick={() => git.cancel()}>{tr('取り消し', 'Cancel', '取消')}</button>{/if}
      </div>
    {/if}
    {#if git.error}<div class="banner error">{git.error}</div>{/if}
    {#if git.slow}
      <div class="banner">
        {tr('状態の取得に時間がかかるため、自動の更新を止めています。', 'Automatic refresh is paused because getting the status is slow.', '获取状态较慢，已暂停自动刷新。')}
        <button class="link" onclick={() => void git.refresh()}>{tr('更新', 'Refresh', '刷新')}</button>
      </div>
    {/if}
    {#if git.status?.status.truncated}
      <div class="banner">
        {tr(
          '未追跡のファイルが多すぎるため、一部だけ表示しています。.gitignore を見直してください。',
          'Too many untracked files; only some are shown. Consider updating .gitignore.',
          '未跟踪的文件过多，仅显示部分。请检查 .gitignore。',
        )}
      </div>
    {/if}
    {#if st?.kind}
      <div class="banner state">
        <strong>{stateLabel}</strong>
        {#if st.kind !== 'rebase'}
          <span class="actions">
            <button class="btn small" onclick={() => void git.sequencer('continue')}>{st.kind === 'merge' ? tr('マージを完了', 'Finish merge', '完成合并') : tr('続ける', 'Continue', '继续')}</button>
            {#if st.kind !== 'merge'}<button class="btn small" onclick={() => void git.sequencer('skip')}>{tr('スキップ', 'Skip', '跳过')}</button>{/if}
            <button class="btn small danger" onclick={() => void git.sequencer('abort')}>{tr('中止', 'Abort', '中止')}</button>
          </span>
        {/if}
      </div>
    {/if}

    <div class="scroll">
      <div class="commit">
        <textarea
          bind:value={git.message}
          oninput={() => git.onMessageInput()}
          onkeydown={messageKey}
          placeholder={tr('コミットメッセージ（Ctrl+Enter でコミット）', 'Commit message (Ctrl+Enter to commit)', '提交信息（Ctrl+Enter 提交）')}
          rows="3"
          spellcheck="false"
        ></textarea>
        {#if firstLineLong}<div class="warn small">{tr('1 行目が 72 文字を超えています', 'The first line is longer than 72 characters', '第一行超过 72 个字符')}</div>{/if}
        <div class="commit-buttons">
          <button class="btn primary commit-btn" onclick={() => void git.commit()} disabled={!!git.busy}>
            <Icon name="check" size={14} />
            {tr('コミット', 'Commit', '提交')}
          </button>
          <button class="btn primary drop" onclick={commitMenu} disabled={!!git.busy} title={tr('コミットの方法', 'Commit options', '提交方式')} aria-label={tr('コミットの方法', 'Commit options', '提交方式')}>▾</button>
        </div>
      </div>

      {#each sections as sec (sec.id)}
        {#if sec.files.length || sec.id === 'unstaged' || sec.id === 'staged'}
          {#if sec.id !== 'conflict' || sec.files.length}
            <div class="sec-head" role="button" tabindex="0" onclick={() => toggle(sec.id)} onkeydown={(e) => e.key === 'Enter' && toggle(sec.id)}>
              <Icon name={git.collapsed.has(sec.id) ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
              <span class="title">{sec.label}</span>
              <span class="count">{sec.files.length}</span>
              <span class="sec-actions">
                {#if sec.id === 'staged' && sec.files.length}
                  <button class="icon-btn" title={tr('すべてのステージを外す', 'Unstage all', '全部取消暂存')} onclick={(e) => (e.stopPropagation(), void git.unstageAll())}><Icon name="minus" size={13} /></button>
                {:else if (sec.id === 'unstaged' || sec.id === 'untracked') && sec.files.length}
                  <button class="icon-btn" title={tr('すべて破棄', 'Discard all', '全部放弃')} onclick={(e) => (e.stopPropagation(), void git.discard(sec.files))}><Icon name="undo" size={13} /></button>
                  <button class="icon-btn" title={tr('すべてステージ', 'Stage all', '全部暂存')} onclick={(e) => (e.stopPropagation(), void git.stage(sec.files))}><Icon name="plus" size={13} /></button>
                {:else if sec.id === 'conflict'}
                  <button class="icon-btn" title={tr('すべて解決済みにする', 'Mark all as resolved', '全部标记为已解决')} onclick={(e) => (e.stopPropagation(), void git.markResolved(sec.files))}><Icon name="check" size={13} /></button>
                {/if}
              </span>
            </div>
            {#if !git.collapsed.has(sec.id)}
              {#if sec.files.length === 0}
                <div class="none">{tr('なし', 'None', '无')}</div>
              {/if}
              {#each rowsOf(sec.id, sec.files) as r (r.kind === 'dir' ? `d:${r.path}` : r.item.path)}
                {#if r.kind === 'dir'}
                  {@const key = `${sec.id}:${r.path}`}
                  {@const inDir = filesIn(sec.files, r.path)}
                  <div
                    class="file dir"
                    role="button"
                    tabindex="0"
                    title={r.path}
                    onclick={() => toggle(key)}
                    onkeydown={(e) => e.key === 'Enter' && toggle(key)}
                    style:padding-left="{14 + r.depth * 12}px"
                  >
                    <Icon name={git.collapsed.has(key) ? 'chevronRight' : 'chevronDown'} size={12} stroke={2} />
                    <span class="fname">{r.name}</span>
                    <span class="acts">
                      {#if sec.id === 'staged'}
                        <button class="icon-btn" title={tr('このフォルダのステージを外す', 'Unstage this folder', '取消暂存此文件夹')} onclick={(e) => (e.stopPropagation(), void git.unstage(inDir))}><Icon name="minus" size={13} /></button>
                      {:else if sec.id === 'conflict'}
                        <button class="icon-btn" title={tr('このフォルダを解決済みにする', 'Mark this folder as resolved', '将此文件夹标记为已解决')} onclick={(e) => (e.stopPropagation(), void git.markResolved(inDir))}><Icon name="check" size={13} /></button>
                      {:else}
                        <button class="icon-btn" title={tr('このフォルダの変更を破棄', 'Discard changes in this folder', '放弃此文件夹中的更改')} onclick={(e) => (e.stopPropagation(), void git.discard(inDir))}><Icon name="undo" size={13} /></button>
                        <button class="icon-btn" title={tr('このフォルダをステージ', 'Stage this folder', '暂存此文件夹')} onclick={(e) => (e.stopPropagation(), void git.stage(inDir))}><Icon name="plus" size={13} /></button>
                      {/if}
                    </span>
                    <span class="dcount">{r.count}</span>
                  </div>
                {:else}
                  {@const f = r.item}
                  {@const rel = git.rootPath(f.path)}
                  {@const c = code(f, sec.id)}
                  <div
                    class="file"
                    class:outside={rel === null}
                    class:active={store.activeView?.diff?.path === f.path}
                    role="button"
                    tabindex="0"
                    title={f.orig ? `${f.orig} → ${f.path}` : f.path}
                    onclick={(e) => git.openChange(f, sec.id, e.detail >= 2)}
                    onkeydown={(e) => e.key === 'Enter' && git.openChange(f, sec.id, true)}
                    oncontextmenu={(e) => fileMenu(e, f, sec.id)}
                    style:padding-left="{(git.flat ? 20 : 30) + r.depth * 12}px"
                  >
                    <span class="fname" class:deleted={c === 'D'}>{r.name}</span>
                    {#if git.flat && dirname(f.path)}<span class="fdir">{dirname(f.path)}</span>{/if}
                    {#if rel === null}<span class="fdir">{tr('ルート外', 'outside root', '根目录外')}</span>{/if}
                    <span class="acts">
                      {#if rel !== null && c !== 'D'}
                        <button class="icon-btn" title={tr('ファイルを開く', 'Open file', '打开文件')} onclick={(e) => (e.stopPropagation(), openFile(f))}><Icon name="openFile" size={13} /></button>
                      {/if}
                      {#if sec.id === 'staged'}
                        <button class="icon-btn" title={tr('ステージを外す', 'Unstage', '取消暂存')} onclick={(e) => (e.stopPropagation(), void git.unstage([f]))}><Icon name="minus" size={13} /></button>
                      {:else if sec.id === 'conflict'}
                        <button class="icon-btn" title={tr('解決済みにする', 'Mark as resolved', '标记为已解决')} onclick={(e) => (e.stopPropagation(), void git.markResolved([f]))}><Icon name="check" size={13} /></button>
                      {:else}
                        <button class="icon-btn" title={tr('変更を破棄', 'Discard changes', '放弃更改')} onclick={(e) => (e.stopPropagation(), void git.discard([f]))}><Icon name="undo" size={13} /></button>
                        <button class="icon-btn" title={tr('ステージ', 'Stage', '暂存')} onclick={(e) => (e.stopPropagation(), void git.stage([f]))}><Icon name="plus" size={13} /></button>
                      {/if}
                    </span>
                    <span class="code c-{c === '!' ? 'x' : c}">{c}</span>
                  </div>
                {/if}
              {/each}
            {/if}
          {/if}
        {/if}
      {/each}

      <GitHistory />
      <GitRefs />
      <GitStashes />
    </div>
  {/if}
</div>

<style>
  .git {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .empty {
    padding: 16px;
    color: var(--fg-muted);
    line-height: 1.7;
  }

  .muted {
    color: var(--fg-muted);
  }

  .small {
    font-size: 11px;
  }

  .head {
    flex: none;
    padding: 6px 8px 4px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .repo {
    width: 100%;
  }

  .branchbar {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .branch {
    display: flex;
    align-items: center;
    gap: 5px;
    border: none;
    background: transparent;
    padding: 2px 4px;
    border-radius: 4px;
    cursor: pointer;
    min-width: 0;
    color: var(--fg);
  }

  .branch:hover {
    background: var(--bg-hover);
  }

  .branch .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .ab {
    font-size: 11px;
    color: var(--fg-muted);
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
  }

  .note {
    font-size: 11px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icon-btn {
    border: none;
    background: transparent;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--fg-muted);
    padding: 0;
    flex: none;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--fg);
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .busy {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    font-size: 11px;
    color: var(--fg-muted);
    flex: none;
  }

  .busy .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spin {
    width: 10px;
    height: 10px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex: none;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .banner {
    margin: 2px 8px 4px;
    padding: 5px 8px;
    border-radius: 5px;
    font-size: 12px;
    background: color-mix(in srgb, #d97706 14%, var(--bg));
    flex: none;
    word-break: break-all;
  }

  .banner.error {
    background: color-mix(in srgb, #cf222e 14%, var(--bg));
    white-space: pre-wrap;
    max-height: 120px;
    overflow: auto;
  }

  .banner.state {
    background: color-mix(in srgb, var(--accent) 14%, var(--bg));
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .actions {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .btn.small {
    padding: 1px 8px;
    font-size: 12px;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-bottom: 12px;
  }

  .commit {
    padding: 2px 8px 8px;
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    min-height: 54px;
    padding: 5px 7px;
    font-family: var(--ui-font);
  }

  .warn {
    color: #d97706;
  }

  .commit-buttons {
    display: flex;
    margin-top: 4px;
  }

  .commit-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }

  .drop {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left: 1px solid color-mix(in srgb, var(--accent-fg) 40%, transparent);
    padding: 0 8px;
  }

  .sec-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
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

  .sec-head .title {
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

  .none {
    padding: 2px 24px 4px;
    color: var(--fg-muted);
    font-size: 12px;
  }

  .file.dir {
    gap: 4px;
    color: var(--fg-muted);
  }

  .dcount {
    margin-left: auto;
    font-size: 11px;
    flex: none;
  }

  .file.dir:hover .dcount {
    margin-left: 0;
  }

  .file {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding-right: 8px;
    cursor: pointer;
    white-space: nowrap;
  }

  .file:hover {
    background: var(--bg-hover);
  }

  .file.active {
    background: var(--bg-active);
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

  .file.outside .fname {
    opacity: 0.75;
  }

  .acts {
    display: none;
    margin-left: auto;
  }

  .file:hover .acts {
    display: flex;
  }

  .code {
    width: 12px;
    text-align: center;
    font-size: 11px;
    font-weight: 700;
    margin-left: auto;
    flex: none;
  }

  .file:hover .code {
    margin-left: 0;
  }

  .c-M,
  .c-R,
  .c-C {
    color: #d29922;
  }

  .c-A,
  .c-U {
    color: #2da44e;
  }

  .c-D,
  .c-x {
    color: #cf222e;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
  }
</style>
