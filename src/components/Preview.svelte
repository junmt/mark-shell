<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { store, type Doc } from '../lib/store.svelte';
  import { renderMarkdown } from '../lib/markdown';
  import { enhance } from '../lib/enhance';
  import { toggleTaskLine } from '../lib/editor';
  import { dirname, join } from '../lib/paths';

  let {
    doc: docProp,
    onscrollline,
    scrollToLine = $bindable(),
  }: { doc: Doc; onscrollline: (line: number) => void; scrollToLine?: ((line: number) => void) | null } = $props();

  // 作ったときのファイルに固定する（Editor.svelte と同じ理由）
  const doc = untrack(() => docProp);

  let scroller: HTMLDivElement;
  let content: HTMLDivElement;
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let first = true;
  let programmatic = 0;

  function render() {
    const my = ++seq;
    const html = renderMarkdown(doc.text, {
      root: store.root ?? '',
      notePath: doc.path,
      externalImages: store.config.externalImages,
    });
    content.innerHTML = html;
    void enhance(content, store.dark, () => my === seq);
    if (store.pendingAnchor) scrollToAnchor(store.pendingAnchor);
  }

  // 本文・テーマ・設定が変わったら描き直す（入力中は少しまとめる）
  $effect(() => {
    void doc.text;
    void store.dark;
    void store.config.externalImages;
    if (!content) return;
    if (timer) clearTimeout(timer);
    if (first) {
      first = false;
      render();
    } else {
      timer = setTimeout(render, store.view?.composing ? 300 : 60);
    }
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
    seq++;
  });

  $effect(() => {
    void store.revealSeq;
    if (store.pendingAnchor && doc.mode === 'preview') scrollToAnchor(store.pendingAnchor);
    const r = store.pendingReveal;
    if (r && r.path === doc.path && doc.mode === 'preview') {
      store.pendingReveal = null;
      scrollToLine?.(r.line);
    }
  });

  function scrollToAnchor(anchor: string) {
    const id = anchor.toLowerCase();
    const el = content.querySelector<HTMLElement>(`[id="${CSS.escape(id)}"]`);
    if (el) {
      scroller.scrollTop = el.offsetTop - 12;
      if (doc.mode === 'preview') store.pendingAnchor = null;
    }
  }

  scrollToLine = (line: number) => {
    if (!content) return;
    const els = Array.from(content.querySelectorAll<HTMLElement>('[data-line]'));
    let target: HTMLElement | null = null;
    for (const el of els) {
      if (Number(el.dataset.line) <= line) target = el;
      else break;
    }
    programmatic = Date.now();
    scroller.scrollTop = target ? target.offsetTop - 8 : 0;
  };

  function onScroll() {
    if (Date.now() - programmatic < 150) return;
    const top = scroller.scrollTop;
    const els = content.querySelectorAll<HTMLElement>('[data-line]');
    for (const el of els) {
      if (el.offsetTop + el.offsetHeight > top) {
        onscrollline(Number(el.dataset.line));
        return;
      }
    }
  }

  async function onClick(e: MouseEvent) {
    const t = e.target as HTMLElement;

    const checkbox = t.closest<HTMLInputElement>('input.task-checkbox');
    if (checkbox) {
      e.preventDefault();
      const li = checkbox.closest<HTMLElement>('[data-line]');
      const n = Number(li?.dataset.line);
      if (!n || doc.readonly) return;
      const lines = doc.text.split('\n');
      const line = lines[n - 1];
      if (line === undefined || !/^\s*(?:[-*+]|\d+[.)])\s+\[( |x|X)\]/.test(line)) return;
      lines[n - 1] = toggleTaskLine(line);
      const text = lines.join('\n');
      store.setDocText(doc, text);
      if (!store.view) store.onDocChanged(doc, text);
      return;
    }

    const img = t.closest<HTMLImageElement>('img.external-image');
    if (img?.dataset.src) {
      img.src = img.dataset.src;
      img.classList.remove('external-image');
      return;
    }

    const a = t.closest<HTMLAnchorElement>('a');
    if (!a) return;
    e.preventDefault();
    const d = a.dataset;
    if (d.wiki) {
      const [name, hash] = d.wiki.split('#');
      const found = store.resolveWiki(name, doc.path);
      const target = found ?? join(dirname(doc.path), /\.[^./]+$/.test(name) ? name : `${name}.md`);
      await store.followLink(hash ? `${target}#${hash}` : target);
    } else if (d.anchor !== undefined) {
      scrollToAnchor(d.anchor);
    } else if (d.href) {
      await store.followLink(d.href);
    } else if (d.external) {
      await store.openExternal(d.external);
    }
  }
</script>

<div class="scroller" bind:this={scroller} onscroll={onScroll}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="markdown-body" style:font-size="{store.config.previewFontSize}px" bind:this={content} onclick={onClick}></div>
</div>

<style>
  .scroller {
    position: relative;
    flex: 1;
    overflow: auto;
    min-height: 0;
    background: var(--bg);
  }

  .markdown-body {
    max-width: 900px;
    margin: 0 auto;
    padding: 24px 32px 40vh;
  }
</style>
