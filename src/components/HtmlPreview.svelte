<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { store, type Doc, type MenuItem } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { isMac, tr } from '../lib/i18n.svelte';
  import { prepareHtmlPreview } from '../lib/htmlPreview';

  let {
    doc: docProp,
    reloadSeq = 0,
    dragging = false,
  }: { doc: Doc; reloadSeq?: number; dragging?: boolean } = $props();

  const doc = untrack(() => docProp);

  let iframe: HTMLIFrameElement | undefined = $state();
  let srcdoc = $state('');
  let hasScripts = $state(false);
  let scriptNoticeDismissed = $state(false);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let first = true;
  let savedScroll = { x: 0, y: 0 };

  async function render() {
    const my = ++seq;
    // 更新前のスクロール位置を覚えておく（分割編集時に先頭へ戻らないようにする）
    try {
      const win = iframe?.contentWindow;
      if (win) savedScroll = { x: win.scrollX, y: win.scrollY };
    } catch {
      // 読み込み前などは無視
    }
    const res = await prepareHtmlPreview(doc.text, {
      root: store.root ?? '',
      filePath: doc.path,
      externalImages: store.config.externalImages,
      readText: async (rel) => {
        try {
          return (await api.readFile(rel)).content;
        } catch {
          return null;
        }
      },
    });
    if (my !== seq) return;
    srcdoc = res.srcdoc;
    hasScripts = res.hasScripts;
  }

  $effect(() => {
    void doc.text;
    void store.config.externalImages;
    void store.cssVersion;
    void reloadSeq;
    if (timer) clearTimeout(timer);
    if (first) {
      first = false;
      void render();
    } else {
      timer = setTimeout(() => void render(), store.view?.composing ? 300 : 60);
    }
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
    seq++;
  });

  function scrollToAnchor(id: string) {
    const idoc = iframe?.contentDocument;
    if (!idoc || !id) return;
    const target = idoc.getElementById(id) ?? idoc.querySelector<HTMLElement>(`[name="${CSS.escape(id)}"]`);
    target?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  function onLoad() {
    const idoc = iframe?.contentDocument;
    const iwin = iframe?.contentWindow;
    if (!idoc || !iwin) return;

    if (savedScroll.x || savedScroll.y) {
      iwin.scrollTo(savedScroll.x, savedScroll.y);
    }
    if (store.pendingAnchor) {
      scrollToAnchor(store.pendingAnchor);
      if (doc.mode === 'preview') store.pendingAnchor = null;
    }

    // iframe 内をクリックしたら親のコンテキストメニューを閉じる
    idoc.addEventListener('pointerdown', () => {
      if (store.menu) store.menu = null;
    });

    idoc.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && store.menu) {
        store.menu = null;
        return;
      }
      const mod = (isMac ? e.metaKey : e.ctrlKey) && !e.altKey;
      const key = e.key.toLowerCase();
      if (mod && e.shiftKey && key === 'v') {
        e.preventDefault();
        store.cycleMode();
      } else if (mod && !e.shiftKey && key === 'w') {
        e.preventDefault();
        void store.closeTab(doc.path);
      }
    });

    idoc.addEventListener('click', (e) => {
      const t = e.target as HTMLElement | null;
      if (!t) return;

      const img = t.closest<HTMLImageElement>('img.mark-shell-external-image');
      if (img?.dataset.src) {
        img.src = img.dataset.src;
        img.classList.remove('mark-shell-external-image');
        return;
      }

      const a = t.closest<HTMLAnchorElement>('a');
      if (!a) return;
      e.preventDefault();
      const d = a.dataset;
      if (d.anchor !== undefined) {
        scrollToAnchor(d.anchor);
      } else if (d.href) {
        void store.followLink(d.href);
      } else if (d.external) {
        void store.openExternal(d.external);
      }
    });

    idoc.addEventListener('contextmenu', (e) => {
      if (!iframe) return;
      const rect = iframe.getBoundingClientRect();
      const synthetic = {
        preventDefault: () => e.preventDefault(),
        clientX: rect.left + e.clientX,
        clientY: rect.top + e.clientY,
      } as MouseEvent;

      const t = e.target as HTMLElement | null;
      const img = t?.closest<HTMLImageElement>('img');
      if (img && !img.classList.contains('mark-shell-blocked-image')) {
        openImageMenu(synthetic, img);
        return;
      }

      const sel = iwin.getSelection()?.toString() ?? '';
      const items: MenuItem[] = [];
      if (sel) {
        items.push(
          {
            label: tr('コピー', 'Copy', '复制'),
            shortcut: 'Ctrl+C',
            action: () => void navigator.clipboard.writeText(sel),
          },
          { label: '', separator: true },
        );
      }
      items.push(
        { label: tr('再読み込み', 'Reload', '重新加载'), action: () => void render() },
        {
          label: tr('既定のアプリで開く', 'Open in default app', '用默认应用打开'),
          action: () => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error')),
        },
        { label: '', separator: true },
        { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void store.copyPath(doc.path, false) },
        { label: tr('相対パスをコピー', 'Copy relative path', '复制相对路径'), action: () => void store.copyPath(doc.path, true) },
      );
      store.openMenu(synthetic, items);
    });
  }

  function openImageMenu(e: MouseEvent, img: HTMLImageElement) {
    const items: MenuItem[] = [];
    const localPath = img.dataset.localPath;
    const externalUrl = img.dataset.src || (/^https?:\/\//i.test(img.src) ? img.src : null);

    if (img.classList.contains('mark-shell-external-image') && img.dataset.src) {
      const url = img.dataset.src;
      items.push(
        {
          label: tr('画像を読み込む', 'Load image', '加载图片'),
          action: () => {
            img.src = url;
            img.classList.remove('mark-shell-external-image');
          },
        },
        { label: '', separator: true },
        {
          label: tr('画像 URL をコピー', 'Copy image URL', '复制图片地址'),
          action: () => void navigator.clipboard.writeText(url).then(() => store.toast(tr(`コピーしました: ${url}`, `Copied: ${url}`, `已复制：${url}`))),
        },
        { label: tr('ブラウザで開く', 'Open in browser', '在浏览器中打开'), action: () => void store.openExternal(url) },
      );
      store.openMenu(e, items);
      return;
    }

    items.push({
      label: tr('画像をコピー', 'Copy image', '复制图片'),
      action: () => void store.copyImage({ img }),
    });

    if (localPath) {
      items.push(
        {
          label: tr('新しいタブで開く', 'Open in new tab', '在新标签页中打开'),
          action: () => void store.openFile(localPath, { pin: true }),
        },
        { label: '', separator: true },
        { label: tr('パスをコピー', 'Copy path', '复制路径'), action: () => void store.copyPath(localPath, false) },
        { label: tr('相対パスをコピー', 'Copy relative path', '复制相对路径'), action: () => void store.copyPath(localPath, true) },
        {
          label: tr('既定のアプリで開く', 'Open in default app', '用默认应用打开'),
          action: () => void api.openWithDefaultApp(localPath).catch((err) => store.toast(errorMessage(err), 'error')),
        },
        {
          label: isMac ? tr('Finder で表示', 'Reveal in Finder', '在访达中显示') : tr('エクスプローラーで表示', 'Reveal in Explorer', '在资源管理器中显示'),
          action: () => void api.revealInExplorer(localPath).catch((err) => store.toast(errorMessage(err), 'error')),
        },
      );
    } else if (externalUrl) {
      items.push(
        { label: '', separator: true },
        {
          label: tr('画像 URL をコピー', 'Copy image URL', '复制图片地址'),
          action: () => void navigator.clipboard.writeText(externalUrl).then(() => store.toast(tr(`コピーしました: ${externalUrl}`, `Copied: ${externalUrl}`, `已复制：${externalUrl}`))),
        },
        { label: tr('ブラウザで開く', 'Open in browser', '在浏览器中打开'), action: () => void store.openExternal(externalUrl) },
      );
    }

    store.openMenu(e, items);
  }
</script>

<div class="html-preview">
  {#if hasScripts && !scriptNoticeDismissed}
    <div class="notice">
      <span>{tr('セキュリティのため、プレビューではスクリプトを実行していません。', 'Scripts are disabled in preview for security.', '出于安全原因，预览中不执行脚本。')}</span>
      <button class="btn" onclick={() => void api.openWithDefaultApp(doc.path).catch((err) => store.toast(errorMessage(err), 'error'))}>
        {tr('ブラウザで開く', 'Open in browser', '在浏览器中打开')}
      </button>
      <button class="close" title={tr('閉じる', 'Dismiss', '关闭')} onclick={() => (scriptNoticeDismissed = true)}>✕</button>
    </div>
  {/if}
  <iframe
    bind:this={iframe}
    class:no-pointer={dragging}
    title={doc.path}
    sandbox="allow-same-origin"
    {srcdoc}
    onload={onLoad}
  ></iframe>
</div>

<style>
  .html-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: #ffffff;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    font-size: 12px;
    background: var(--bg-alt);
    color: var(--fg-muted);
    border-bottom: 1px solid var(--border);
    flex: none;
  }

  .notice .btn {
    padding: 2px 8px;
    font-size: 11px;
    color: var(--fg);
  }

  .notice .close {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--fg-muted);
    cursor: pointer;
    padding: 2px 5px;
    border-radius: 3px;
  }

  .notice .close:hover {
    background: var(--bg-hover);
    color: var(--fg);
  }

  iframe {
    flex: 1;
    width: 100%;
    height: 100%;
    border: none;
    background: #ffffff;
  }

  iframe.no-pointer {
    pointer-events: none;
  }
</style>
