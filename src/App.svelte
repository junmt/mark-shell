<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { store } from './lib/store.svelte';
  import { DEFAULT_FONTS, fontStack } from './lib/themes';
  import Toolbar from './components/Toolbar.svelte';
  import Welcome from './components/Welcome.svelte';
  import LeftPane from './components/LeftPane.svelte';
  import RightPane from './components/RightPane.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import Palette from './components/Palette.svelte';
  import Dialog from './components/Dialog.svelte';
  import ContextMenu from './components/ContextMenu.svelte';
  import Toasts from './components/Toasts.svelte';
  import ConflictDialog from './components/ConflictDialog.svelte';
  import DiffDialog from './components/DiffDialog.svelte';
  import Settings from './components/Settings.svelte';
  import Splitter from './components/Splitter.svelte';

  onMount(() => {
    void store.init();
    // Ctrl+ホイールで、カーソルの下にあるエディタ・プレビュー・ターミナルの文字の大きさを変える。
    // ページ全体の拡大を止めるため passive にしない
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const el = e.target instanceof Element ? e.target : null;
      const target = el?.closest('.term')
        ? 'terminal'
        : el?.closest('.markdown-body, .scroller')
          ? 'preview'
          : el?.closest('.editor')
            ? 'editor'
            : null;
      if (target && e.deltaY !== 0) store.adjustFontSize(target, e.deltaY < 0 ? 1 : -1);
    };
    window.addEventListener('wheel', onWheel, { passive: false, capture: true });
    return () => window.removeEventListener('wheel', onWheel, { capture: true });
  });

  // 配色とフォントを CSS 変数にする（app.css の値は、ここが動く前の既定）
  $effect(() => {
    const root = document.documentElement;
    root.dataset.theme = store.dark ? 'dark' : 'light';
    for (const [name, value] of Object.entries(store.colors.vars)) root.style.setProperty(`--${name}`, value);
    root.style.setProperty('--ui-font', fontStack(store.config.uiFontFamily, DEFAULT_FONTS.ui));
    root.style.setProperty('--preview-font', fontStack(store.config.previewFontFamily, DEFAULT_FONTS.preview));
  });

  // 開いているファイルと選択範囲を Claude Code に伝える（store.cursor は選択が変わるたびに置き換わる）
  $effect(() => {
    void store.root;
    void store.activePath;
    void store.cursor;
    store.scheduleAiContext();
  });

  // タイトルバーなどウィンドウ枠も配色の明暗に合わせる（system なら Windows の設定に従う）
  $effect(() => {
    const theme = store.config.theme === 'system' ? null : store.dark ? 'dark' : 'light';
    void getCurrentWindow()
      .setTheme(theme)
      .catch(() => {});
  });

  function inTerminal(e: KeyboardEvent): boolean {
    return e.target instanceof HTMLElement && !!e.target.closest('.xterm');
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey && !e.altKey;
    const key = e.key.toLowerCase();
    // ターミナルでは Ctrl+P などをシェルに渡す。ターミナル内でも効くのは Ctrl+` と Ctrl+Shift の組み合わせだけ。
    // 日本語キーボードでは ` が Shift+@ なので Ctrl+@ でも切り替える
    if (mod && !e.shiftKey && (e.code === 'Backquote' || key === '`' || key === '@')) {
      e.preventDefault();
      e.stopPropagation();
      store.toggleTerminal();
      return;
    }
    if (inTerminal(e) && (!(mod && e.shiftKey) || key === 'c' || key === 'v')) return;
    if (store.dialog || store.palette) return;

    const run = (fn: () => void) => {
      e.preventDefault();
      e.stopPropagation();
      fn();
    };
    if (mod && e.shiftKey) {
      if (key === 'p') return run(() => (store.palette = 'commands'));
      if (key === 'f') return run(() => store.showLeft('search'));
      if (key === 'e') return run(() => store.toggleLeft());
      if (key === 'v') return run(() => store.cycleMode());
      return;
    }
    if (mod) {
      if (key === 'p') return run(() => (store.palette = 'files'));
      if (key === 'o') return run(() => void store.chooseRoot());
      // WebView の再読み込み（編集中の内容が消えうる）の代わりに、最近のフォルダを選ぶ
      if (key === 'r') return run(() => void store.chooseRecentRoot());
      if (key === 'n') return run(() => void store.newFile());
      if (key === 's') return run(() => store.activeDoc && void store.save(store.activeDoc, true));
      if (key === 'w') return run(() => store.activeDoc && void store.closeTab(store.activeDoc.path));
      if (key === ',') return run(() => (store.settingsOpen = true));
      if (key === 'tab') return run(() => store.cycleTab(e.shiftKey ? -1 : 1));
    }
  }

  // ウィンドウの外をクリックしたら開いているメニューを閉じる
  function onPointerDown(e: PointerEvent) {
    if (store.menu && !(e.target instanceof HTMLElement && e.target.closest('.context-menu'))) store.menu = null;
  }
</script>

<svelte:window onkeydowncapture={onKeydown} onpointerdowncapture={onPointerDown} onblur={() => store.activeDoc && void store.save(store.activeDoc)} />

<div class="app">
  <Toolbar />
  <div class="main">
    {#if store.root}
      {#if store.leftVisible}
        <div class="left" style:width="{store.leftWidth}px">
          <LeftPane />
        </div>
        <Splitter
          direction="vertical"
          onresize={(d) => (store.leftWidth = Math.min(Math.max(160, store.leftWidth + d), window.innerWidth - 300))}
          ondone={() => store.saveWorkspaceState()}
        />
      {/if}
      <div class="right">
        <RightPane />
      </div>
    {:else}
      <Welcome />
    {/if}
  </div>
  <StatusBar />
</div>

{#if store.printHtml !== null}
  <div id="print-root" class="markdown-body">{@html store.printHtml}</div>
{/if}

<Palette />
<Dialog />
<ContextMenu />
<ConflictDialog />
<DiffDialog />
<Settings />
<Toasts />

{#if store.zoomHint}
  <div class="zoom-hint" role="status">{store.zoomHint}</div>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .main {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .left {
    flex: none;
    min-width: 160px;
    background: var(--bg-pane);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .zoom-hint {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    padding: 8px 18px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg-alt) 92%, transparent);
    border: 1px solid var(--border);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
    font-size: 14px;
    pointer-events: none;
    z-index: 100;
  }

  .right {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
</style>
