// アプリの設定（%APPDATA%\mark-shell\config.json）

import type { CustomColors } from './themes';

export type Config = {
  /** 配色: 'system'（Windows の設定でライト / ダーク）、プリセット名（themes.ts）、'custom' */
  theme: string;
  /** theme が 'custom' のときの配色（元にするプリセットと、変えた色） */
  customColors: CustomColors;
  /** 画面全体（メニュー・ツリー・タブ）のフォント。空なら既定 */
  uiFontFamily: string;
  editorFontSize: number;
  editorFontFamily: string;
  previewFontSize: number;
  /** プレビューのフォント。空なら既定 */
  previewFontFamily: string;
  lineNumbers: boolean;
  lineWrapping: boolean;
  autosaveDelay: number;
  /** Markdown として扱う（プレビューできる）拡張子。それ以外のファイルもツリーに出し、テキストなら編集できる */
  markdownExtensions: string[];
  followSymlinks: boolean;
  /** 起動時に GitHub のリリースを見て新しいバージョンを知らせる */
  checkUpdates: boolean;
  treeSort: 'name' | 'mtime';
  externalImages: 'click' | 'always';
  imageFolder: string;
  terminal: {
    defaultShell: string;
    cwd: 'root' | 'noteDir';
    fontSize: number;
    fontFamily: string;
    copyOnSelect: boolean;
  };
};

export const defaultConfig: Config = {
  theme: 'system',
  customColors: { base: 'light', colors: {} },
  uiFontFamily: '',
  editorFontSize: 15,
  editorFontFamily: '',
  previewFontSize: 16,
  previewFontFamily: '',
  lineNumbers: false,
  lineWrapping: true,
  autosaveDelay: 1000,
  markdownExtensions: ['md', 'markdown', 'mdx'],
  followSymlinks: false,
  checkUpdates: true,
  treeSort: 'name',
  externalImages: 'click',
  imageFolder: 'assets',
  terminal: {
    defaultShell: '',
    cwd: 'root',
    fontSize: 14,
    fontFamily: '',
    copyOnSelect: false,
  },
};

export function mergeConfig(raw: Record<string, unknown> | null | undefined): Config {
  const r = (raw ?? {}) as Partial<Config>;
  return {
    ...defaultConfig,
    ...r,
    terminal: { ...defaultConfig.terminal, ...((r.terminal ?? {}) as Partial<Config['terminal']>) },
    customColors: { base: r.customColors?.base ?? 'light', colors: { ...(r.customColors?.colors ?? {}) } },
  };
}

export const EDITOR_FONT =
  "'Cascadia Mono', 'BIZ UDGothic', 'MS Gothic', Consolas, monospace";
export const UI_FONT = "'Segoe UI', 'Yu Gothic UI', 'Meiryo UI', sans-serif";
export const TERMINAL_FONT = "'Cascadia Mono', 'BIZ UDGothic', 'MS Gothic', Consolas, monospace";
