<script lang="ts">
  import { tick } from 'svelte';
  import { store } from '../lib/store.svelte';
  import { api } from '../lib/api';
  import { tr } from '../lib/i18n.svelte';
  import { basename, dirname } from '../lib/paths';
  import { git } from '../lib/git.svelte';
  import { detectLanguage, languageNames, PLAIN_TEXT } from '../lib/editor';

  type Item = {
    key: string;
    label: string;
    detail?: string;
    shortcut?: string;
    run: () => void;
    /** 右クリックメニューでの run の名前 */
    primaryLabel?: string;
    /** 別の開き方（Shift+Enter・右クリック） */
    alt?: { label: string; run: () => void };
  };

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
    if (store.palette === 'folders') {
      const q = query.trim();
      return store.recent
        .filter((r) => r !== store.root)
        .map((r) => ({ r, name: r.split(/[\\/]/).filter(Boolean).pop() ?? r }))
        .map((x) => ({ ...x, s: Math.max(score(x.name, q) * 2, score(x.r, q)) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => (q ? b.s - a.s : 0))
        .map(({ r, name }) => ({
          key: r,
          label: name,
          detail: r,
          run: () => void store.openRoot(r),
          primaryLabel: tr('このウィンドウで開く', 'Open in this window', '在此窗口中打开'),
          alt: { label: tr('新しいウィンドウで開く', 'Open in new window', '在新窗口中打开'), run: () => void api.openNewWindow(r) },
        }));
    }
    // 色付けの言語を選ぶ（Markdown 以外のファイル）
    if (store.palette === 'language') {
      const doc = store.activeDoc;
      if (!doc) return [];
      const q = query.trim();
      const auto = detectLanguage(doc.path, doc.text)?.name ?? tr('プレーンテキスト', 'Plain Text', '纯文本');
      const current = doc.language;
      const list: Item[] = [
        { key: '__auto', label: tr('自動で選ぶ', 'Auto detect', '自动检测'), detail: auto + (current === null ? tr('（選択中）', ' (current)', '（当前）') : ''), run: () => store.setDocLanguage(doc, null) },
        {
          key: '__plain',
          label: tr('プレーンテキスト（色付けしない）', 'Plain Text (no highlighting)', '纯文本（不高亮）'),
          detail: current === PLAIN_TEXT ? tr('選択中', 'current', '当前') : '',
          run: () => store.setDocLanguage(doc, PLAIN_TEXT),
        },
        ...languageNames().map((n) => ({ key: n, label: n, detail: current === n ? tr('選択中', 'current', '当前') : '', run: () => store.setDocLanguage(doc, n) })),
      ];
      return q ? list.map((it) => ({ it, s: score(it.label, q) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).map((x) => x.it) : list;
    }
    // Git: ブランチの切り替え・マージするものを選ぶ
    if (store.palette === 'branches' || store.palette === 'merge') {
      const merge = store.palette === 'merge';
      const q = query.trim();
      const current = git.branch?.head;
      const list: Item[] = git
        .branchChoices()
        .filter((c) => !(c.remote === null && c.name === current))
        .map((c) => ({ c, s: score(c.name, q) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => (q ? b.s - a.s : 0))
        .map(({ c }) => ({
          key: c.name,
          label: c.name,
          detail: c.detail,
          run: () =>
            merge ? void git.merge(c.name) : c.remote ? void git.checkoutRemote(c.remote.remote, c.remote.name) : void git.checkout(c.name),
        }));
      if (merge) {
        for (const t of git.refs?.tags ?? []) {
          if (score(t.name, q) > 0) list.push({ key: 'tag:' + t.name, label: t.name, detail: tr('タグ', 'tag', '标签'), run: () => void git.merge(t.name) });
        }
      } else {
        list.unshift({ key: '+new', label: tr('＋ 新しいブランチを作成…', '+ Create new branch…', '＋ 新建分支…'), run: () => void git.createBranch() });
      }
      return list;
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

  function choose(item: Item | undefined, alt = false) {
    if (!item) return;
    close();
    (alt && item.alt ? item.alt.run : item.run)();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowDown') index = Math.min(items.length - 1, index + 1);
    else if (e.key === 'ArrowUp') index = Math.max(0, index - 1);
    else if (e.key === 'Enter') choose(items[index], e.shiftKey);
    else return;
    e.preventDefault();
  }

  // 別の開き方（新しいウィンドウで開くなど）がある項目は、右クリックで選べる
  function itemMenu(e: MouseEvent, item: Item) {
    if (!item.alt) return;
    store.openMenu(e, [
      { label: item.primaryLabel ?? tr('開く', 'Open', '打开'), shortcut: 'Enter', action: () => choose(item) },
      { label: item.alt.label, shortcut: 'Shift+Enter', action: () => choose(item, true) },
    ]);
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
  <div class="palette" role="dialog" aria-label={{
      files: tr('ファイルを開く', 'Open file', '打开文件'),
      commands: tr('コマンド', 'Commands', '命令'),
      folders: tr('最近開いたフォルダ', 'Recent folders', '最近打开的文件夹'),
      branches: tr('ブランチの切り替え', 'Switch branch', '切换分支'),
      merge: tr('マージするブランチ', 'Branch to merge', '要合并的分支'),
      language: tr('言語モード', 'Language mode', '语言模式'),
    }[store.palette]}>
    <input
      bind:this={input}
      bind:value={query}
      onkeydown={keydown}
      placeholder={{
        files: tr('ファイル名で検索', 'Search by file name', '按文件名搜索'),
        commands: tr('コマンドを検索', 'Search commands', '搜索命令'),
        folders: tr('最近開いたフォルダを名前で検索', 'Search recent folders by name', '按名称搜索最近打开的文件夹'),
        language: tr('色付けに使う言語を検索', 'Search for a language to highlight with', '搜索用于高亮的语言'),
        branches: tr('切り替えるブランチ（リモートのブランチも選べます）', 'Branch to switch to (remote branches too)', '要切换到的分支（也可选择远程分支）'),
        merge: tr(`${git.branch?.head ?? 'HEAD'} にマージするブランチ・タグ`, `Branch or tag to merge into ${git.branch?.head ?? 'HEAD'}`, `要合并到 ${git.branch?.head ?? 'HEAD'} 的分支或标签`),
      }[store.palette]}
      spellcheck="false"
    />
    <div class="list">
      {#each items as item, i (item.key)}
        <button
          class="item"
          class:on={i === index}
          onmouseenter={() => (index = i)}
          onclick={(e) => choose(item, e.shiftKey)}
          oncontextmenu={(e) => itemMenu(e, item)}
        >
          <span class="label">{item.label}</span>
          {#if item.detail}<span class="detail">{item.detail}</span>{/if}
          {#if item.shortcut}<span class="shortcut">{item.shortcut}</span>{/if}
        </button>
      {:else}
        <div class="none">{tr('見つかりません', 'No results', '未找到')}</div>
      {/each}
    </div>
    {#if store.palette === 'folders' && items.length > 0}
      <div class="hint">
        {tr(
          'Enter: このウィンドウで開く　Shift+Enter: 新しいウィンドウで開く（右クリックでも選べます）',
          'Enter: open in this window   Shift+Enter: open in new window (also via right-click)',
          'Enter: 在此窗口中打开　Shift+Enter: 在新窗口中打开（也可右键选择）',
        )}
      </div>
    {/if}
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

  .hint {
    padding: 6px 14px;
    font-size: 12px;
    color: var(--fg-muted);
    border-top: 1px solid var(--border);
  }
</style>
