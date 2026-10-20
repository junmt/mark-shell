// 配色。プリセットと、プリセットを元に色を変えるカスタム。
// 基本の色（背景・文字・強調色など）から、ホバーや選択などの中間の色を計算して CSS 変数にする

/** カスタムで変えられる色 */
export const COLOR_KEYS = [
  'bg',
  'bgPane',
  'fg',
  'fgMuted',
  'border',
  'accent',
  'mdHeading',
  'mdLink',
  'mdCode',
  'terminalBg',
  'terminalFg',
] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];

export const COLOR_LABELS: Record<ColorKey, string> = {
  bg: '背景（エディタ・プレビュー）',
  bgPane: 'パネルの背景（ツリー・タブ・ツールバー）',
  fg: '文字',
  fgMuted: '薄い文字',
  border: '枠線',
  accent: '強調色（ボタン・選択）',
  mdHeading: '見出し',
  mdLink: 'リンク',
  mdCode: 'コード',
  terminalBg: 'ターミナルの背景',
  terminalFg: 'ターミナルの文字',
};

type Preset = {
  label: string;
  dark: boolean;
  colors: Record<ColorKey, string>;
  /** 計算で作る色を、決まった値にしたいとき（CSS 変数名から -- を除いたもの） */
  extra?: Record<string, string>;
};

const LIGHT_CODE = {
  'code-keyword': '#cf222e',
  'code-string': '#0a3069',
  'code-comment': '#6e7781',
  'code-number': '#0550ae',
  'code-function': '#8250df',
  'code-type': '#953800',
  'code-property': '#0550ae',
};

const DARK_CODE = {
  'code-keyword': '#ff7b72',
  'code-string': '#a5d6ff',
  'code-comment': '#8b949e',
  'code-number': '#79c0ff',
  'code-function': '#d2a8ff',
  'code-type': '#ffa657',
  'code-property': '#79c0ff',
};

export const PRESETS: Record<string, Preset> = {
  light: {
    label: 'ライト',
    dark: false,
    colors: {
      bg: '#ffffff',
      bgPane: '#f3f4f6',
      fg: '#1f2328',
      fgMuted: '#6b7280',
      border: '#d9dde3',
      accent: '#2f6fdd',
      mdHeading: '#1f2328',
      mdLink: '#0969da',
      mdCode: '#9a3412',
      terminalBg: '#f3f4f6',
      terminalFg: '#1f2328',
    },
    extra: {
      'bg-alt': '#f6f7f9',
      'bg-hover': '#e8eaee',
      'bg-active': '#dbe4f3',
      'active-line': 'rgba(47, 111, 221, 0.05)',
      selection: 'rgba(47, 111, 221, 0.22)',
      'md-meta': '#8b95a1',
      'md-quote': '#57606a',
    },
  },
  dark: {
    label: 'ダーク',
    dark: true,
    colors: {
      bg: '#1e1f22',
      bgPane: '#191a1d',
      fg: '#e3e5e8',
      fgMuted: '#9097a1',
      border: '#34373d',
      accent: '#4c8dff',
      mdHeading: '#f0f2f5',
      mdLink: '#6aa5ff',
      mdCode: '#f0a96b',
      terminalBg: '#191a1d',
      terminalFg: '#e3e5e8',
    },
    extra: {
      'bg-alt': '#26282c',
      'bg-hover': '#2d3036',
      'bg-active': '#2a3a57',
      'active-line': 'rgba(255, 255, 255, 0.04)',
      selection: 'rgba(76, 141, 255, 0.3)',
      'md-meta': '#6f7782',
      'md-quote': '#a0a8b3',
    },
  },
  sepia: {
    label: 'セピア',
    dark: false,
    colors: {
      bg: '#f8f1e3',
      bgPane: '#efe6d2',
      fg: '#433422',
      fgMuted: '#8a7a63',
      border: '#ddd0b6',
      accent: '#b0632b',
      mdHeading: '#3b2a17',
      mdLink: '#9a4f15',
      mdCode: '#8f3b1b',
      terminalBg: '#efe6d2',
      terminalFg: '#433422',
    },
  },
  'solarized-light': {
    label: 'Solarized ライト',
    dark: false,
    colors: {
      bg: '#fdf6e3',
      bgPane: '#eee8d5',
      fg: '#586e75',
      fgMuted: '#93a1a1',
      border: '#e0d9c4',
      accent: '#268bd2',
      mdHeading: '#073642',
      mdLink: '#268bd2',
      mdCode: '#d33682',
      terminalBg: '#eee8d5',
      terminalFg: '#586e75',
    },
  },
  'solarized-dark': {
    label: 'Solarized ダーク',
    dark: true,
    colors: {
      bg: '#002b36',
      bgPane: '#073642',
      fg: '#93a1a1',
      fgMuted: '#657b83',
      border: '#0f4552',
      accent: '#268bd2',
      mdHeading: '#eee8d5',
      mdLink: '#2aa198',
      mdCode: '#cb4b16',
      terminalBg: '#002b36',
      terminalFg: '#93a1a1',
    },
  },
  nord: {
    label: 'Nord',
    dark: true,
    colors: {
      bg: '#2e3440',
      bgPane: '#272c36',
      fg: '#d8dee9',
      fgMuted: '#8b95a7',
      border: '#3b4252',
      accent: '#88c0d0',
      mdHeading: '#eceff4',
      mdLink: '#88c0d0',
      mdCode: '#d08770',
      terminalBg: '#272c36',
      terminalFg: '#d8dee9',
    },
  },
  'high-contrast': {
    label: 'ハイコントラスト',
    dark: true,
    colors: {
      bg: '#000000',
      bgPane: '#0a0a0a',
      fg: '#ffffff',
      fgMuted: '#c8c8c8',
      border: '#6f6f6f',
      accent: '#ffd400',
      mdHeading: '#ffffff',
      mdLink: '#4fc3ff',
      mdCode: '#ff9d4d',
      terminalBg: '#000000',
      terminalFg: '#ffffff',
    },
  },
};

export type CustomColors = {
  /** 元にするプリセット */
  base: string;
  /** 変えた色だけを持つ */
  colors: Partial<Record<ColorKey, string>>;
};

/** 'system' | プリセット名 | 'custom' */
export type ThemeSetting = string;

export type ResolvedTheme = {
  dark: boolean;
  colors: Record<ColorKey, string>;
  /** CSS 変数（-- を除いた名前 → 値） */
  vars: Record<string, string>;
  terminal: { background: string; foreground: string; cursor: string; selectionBackground: string };
};

function parseHex(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)) as [number, number, number];
}

function toHex([r, g, b]: [number, number, number]): string {
  return '#' + [r, g, b].map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0')).join('');
}

/** a に b を t の割合で混ぜる */
export function mix(a: string, b: string, t: number): string {
  const x = parseHex(a);
  const y = parseHex(b);
  if (!x || !y) return a;
  return toHex([0, 1, 2].map((i) => x[i] + (y[i] - x[i]) * t) as [number, number, number]);
}

function rgba(hex: string, alpha: number): string {
  const c = parseHex(hex);
  return c ? `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})` : hex;
}

/** 相対輝度（0〜1） */
export function luminance(hex: string): number {
  const c = parseHex(hex);
  if (!c) return 0;
  const [r, g, b] = c.map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function presetFor(theme: ThemeSetting, systemDark: boolean): string {
  if (theme === 'system') return systemDark ? 'dark' : 'light';
  return PRESETS[theme] ? theme : 'light';
}

/** 設定から実際の色を決める */
export function resolveTheme(theme: ThemeSetting, custom: CustomColors, systemDark: boolean): ResolvedTheme {
  const isCustom = theme === 'custom';
  const preset = PRESETS[isCustom ? presetFor(custom.base, systemDark) : presetFor(theme, systemDark)];
  const colors = { ...preset.colors };
  if (isCustom) {
    for (const k of COLOR_KEYS) {
      const v = custom.colors[k];
      if (v && parseHex(v)) colors[k] = v;
    }
  }
  // カスタムで背景を変えたときは、明るさで明暗を決める（コードの色やターミナルの色に使う）
  const dark = isCustom ? luminance(colors.bg) < 0.35 : preset.dark;
  const c = colors;
  const vars: Record<string, string> = {
    bg: c.bg,
    'bg-pane': c.bgPane,
    fg: c.fg,
    'fg-muted': c.fgMuted,
    border: c.border,
    accent: c.accent,
    'accent-fg': luminance(c.accent) > 0.45 ? '#000000' : '#ffffff',
    'md-heading': c.mdHeading,
    'md-link': c.mdLink,
    'md-code': c.mdCode,
    'bg-alt': mix(c.bg, c.fg, 0.04),
    'bg-hover': mix(c.bgPane, c.fg, 0.08),
    'bg-active': mix(c.bgPane, c.accent, 0.22),
    'active-line': rgba(c.accent, 0.06),
    selection: rgba(c.accent, dark ? 0.32 : 0.22),
    'search-match': dark ? 'rgba(255, 196, 0, 0.3)' : 'rgba(255, 196, 0, 0.4)',
    danger: dark ? '#ff6b6b' : '#cf222e',
    'md-meta': mix(c.fg, c.bg, 0.45),
    'md-quote': mix(c.fg, c.bg, 0.3),
    'terminal-bg': c.terminalBg,
    ...(dark ? DARK_CODE : LIGHT_CODE),
  };
  // 手で決めた中間の色（ライト・ダーク）は、色を変えていなければそのまま使う
  if (!isCustom || Object.keys(custom.colors).length === 0) Object.assign(vars, preset.extra ?? {});
  return {
    dark,
    colors,
    vars,
    terminal: {
      background: c.terminalBg,
      foreground: c.terminalFg,
      cursor: c.terminalFg,
      selectionBackground: mix(c.terminalBg, c.accent, 0.35),
    },
  };
}

/** フォント名を CSS の font-family にする。カンマ区切りならそのまま、1 つなら引用符で囲んで既定の候補を後ろに付ける */
export function fontStack(name: string, fallback: string): string {
  const n = name.trim();
  if (!n) return fallback;
  if (n.includes(',')) return n;
  return `"${n.replace(/"/g, '')}", ${fallback}`;
}

export const DEFAULT_FONTS = {
  ui: "'Segoe UI', 'Yu Gothic UI', 'Meiryo UI', sans-serif",
  preview: "'Segoe UI', 'Yu Gothic UI', 'Meiryo', 'BIZ UDPGothic', sans-serif",
};

/** フォント入力欄の候補（Windows に入っているもの・よく入れられるもの） */
export const FONT_SUGGESTIONS = {
  proportional: [
    'Yu Gothic UI',
    'Yu Gothic',
    'Meiryo',
    'Meiryo UI',
    'BIZ UDPGothic',
    'BIZ UDPMincho',
    'Yu Mincho',
    'UD Digi Kyokasho NP-R',
    'Noto Sans JP',
    'Noto Serif JP',
    'Segoe UI',
  ],
  mono: ['Cascadia Mono', 'Cascadia Code', 'BIZ UDGothic', 'MS Gothic', 'Consolas', 'HackGen', 'PlemolJP', 'UDEV Gothic'],
};
