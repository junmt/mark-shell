<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Terminal, type ITheme } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import { Unicode11Addon } from '@xterm/addon-unicode11';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { api, Channel, errorMessage, type PtyEvent } from '../lib/api';
  import { TERMINAL_FONT } from '../lib/config';
  import { store, type TermSession } from '../lib/store.svelte';

  let { session, active }: { session: TermSession; active: boolean } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let observer: ResizeObserver;

  const light: ITheme = {
    background: '#f3f4f6',
    foreground: '#1f2328',
    cursor: '#1f2328',
    selectionBackground: '#b6cdf5',
    black: '#24292f',
    red: '#cf222e',
    green: '#116329',
    yellow: '#7d4e00',
    blue: '#0969da',
    magenta: '#8250df',
    cyan: '#1b7c83',
    white: '#6e7781',
    brightBlack: '#57606a',
    brightRed: '#a40e26',
    brightGreen: '#1a7f37',
    brightYellow: '#633c01',
    brightBlue: '#218bff',
    brightMagenta: '#a475f9',
    brightCyan: '#3192aa',
    brightWhite: '#8c959f',
  };
  const dark: ITheme = {
    background: '#191a1d',
    foreground: '#e3e5e8',
    cursor: '#e3e5e8',
    selectionBackground: '#2f4a78',
    black: '#484f58',
    red: '#ff7b72',
    green: '#3fb950',
    yellow: '#d29922',
    blue: '#58a6ff',
    magenta: '#bc8cff',
    cyan: '#39c5cf',
    white: '#b1bac4',
    brightBlack: '#6e7681',
    brightRed: '#ffa198',
    brightGreen: '#56d364',
    brightYellow: '#e3b341',
    brightBlue: '#79c0ff',
    brightMagenta: '#d2a8ff',
    brightCyan: '#56d4dd',
    brightWhite: '#f0f6fc',
  };

  function resize() {
    if (!term || !host.offsetParent) return;
    try {
      fit.fit();
    } catch {
      return;
    }
    if (session.ptyId !== null && !session.exited) void api.ptyResize(session.ptyId, term.cols, term.rows).catch(() => {});
  }

  async function copySelection() {
    const text = term.getSelection();
    if (text) await navigator.clipboard.writeText(text);
  }

  async function paste() {
    const text = await navigator.clipboard.readText();
    if (text) term.paste(text);
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: store.config.terminal.fontFamily || TERMINAL_FONT,
      fontSize: store.config.terminal.fontSize,
      scrollback: 10000,
      allowProposedApi: true,
      cursorBlink: true,
      theme: store.dark ? dark : light,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon((_e, uri) => void openUrl(uri)));
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = '11';
    term.open(host);
    try {
      fit.fit();
    } catch {
      // 非表示のときは大きさが測れない
    }

    // Ctrl+C は選択中ならコピー、そうでなければシェルへ（中断）。Ctrl+V は貼り付け
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== 'keydown') return true;
      const key = e.key.toLowerCase();
      if (e.ctrlKey && key === 'c' && (e.shiftKey || term.hasSelection())) {
        void copySelection();
        if (!e.shiftKey) term.clearSelection();
        return false;
      }
      if (e.ctrlKey && key === 'v') {
        void paste();
        return false;
      }
      return true;
    });
    term.onSelectionChange(() => {
      if (store.config.terminal.copyOnSelect && term.hasSelection()) void copySelection();
    });

    const channel = new Channel<PtyEvent>();
    channel.onmessage = (ev) => {
      if (ev.type === 'data') term.write(ev.data);
      else {
        session.exited = true;
        term.write(`\r\n\x1b[90m[プロセスは終了しました${ev.code !== null ? `（コード ${ev.code}）` : ''}]\x1b[0m\r\n`);
      }
    };
    term.onData((data) => {
      if (session.ptyId !== null && !session.exited) void api.ptyWrite(session.ptyId, data).catch(() => {});
    });

    void api
      .ptySpawn({ ...session.request, cols: term.cols, rows: term.rows }, channel)
      .then((r) => {
        session.ptyId = r.id;
        resize();
      })
      .catch((e) => {
        session.exited = true;
        term.write(`\x1b[31m起動できませんでした: ${errorMessage(e)}\x1b[0m\r\n`);
      });

    observer = new ResizeObserver(() => resize());
    observer.observe(host);
  });

  onDestroy(() => {
    observer?.disconnect();
    term?.dispose();
  });

  $effect(() => {
    if (term) term.options.theme = store.dark ? dark : light;
  });

  $effect(() => {
    const t = store.config.terminal;
    if (!term) return;
    term.options.fontSize = t.fontSize;
    term.options.fontFamily = t.fontFamily || TERMINAL_FONT;
    resize();
  });

  $effect(() => {
    if (active && term) {
      requestAnimationFrame(() => {
        resize();
        term.focus();
      });
    }
  });

  function menu(e: MouseEvent) {
    store.openMenu(e, [
      { label: 'コピー', shortcut: 'Ctrl+C', disabled: !term.hasSelection(), action: () => void copySelection() },
      { label: '貼り付け', shortcut: 'Ctrl+V', action: () => void paste() },
      { label: 'すべて選択', action: () => term.selectAll() },
      { label: 'クリア', action: () => term.clear() },
      { label: '', separator: true },
      { label: 'このターミナルを閉じる', action: () => store.closeTerminal(session.key) },
    ]);
  }
</script>

<div class="term" bind:this={host} oncontextmenu={menu} role="presentation"></div>

<style>
  .term {
    position: absolute;
    inset: 4px 0 0 8px;
  }

  .term :global(.xterm) {
    height: 100%;
  }
</style>
