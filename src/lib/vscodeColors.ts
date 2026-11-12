// 開いているフォルダの .vscode/settings.json の workbench.colorCustomizations を読み、
// ウィンドウの枠・ツールバー・ステータスバーの色にする。VS Code の拡張機能 Peacock などがプロジェクトごとに書く設定

export const VSCODE_SETTINGS = '.vscode/settings.json';

/** プロジェクトの色。指定のないものは undefined（いつもの配色のまま） */
export type ProjectColors = {
  /** タイトルバー（ウィンドウの枠） */
  titleBg?: string;
  titleFg?: string;
  titleInactiveBg?: string;
  titleInactiveFg?: string;
  /** 上のツールバー（VS Code のアクティビティバー） */
  toolbarBg?: string;
  toolbarFg?: string;
  statusBg?: string;
  statusFg?: string;
  statusHoverBg?: string;
};

/** ProjectColors のキー → VS Code の色の名前（先に書いたものを優先） */
const KEYS: Record<keyof ProjectColors, string[]> = {
  titleBg: ['titleBar.activeBackground'],
  titleFg: ['titleBar.activeForeground'],
  titleInactiveBg: ['titleBar.inactiveBackground'],
  titleInactiveFg: ['titleBar.inactiveForeground'],
  toolbarBg: ['activityBar.background', 'titleBar.activeBackground'],
  toolbarFg: ['activityBar.foreground', 'titleBar.activeForeground'],
  statusBg: ['statusBar.background'],
  statusFg: ['statusBar.foreground'],
  statusHoverBg: ['statusBarItem.hoverBackground'],
};

/** JSONC（コメントと末尾のカンマを許す JSON。VS Code の設定ファイルの形式）を読む */
export function parseJsonc(text: string): unknown {
  let out = '';
  let i = 0;
  const n = text.length;
  while (i < n) {
    const ch = text[i];
    if (ch === '"') {
      // 文字列はそのまま写す（中の // や , は触らない）
      let j = i + 1;
      while (j < n && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j + 1;
    } else if (ch === '/' && text[i + 1] === '/') {
      while (i < n && text[i] !== '\n') i++;
    } else if (ch === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      i = end < 0 ? n : end + 2;
    } else if (ch === ',') {
      // 後ろが } か ] だけなら、末尾のカンマなので捨てる（コメントも飛ばして見る）
      let j = i + 1;
      for (;;) {
        while (j < n && /\s/.test(text[j])) j++;
        if (text[j] === '/' && text[j + 1] === '/') {
          while (j < n && text[j] !== '\n') j++;
        } else if (text[j] === '/' && text[j + 1] === '*') {
          const end = text.indexOf('*/', j + 2);
          j = end < 0 ? n : end + 2;
        } else break;
      }
      if (text[j] !== '}' && text[j] !== ']') out += ch;
      i++;
    } else {
      out += ch;
      i++;
    }
  }
  return JSON.parse(out.replace(/^﻿/, ''));
}

/** #rgb / #rgba / #rrggbb / #rrggbbaa を #rrggbb か #rrggbbaa にする。色でなければ undefined */
export function normalizeColor(v: unknown): string | undefined {
  if (typeof v !== 'string') return undefined;
  const m = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(v.trim());
  if (!m) return undefined;
  let h = m[1].toLowerCase();
  if (h.length <= 4) h = [...h].map((c) => c + c).join('');
  return '#' + (h.endsWith('ff') && h.length === 8 ? h.slice(0, 6) : h);
}

/** settings.json の中身からプロジェクトの色を取り出す。色の指定がなければ null */
export function projectColors(settings: unknown): ProjectColors | null {
  const custom = (settings as Record<string, unknown> | null)?.['workbench.colorCustomizations'];
  if (!custom || typeof custom !== 'object') return null;
  const c = custom as Record<string, unknown>;
  const out: ProjectColors = {};
  for (const [key, names] of Object.entries(KEYS) as [keyof ProjectColors, string[]][]) {
    for (const name of names) {
      const v = normalizeColor(c[name]);
      if (v) {
        out[key] = v;
        break;
      }
    }
  }
  if (!Object.keys(out).length) return null;
  // 背景だけ指定されているときは、読める文字の色（白か黒）を補う
  out.titleFg ??= out.titleBg && readableOn(out.titleBg);
  out.toolbarFg ??= out.toolbarBg && readableOn(out.toolbarBg);
  out.statusFg ??= out.statusBg && readableOn(out.statusBg);
  for (const k of Object.keys(out) as (keyof ProjectColors)[]) if (!out[k]) delete out[k];
  return out;
}

/** 背景の上で読める文字の色 */
function readableOn(bg: string): string {
  const ch = (i: number) => {
    const v = parseInt(bg.slice(1 + i * 2, 3 + i * 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  const l = 0.2126 * ch(0) + 0.7152 * ch(1) + 0.0722 * ch(2);
  return l > 0.4 ? '#1f2328' : '#ffffff';
}

/** settings.json の文字列から。読めなければ null */
export function projectColorsFromText(text: string): ProjectColors | null {
  try {
    return projectColors(parseJsonc(text));
  } catch {
    return null;
  }
}

/** #rrggbb(aa) → タイトルバー用の #rrggbb。透明度があれば下地（#rrggbb）と混ぜる */
export function opaque(color: string | undefined, base: string): string | null {
  if (!color) return null;
  const rgb = color.slice(0, 7);
  if (color.length !== 9 || !/^#[0-9a-f]{6}$/i.test(base)) return rgb;
  const a = parseInt(color.slice(7, 9), 16) / 255;
  const ch = (hex: string, i: number) => parseInt(hex.slice(1 + i * 2, 3 + i * 2), 16);
  return (
    '#' +
    [0, 1, 2]
      .map((i) => Math.round(ch(rgb, i) * a + ch(base, i) * (1 - a)).toString(16).padStart(2, '0'))
      .join('')
  );
}
