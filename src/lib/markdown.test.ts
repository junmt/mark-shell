import { describe, expect, it } from 'vitest';
import { headings, renderMarkdown, resolveLocal } from './markdown';

const ctx = { root: 'C:\\Notes', notePath: 'notes/today.md', externalImages: 'click' as const };
const render = (md: string) => {
  const div = document.createElement('div');
  div.innerHTML = renderMarkdown(md, ctx);
  return div;
};

describe('renderMarkdown', () => {
  it('GFM: 表・タスク・取り消し線・脚注', () => {
    const d = render('| a | b |\n|---|---|\n| 1 | 2 |\n\n- [ ] todo\n- [x] done\n\n~~消す~~[^1]\n\n[^1]: 注');
    expect(d.querySelectorAll('td')).toHaveLength(2);
    const boxes = d.querySelectorAll<HTMLInputElement>('input.task-checkbox');
    expect(boxes).toHaveLength(2);
    expect(boxes[1].checked).toBe(true);
    expect(boxes[0].disabled).toBe(false);
    expect(d.querySelector('del')?.textContent).toBe('消す');
    expect(d.querySelector('.footnotes')).not.toBeNull();
  });

  it('タスクの li に元の行番号が付く', () => {
    const d = render('# 見出し\n\n- [ ] a\n- [ ] b');
    const lis = d.querySelectorAll<HTMLElement>('li');
    expect(lis[1].dataset.line).toBe('4');
  });

  it('数式は KaTeX で描画する', () => {
    const d = render('式 $E=mc^2$\n\n$$\n\\int_0^1 x\\,dx\n$$');
    expect(d.querySelectorAll('.katex').length).toBeGreaterThanOrEqual(2);
  });

  it('[[内部リンク]] と別名・見出し', () => {
    const d = render('[[設計メモ]] と [[todo#今日|今日のやること]]');
    const links = d.querySelectorAll<HTMLAnchorElement>('a.wiki-link');
    expect(links[0].dataset.wiki).toBe('設計メモ');
    expect(links[1].dataset.wiki).toBe('todo#今日');
    expect(links[1].textContent).toBe('今日のやること');
  });

  it('コード内の [[...]] はリンクにしない', () => {
    const d = render('`[[x]]`');
    expect(d.querySelector('a')).toBeNull();
  });

  it('GitHub 形式のアラート', () => {
    const d = render('> [!WARNING]\n> 注意してください');
    const alert = d.querySelector('.markdown-alert-warning');
    expect(alert).not.toBeNull();
    expect(alert?.querySelector('.markdown-alert-title')?.textContent).toBe('Warning');
    expect(alert?.textContent).toContain('注意してください');
  });

  it('Mermaid はコードのまま図用の要素にする', () => {
    const d = render('```mermaid\ngraph TD; A-->B\n```');
    const block = d.querySelector<HTMLElement>('.mermaid-block');
    expect(decodeURIComponent(block?.dataset.mermaid ?? '')).toBe('graph TD; A-->B');
  });

  it('画像: 相対パスは mdimg、外部はクリックで読み込み、ルートの外は表示しない', () => {
    const d = render('![](assets/a%20b.png) ![](https://example.com/x.png) ![](../../outside.png)');
    const imgs = d.querySelectorAll<HTMLImageElement>('img');
    expect(imgs[0].getAttribute('src')).toBe('http://mdimg.localhost/notes/assets/a%20b.png');
    expect(imgs[1].getAttribute('src')).toBeNull();
    expect(imgs[1].dataset.src).toBe('https://example.com/x.png');
    expect(imgs[2].getAttribute('src')).toBeNull();
    expect(imgs[2].classList.contains('blocked-image')).toBe(true);
  });

  it('リンク: 相対・外部・ページ内', () => {
    const d = render('[a](../other.md#h) [b](https://x.y) [c](#見出し)');
    const [a, b, c] = d.querySelectorAll<HTMLAnchorElement>('a');
    expect(a.dataset.href).toBe('other.md#h');
    expect(b.dataset.external).toBe('https://x.y');
    expect(c.dataset.anchor).toBe('見出し');
  });

  it('危険な HTML は取り除く', () => {
    const d = render('<script>alert(1)</script><img src="x.png" onerror="alert(1)"><details><summary>s</summary>ok</details><a href="javascript:alert(1)">x</a>');
    expect(d.querySelector('script')).toBeNull();
    expect(d.querySelector('img')?.getAttribute('onerror')).toBeNull();
    expect(d.querySelector('details')).not.toBeNull();
    expect(d.querySelector('a')?.getAttribute('href') ?? '').not.toContain('javascript');
  });

  it('フロントマターは表にする', () => {
    const d = render('---\ntitle: メモ\ntags: [a, b]\n---\n\n本文');
    expect(d.querySelector('details.frontmatter th')?.textContent).toBe('title');
    expect(d.querySelector('details.frontmatter td')?.textContent).toBe('メモ');
    expect(d.textContent).toContain('本文');
  });

  it('見出しに id を付ける', () => {
    const d = render('## 今日 の 予定\n## 今日 の 予定');
    const hs = d.querySelectorAll('h2');
    expect(hs[0].id).toBe('今日-の-予定');
    expect(hs[1].id).toBe('今日-の-予定-1');
  });
});

describe('headings / resolveLocal', () => {
  it('コードブロック内とフロントマターは見出しにしない', () => {
    const h = headings('---\ntitle: x\n---\n# A\n```\n# no\n```\n## B ##');
    expect(h).toEqual([
      { level: 1, text: 'A', line: 4 },
      { level: 2, text: 'B', line: 8 },
    ]);
  });

  it('resolveLocal', () => {
    expect(resolveLocal('../img/a.png', 'notes/x.md', 'C:\\Notes')).toBe('img/a.png');
    expect(resolveLocal('C:\\Notes\\a.png', 'x.md', 'C:\\Notes')).toBe('a.png');
    expect(resolveLocal('D:\\a.png', 'x.md', 'C:\\Notes')).toBeNull();
  });
});
