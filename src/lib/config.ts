// アプリの設定（%APPDATA%\mark-shell\config.json）

import type { CustomColors } from './themes';
import type { LanguageSetting } from './i18n.svelte';

export type Config = {
  /** 表示言語: 'system'（PC の言語。日本語・中国語以外は英語）、'ja'、'en'、'zh' */
  language: LanguageSetting;
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
  /** エクスプローラーの右クリックメニュー「mark-shell で開く」を登録する（起動のたびに登録し直す） */
  explorerMenu: boolean;
  treeSort: 'name' | 'mtime';
  externalImages: 'click' | 'always';
  imageFolder: string;
  /** Git 連携（docs/git.md） */
  gitEnabled: boolean;
  /** git の場所。空なら PATH から探す（Rust 側が読む） */
  gitPath: string;
  /** ウィンドウが前面にあるとき、gitAutoFetchMinutes 分ごとにフェッチする */
  gitAutoFetch: boolean;
  gitAutoFetchMinutes: number;
  /** ステージ済みがないときのコミット: 'ask'（すべてステージするか確認）、'always'（確認せずすべてステージ） */
  gitCommitAll: 'ask' | 'always';
  /** 差分の表示: 'split'（左右）、'inline'（行内） */
  gitDiffMode: 'split' | 'inline';
  /** 履歴を一度に読む件数。続きは「さらに表示」で読む */
  gitLogPage: number;
  /** エディタの行番号の横に変更の帯を出す */
  gitGutter: boolean;
  /** ファイルツリーで変更のあるファイルに色と記号を付ける */
  gitTreeColors: boolean;
  terminal: {
    defaultShell: string;
    cwd: 'root' | 'noteDir';
    fontSize: number;
    fontFamily: string;
    copyOnSelect: boolean;
  };
};

export const defaultConfig: Config = {
  language: 'system',
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
  explorerMenu: true,
  treeSort: 'name',
  externalImages: 'click',
  imageFolder: 'assets',
  gitEnabled: true,
  gitPath: '',
  gitAutoFetch: false,
  gitAutoFetchMinutes: 5,
  gitCommitAll: 'ask',
  gitDiffMode: 'split',
  gitLogPage: 50,
  gitGutter: true,
  gitTreeColors: true,
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
