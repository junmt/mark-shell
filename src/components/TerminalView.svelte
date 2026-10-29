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
  import { tr } from '../lib/i18n.svelte';

  let { session, active }: { session: TermSession; active: boolean } = $props();

  let host: HTMLDivElement;
  let term: Terminal;
  let fit: FitAddon;
  let observer: ResizeObserver;
  /** 履歴があってスクロールできるか（スクロールバーを常に出す） */
  let scrollable = $state(false);

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

  /** 16 色は明暗で選び、背景・文字・カーソル・選択の色は配色に合わせる */
  function terminalTheme(): ITheme {
    return { ...(store.dark ? dark : light), ...store.colors.terminal };
  }

  onMount(() => {
    term = new Terminal({
      fontFamily: store.config.terminal.fontFamily || TERMINAL_FONT,
      fontSize: store.config.terminal.fontSize,
      scrollback: 10000,
      allowProposedApi: true,
      cursorBlink: true,
      theme: terminalTheme(),
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

    // xterm.js 6 のスクロールバーはマウスを重ねたときだけ出る。履歴があるときは常に出す
    // （全画面のツールが使う代替バッファには履歴がないので出さない）
    const updateScrollable = () => (scrollable = term.buffer.active.baseY > 0);
    term.onWriteParsed(updateScrollable);
    term.onResize(updateScrollable);
    term.buffer.onBufferChange(updateScrollable);

    // AI ツールなどが付けるタイトル（OSC 0 / 2）をタブに出す
    term.onTitleChange((title) => session.setOsTitle(title));

    const channel = new Channel<PtyEvent>();
    channel.onmessage = (ev) => {
      if (ev.type === 'data') {
        term.write(ev.data);
        session.markOutput();
      } else {
        session.exited = true;
        const code = ev.code;
        const msg =
          code !== null
            ? tr(`プロセスは終了しました（コード ${code}）`, `Process exited (code ${code})`, `进程已退出（代码 ${code}）`)
            : tr('プロセスは終了しました', 'Process exited', '进程已退出');
        term.write(`\r\n\x1b[90m[${msg}]\x1b[0m\r\n`);
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
        // Git の操作を「ターミナルで実行」したときなど、起動したらすぐ入力するコマンド
        if (session.input) {
          const input = session.input;
          session.input = null;
          setTimeout(() => void api.ptyWrite(r.id, input + '\r').catch(() => {}), 600);
        }
      })
      .catch((e) => {
        session.exited = true;
        const err = errorMessage(e);
        term.write(`\x1b[31m${tr(`起動できませんでした: ${err}`, `Failed to start: ${err}`, `无法启动: ${err}`)}\x1b[0m\r\n`);
      });

    observer = new ResizeObserver(() => resize());
    observer.observe(host);
  });

  onDestroy(() => {
    observer?.disconnect();
    term?.dispose();
  });

  $effect(() => {
    const theme = terminalTheme();
    if (term) term.options.theme = theme;
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
      { label: tr('コピー', 'Copy', '复制'), shortcut: 'Ctrl+C', disabled: !term.hasSelection(), action: () => void copySelection() },
      { label: tr('貼り付け', 'Paste', '粘贴'), shortcut: 'Ctrl+V', action: () => void paste() },
      { label: tr('すべて選択', 'Select all', '全选'), action: () => term.selectAll() },
      { label: tr('クリア', 'Clear', '清除'), action: () => term.clear() },
      { label: '', separator: true },
      { label: tr('このターミナルを閉じる', 'Close this terminal', '关闭此终端'), action: () => store.closeTerminal(session.key) },
    ]);
  }
</script>

<div class="term" class:scrollable bind:this={host} oncontextmenu={menu} role="presentation"></div>

<style>
  .term {
    position: absolute;
    inset: 4px 0 0 8px;
  }

  .term :global(.xterm) {
    height: 100%;
  }

  /* 履歴があるときはスクロールバーを常に出す（xterm.js は普段 invisible にしている） */
  .term.scrollable :global(.xterm-scrollable-element > .scrollbar.vertical.invisible) {
    opacity: 1;
    pointer-events: auto;
  }
</style>
