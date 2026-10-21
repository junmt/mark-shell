// Rust 側のコマンドの呼び出し

import { invoke, Channel } from '@tauri-apps/api/core';

export type Entry = { path: string; dir: boolean; mtime: number; size: number };
export type Change =
  | ({ kind: 'create' } & Entry)
  | ({ kind: 'modify' } & Entry)
  | { kind: 'remove'; path: string };

export type ReadResult = {
  content: string;
  encoding: string;
  eol: 'lf' | 'crlf';
  readonly: boolean;
  mtime: number;
};

export type WslInfo = { distro: string; linuxRoot: string };
export type OpenResult = { root: string; name: string; gen: number; wsl: WslInfo | null; wslError: string | null };
export type RecoveryEntry = { path: string; content: string; time: number };

export type SearchOptions = {
  query: string;
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
  folder: string;
};
export type SearchHit = { path: string; line: number; text: string; ranges: [number, number][] };

export type ShellInfo = { id: string; name: string };
export type ToolInfo = {
  id: string;
  name: string;
  kind: 'cli' | 'ide';
  found: boolean;
  path: string | null;
  installUrl: string | null;
  actions: { id: string; label: string }[];
};

export type AiSelection = { fromLine: number; toLine: number; text: string };

export type PtyEvent = { type: 'data'; data: string } | { type: 'exit'; code: number | null };
export type SpawnRequest = {
  shellId?: string | null;
  toolId?: string | null;
  actionId?: string | null;
  cwd?: string | null;
  note?: string | null;
  selection?: string | null;
  cols: number;
  rows: number;
};

export const api = {
  getConfig: () => invoke<Record<string, unknown>>('get_config'),
  setConfig: (config: unknown) => invoke<void>('set_config', { config }),
  loadWorkspaceState: (root: string) => invoke<unknown>('load_workspace_state', { root }),
  saveWorkspaceState: (root: string, value: unknown) => invoke<void>('save_workspace_state', { root, value }),
  getRecent: () => invoke<string[]>('get_recent'),
  removeRecent: (path: string) => invoke<void>('remove_recent', { path }),
  writeRecovery: (path: string, content: string) => invoke<void>('write_recovery', { path, content }),
  deleteRecovery: (path: string) => invoke<void>('delete_recovery', { path }),
  listRecovery: () => invoke<RecoveryEntry[]>('list_recovery'),
  registerExplorerMenu: (enable: boolean) => invoke<void>('register_explorer_menu', { enable }),
  isExplorerMenuRegistered: () => invoke<boolean>('is_explorer_menu_registered'),

  getLaunchTarget: () => invoke<{ root: string; file: string | null } | null>('get_launch_target'),
  openRoot: (path: string) => invoke<OpenResult>('open_root', { path }),
  rescan: () => invoke<number>('rescan'),
  readFile: (path: string) => invoke<ReadResult>('read_file', { path }),
  writeFile: (path: string, content: string, encoding: string, eol: string) =>
    invoke<number>('write_file', { path, content, encoding, eol }),
  createFile: (path: string, content?: string) => invoke<string>('create_file', { path, content }),
  createDir: (path: string) => invoke<string>('create_dir', { path }),
  countLinkUpdates: (from: string, to: string) => invoke<number>('count_link_updates', { from, to }),
  renamePath: (from: string, to: string, rewriteLinks: boolean) =>
    invoke<{ path: string; links: number; files: string[] }>('rename_path', { from, to, rewriteLinks }),
  deletePath: (path: string) => invoke<void>('delete_path', { path }),
  duplicatePath: (path: string) => invoke<string>('duplicate_path', { path }),
  importPaths: (sources: string[], destDir: string) => invoke<string[]>('import_paths', { sources, destDir }),
  saveAsset: (note: string, data: string, ext: string, folder: string) =>
    invoke<string>('save_asset', { note, data, ext, folder }),
  copyAsset: (note: string, source: string, folder: string) => invoke<string>('copy_asset', { note, source, folder }),
  pathExists: (path: string) => invoke<boolean>('path_exists', { path }),
  revealInExplorer: (path: string) => invoke<void>('reveal_in_explorer', { path }),
  absolutePath: (path: string) => invoke<string>('absolute_path', { path }),
  openNewWindow: (path?: string) => invoke<void>('open_new_window', { path }),
  isInstalled: () => invoke<boolean>('is_installed'),
  setLocale: (locale: string) => invoke<void>('set_locale', { locale }),
  openWithDefaultApp: (path: string) => invoke<void>('open_with_default_app', { path }),

  searchStart: (options: SearchOptions) => invoke<number>('search_start', { options }),
  searchCancel: () => invoke<void>('search_cancel'),
  replaceInFiles: (options: SearchOptions, replacement: string, paths: string[]) =>
    invoke<{ files: string[]; replacements: number }>('replace_in_files', { options, replacement, paths }),

  ptySpawn: (req: SpawnRequest, onEvent: Channel<PtyEvent>) =>
    invoke<{ id: number; title: string }>('pty_spawn', { req, onEvent }),
  ptyWrite: (id: number, data: string) => invoke<void>('pty_write', { id, data }),
  ptyResize: (id: number, cols: number, rows: number) => invoke<void>('pty_resize', { id, cols, rows }),
  ptyKill: (id: number) => invoke<void>('pty_kill', { id }),
  listShells: () => invoke<ShellInfo[]>('list_shells'),
  listAiTools: () => invoke<ToolInfo[]>('list_ai_tools'),
  launchIdeTool: (toolId: string, actionId: string | null, note: string | null) =>
    invoke<void>('launch_ide_tool', { toolId, actionId, note }),
  aiToolsFilePath: () => invoke<string>('ai_tools_file_path'),
  setAiContext: (note: string | null, selection: AiSelection | null) => invoke<void>('set_ai_context', { note, selection }),
};

export { Channel };

/** エディタで開けないファイル（バイナリ・大きすぎる）のエラーに付く印（Rust 側の workspace.rs と同じ） */
export const NOT_TEXT_PREFIX = 'not-text:';

export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
