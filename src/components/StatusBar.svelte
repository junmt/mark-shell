<script lang="ts">
  import { store } from '../lib/store.svelte';
  import { tr } from '../lib/i18n.svelte';

  const doc = $derived(store.activeDoc);
  const chars = $derived(doc ? [...doc.text.replace(/\s/g, '')].length : 0);
  const encodingLabel = (e: string) =>
    ({ 'utf-8': 'UTF-8', 'utf-8-bom': 'UTF-8 (BOM)', 'utf-16le': 'UTF-16 LE', 'utf-16be': 'UTF-16 BE' })[e] ?? e;
</script>

<footer class="status">
  {#if store.root}
    {#if store.wsl}
      <span
        class="wsl"
        title={tr(
          'WSL のフォルダ。ターミナルと AI ツールは WSL の中で動きます',
          'WSL folder. Terminals and AI tools run inside WSL',
          'WSL 文件夹。终端和 AI 工具在 WSL 中运行',
        )}>WSL: {store.wsl.distro}</span
      >
      <span title={store.root}>{store.wsl.linuxRoot}</span>
    {:else}
      <span title={store.root}>{store.root}</span>
    {/if}
    {#if store.scanning}<span>{tr('読み込み中…', 'Loading…', '加载中…')}</span>{/if}
  {/if}
  {#if store.aiRunning}<span class="ai">✦ {tr('AI セッション実行中', 'AI session running', 'AI 会话运行中')}</span>{/if}
  <span class="spacer"></span>
  {#if doc}
    <span
      >{doc.busy
        ? tr('保存中…', 'Saving…', '保存中…')
        : store.conflict?.doc === doc
          ? tr('競合（未保存）', 'Conflict (unsaved)', '冲突（未保存）')
          : doc.dirty
            ? tr('未保存', 'Unsaved', '未保存')
            : tr('保存済み', 'Saved', '已保存')}</span
    >
    <span>{tr(`行 ${store.cursor.line}、列 ${store.cursor.col}`, `Ln ${store.cursor.line}, Col ${store.cursor.col}`, `行 ${store.cursor.line}，列 ${store.cursor.col}`)}</span>
    <span title={tr('空白を除いた文字数', 'Characters excluding whitespace', '不含空白的字符数')}
      >{tr(`${chars.toLocaleString()} 文字`, `${chars.toLocaleString()} chars`, `${chars.toLocaleString()} 字`)}</span
    >
    <span>{encodingLabel(doc.encoding)}</span>
    <span>{doc.eol === 'crlf' ? 'CRLF' : 'LF'}</span>
    {#if doc.readonly}<span>{tr('読み取り専用', 'Read-only', '只读')}</span>{/if}
  {/if}
</footer>

<style>
  .status {
    height: 24px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 10px;
    font-size: 12px;
    color: var(--fg-muted);
    background: var(--bg-pane);
    border-top: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
  }

  .status > span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .spacer {
    flex: 1;
  }

  .ai,
  .wsl {
    color: var(--accent);
  }

  .wsl {
    font-weight: 600;
  }
</style>
