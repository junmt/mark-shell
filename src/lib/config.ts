// アプリの設定（%APPDATA%\mark-shell\config.json）

export type Config = {
  theme: 'system' | 'light' | 'dark';
  editorFontSize: number;
  editorFontFamily: string;
  previewFontSize: number;
  lineNumbers: boolean;
  lineWrapping: boolean;
  autosaveDelay: number;
  markdownExtensions: string[];
  /** Markdown 以外に一覧に出すテキストファイルの拡張子（プレビューなしで編集できる） */
  textExtensions: string[];
  excludePatterns: string[];
  respectGitignore: boolean;
  followSymlinks: boolean;
  showEmptyDirs: boolean;
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
  editorFontSize: 15,
  editorFontFamily: '',
  previewFontSize: 16,
  lineNumbers: false,
  lineWrapping: true,
  autosaveDelay: 1000,
  markdownExtensions: ['md', 'markdown', 'mdx'],
  // Rust 側の paths.rs と同じ
  textExtensions: ['txt', 'log', 'csv', 'tsv', 'json', 'jsonl', 'yaml', 'yml', 'toml', 'ini', 'cfg', 'conf', 'xml'],
  excludePatterns: [],
  respectGitignore: true,
  followSymlinks: false,
  showEmptyDirs: false,
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
  };
}

export const EDITOR_FONT =
  "'Cascadia Mono', 'BIZ UDGothic', 'MS Gothic', Consolas, monospace";
export const UI_FONT = "'Segoe UI', 'Yu Gothic UI', 'Meiryo UI', sans-serif";
export const TERMINAL_FONT = "'Cascadia Mono', 'BIZ UDGothic', 'MS Gothic', Consolas, monospace";
