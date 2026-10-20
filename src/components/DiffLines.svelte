<script lang="ts">
  import { diffLines } from 'diff';

  let { before, after, beforeLabel, afterLabel }: { before: string; after: string; beforeLabel: string; afterLabel: string } = $props();

  type Line = { kind: 'same' | 'add' | 'del' | 'gap'; text: string };

  // 変わっていない部分は前後 3 行だけ残して畳む
  const lines = $derived.by((): Line[] => {
    const out: Line[] = [];
    for (const part of diffLines(before, after)) {
      const kind = part.added ? 'add' : part.removed ? 'del' : 'same';
      const rows = part.value.replace(/\n$/, '').split('\n');
      if (kind === 'same' && rows.length > 7) {
        rows.slice(0, 3).forEach((t) => out.push({ kind, text: t }));
        out.push({ kind: 'gap', text: `… ${rows.length - 6} 行は同じ …` });
        rows.slice(-3).forEach((t) => out.push({ kind, text: t }));
      } else {
        rows.forEach((t) => out.push({ kind, text: t }));
      }
    }
    return out;
  });
</script>

<div class="legend"><span class="del">− {beforeLabel}</span><span class="add">＋ {afterLabel}</span></div>
<pre class="diff">{#each lines as l, i (i)}<div class={l.kind}><span class="mark">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ' '}</span>{l.text}</div>{/each}</pre>

<style>
  .legend {
    display: flex;
    gap: 16px;
    font-size: 12px;
    margin-bottom: 6px;
  }

  .diff {
    font-family: var(--editor-mono);
    font-size: 12px;
    line-height: 1.5;
    margin: 0;
    padding: 6px 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    overflow: auto;
    max-height: 55vh;
    user-select: text;
  }

  .diff div {
    padding: 0 10px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .mark {
    display: inline-block;
    width: 1.4em;
    color: var(--fg-muted);
  }

  .add {
    background: color-mix(in srgb, #2da44e 18%, transparent);
  }

  .del {
    background: color-mix(in srgb, #cf222e 16%, transparent);
  }

  .legend .add,
  .legend .del {
    padding: 0 6px;
    border-radius: 3px;
  }

  .gap {
    color: var(--fg-muted);
    font-style: italic;
  }
</style>
