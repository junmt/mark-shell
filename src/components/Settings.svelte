<script lang="ts">
  import { untrack } from 'svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { store } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { EDITOR_FONT, TERMINAL_FONT, type Config } from '../lib/config';
  import { COLOR_KEYS, DEFAULT_FONTS, FONT_SUGGESTIONS, PRESETS, colorLabel, fontStack, presetFor, presetLabel, resolveTheme, type ColorKey } from '../lib/themes';
  import { LANGUAGE_LABELS, tr, type LanguageSetting } from '../lib/i18n.svelte';
  import Icon from './Icon.svelte';

  const languages = Object.entries(LANGUAGE_LABELS) as [LanguageSetting, string][];
  /** フォントの見本（CJK の字形が分かるように漢字を入れる） */
  const fontSample = $derived(tr('あいうえお 漢字 ABC abc 123', 'あいうえお 漢字 ABC abc 123', '中文 汉字 ABC abc 123'));

  let draft = $state<Config | null>(null);
  /** 編集中の配色（見本の表示と、色の入力欄の初期値に使う） */
  const sample = $derived(draft ? resolveTheme(draft.theme, draft.customColors, store.systemDark) : null);

  // カスタムに切り替えたら、それまでの配色を元にする
  function onThemeChange() {
    if (!draft || draft.theme !== 'custom' || Object.keys(draft.customColors.colors).length > 0) return;
    if (store.config.theme !== 'custom') draft.customColors.base = presetFor(store.config.theme, store.systemDark);
  }

  function setColor(k: ColorKey, v: string) {
    if (draft) draft.customColors.colors[k] = v;
  }

  function resetColor(k: ColorKey) {
    if (draft) delete draft.customColors.colors[k];
  }
  let extText = $state('');
  let explorerMenu = $state(false);

  // 開いたときだけ設定を写し取る（draft を読むと自分で書いた値に反応して繰り返してしまう）
  $effect(() => {
    if (store.settingsOpen) {
      const c = untrack(() => $state.snapshot(store.config) as Config);
      draft = c;
      extText = c.markdownExtensions.join(', ');
      void api.isExplorerMenuRegistered().then((v) => (explorerMenu = v));
    } else {
      draft = null;
    }
  });

  async function save() {
    if (!draft) return;
    draft.markdownExtensions = extText.split(/[,\s]+/).map((s) => s.trim().replace(/^\./, '')).filter(Boolean);
    if (draft.markdownExtensions.length === 0) draft.markdownExtensions = ['md'];
    const next = draft;
    store.settingsOpen = false;
    await store.updateConfig(next);
  }

  async function toggleExplorerMenu() {
    try {
      await api.registerExplorerMenu(!explorerMenu);
      explorerMenu = !explorerMenu;
      // 起動時の登録し直しもこの選択に従う
      if (draft) draft.explorerMenu = explorerMenu;
      await store.updateConfig({ explorerMenu });
      store.toast(
        explorerMenu
          ? tr(
              'エクスプローラーの右クリックメニュー（フォルダとファイル）に「mark-shell で開く」を追加しました',
              'Added "Open with mark-shell" to the Explorer context menu (folders and files)',
              '已将“用 mark-shell 打开”添加到资源管理器右键菜单（文件夹和文件）',
            )
          : tr('右クリックメニューから外しました', 'Removed from the context menu', '已从右键菜单中移除'),
      );
    } catch (e) {
      store.toast(errorMessage(e), 'error');
    }
  }

  async function openAiToolsFile() {
    const path = await api.aiToolsFilePath();
    try {
      await openPath(path);
    } catch {
      store.toast(path);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && store.settingsOpen && (store.settingsOpen = false)} />

{#if store.settingsOpen && draft}
  <div class="backdrop" role="presentation" onclick={() => (store.settingsOpen = false)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={tr('設定', 'Settings', '设置')}>
    <h2>{tr('設定', 'Settings', '设置')}</h2>
    <div class="body">
      <section>
        <h3>{tr('言語', 'Language', '语言')}</h3>
        <label>{tr('表示言語', 'Display language', '显示语言')}
          <select bind:value={draft.language}>
            {#each languages as [id, name] (id)}
              <option value={id}>{name}</option>
            {/each}
          </select>
        </label>
        <p class="note">
          {tr(
            '「System」は PC の言語に合わせます。対応していない言語のときは英語で表示します。',
            '"System" follows the PC language. Unsupported languages fall back to English.',
            '“System”会跟随电脑的语言。不支持的语言将显示为英语。',
          )}
        </p>
      </section>

      <section>
        <h3>{tr('配色', 'Colors', '配色')}</h3>
        <label>{tr('配色', 'Color theme', '配色方案')}
          <select bind:value={draft.theme} onchange={onThemeChange}>
            <option value="system">{tr('Windows の設定に合わせる（ライト / ダーク）', 'Match Windows (light / dark)', '跟随 Windows 设置（浅色 / 深色）')}</option>
            {#each Object.keys(PRESETS) as id (id)}
              <option value={id}>{presetLabel(id)}</option>
            {/each}
            <option value="custom">{tr('カスタム', 'Custom', '自定义')}</option>
          </select>
        </label>
        {#if draft.theme === 'custom' && sample}
          <label>{tr('元にする配色', 'Base theme', '基础配色')}
            <select bind:value={draft.customColors.base}>
              {#each Object.keys(PRESETS) as id (id)}
                <option value={id}>{presetLabel(id)}</option>
              {/each}
            </select>
          </label>
          <div class="colors">
            {#each COLOR_KEYS as k (k)}
              <div class="color">
                <label>
                  <input type="color" value={sample.colors[k]} oninput={(e) => setColor(k, e.currentTarget.value)} />
                  <span>{colorLabel(k)}</span>
                </label>
                {#if draft.customColors.colors[k]}
                  <button class="link" onclick={() => resetColor(k)}>{tr('元に戻す', 'Reset', '还原')}</button>
                {/if}
              </div>
            {/each}
          </div>
          <div class="row">
            <button class="btn" onclick={() => draft && (draft.customColors.colors = {})}>{tr('すべて元の配色に戻す', 'Reset all colors', '全部还原为基础配色')}</button>
          </div>
        {/if}
        {#if sample}
          <!-- 見本。保存すると画面全体に反映する -->
          <div class="swatch" style:background={sample.colors.bg} style:color={sample.colors.fg} style:border-color={sample.colors.border}>
            <div class="swatch-pane" style:background={sample.colors.bgPane} style:color={sample.colors.fgMuted} style:border-color={sample.colors.border}>
              <div class="swatch-row"><Icon name="folderOpen" size={14} />{tr('メモ', 'Notes', '笔记')}</div>
              <div class="swatch-row indent"><Icon name="fileText" size={14} />memo.md</div>
            </div>
            <div class="swatch-body">
              <div style:color={sample.colors.mdHeading} class="swatch-heading">{tr('見出し', 'Heading', '标题')}</div>
              <div>
                {tr('本文の文字と', 'Body text, ', '正文文字、')}<span style:color={sample.colors.mdLink}>{tr('リンク', 'a link', '链接')}</span>{tr('と', ' and ', '和')}<code style:color={sample.colors.mdCode}>{tr('コード', 'code', '代码')}</code>
              </div>
              <span class="swatch-btn" style:background={sample.colors.accent} style:color={sample.vars['accent-fg']}>{tr('ボタン', 'Button', '按钮')}</span>
            </div>
            <div class="swatch-term" style:background={sample.colors.terminalBg} style:color={sample.colors.terminalFg}>$ claude</div>
          </div>
        {/if}
      </section>

      <section>
        <h3>{tr('表示', 'Display', '显示')}</h3>
        <datalist id="fonts-proportional">
          {#each FONT_SUGGESTIONS.proportional as f (f)}<option value={f}></option>{/each}
        </datalist>
        <datalist id="fonts-mono">
          {#each FONT_SUGGESTIONS.mono as f (f)}<option value={f}></option>{/each}
        </datalist>
        <label>{tr('画面全体（メニュー・ツリー・タブ）のフォント', 'UI font (menus, tree, tabs)', '界面字体（菜单、文件树、标签页）')}
          <input list="fonts-proportional" bind:value={draft.uiFontFamily} placeholder={tr('既定（Segoe UI, Yu Gothic UI）', 'Default (Segoe UI, Yu Gothic UI)', '默认（Segoe UI, Yu Gothic UI）')} />
          <span class="font-sample" style:font-family={fontStack(draft.uiFontFamily, DEFAULT_FONTS.ui)}>{fontSample}</span>
        </label>
        <label>{tr('エディタの文字の大きさ（Ctrl+ホイールでも変えられます）', 'Editor font size (also Ctrl+wheel)', '编辑器字号（也可用 Ctrl+滚轮调整）')} <input type="number" min="8" max="40" bind:value={draft.editorFontSize} /></label>
        <label>{tr('エディタのフォント', 'Editor font', '编辑器字体')}
          <input list="fonts-mono" bind:value={draft.editorFontFamily} placeholder={tr('既定（Cascadia Mono, BIZ UDGothic）', 'Default (Cascadia Mono, BIZ UDGothic)', '默认（Cascadia Mono, BIZ UDGothic）')} />
          <span class="font-sample" style:font-family={fontStack(draft.editorFontFamily, EDITOR_FONT)}>{fontSample} {'{}'} =&gt;</span>
        </label>
        <label>{tr('プレビューの文字の大きさ', 'Preview font size', '预览字号')} <input type="number" min="8" max="40" bind:value={draft.previewFontSize} /></label>
        <label>{tr('プレビューのフォント', 'Preview font', '预览字体')}
          <input list="fonts-proportional" bind:value={draft.previewFontFamily} placeholder={tr('既定（Segoe UI, Yu Gothic UI, Meiryo）', 'Default (Segoe UI, Yu Gothic UI, Meiryo)', '默认（Segoe UI, Yu Gothic UI, Meiryo）')} />
          <span class="font-sample" style:font-family={fontStack(draft.previewFontFamily, DEFAULT_FONTS.preview)}>{fontSample}</span>
        </label>
        <label class="check"><input type="checkbox" bind:checked={draft.lineNumbers} /> {tr('行番号を表示する', 'Show line numbers', '显示行号')}</label>
        <label class="check"><input type="checkbox" bind:checked={draft.lineWrapping} /> {tr('長い行を折り返す', 'Wrap long lines', '自动换行')}</label>
        <label>{tr('外部の画像', 'External images', '外部图片')}
          <select bind:value={draft.externalImages}>
            <option value="click">{tr('クリックしたときだけ読み込む', 'Load only when clicked', '仅在点击时加载')}</option>
            <option value="always">{tr('常に読み込む', 'Always load', '始终加载')}</option>
          </select>
        </label>
      </section>

      <section>
        <h3>{tr('編集', 'Editing', '编辑')}</h3>
        <label>{tr('自動保存までの時間（ミリ秒。0 で自動保存しない）', 'Autosave delay (ms; 0 disables autosave)', '自动保存延迟（毫秒；0 表示不自动保存）')} <input type="number" min="0" step="100" bind:value={draft.autosaveDelay} /></label>
        <label>{tr('貼り付けた画像の保存先（ファイルと同じ場所からのフォルダ名）', 'Folder for pasted images (relative to the file)', '粘贴图片的保存位置（相对于文件所在位置的文件夹名）')} <input bind:value={draft.imageFolder} /></label>
      </section>

      <section>
        <h3>{tr('ファイルツリー', 'File tree', '文件树')}</h3>
        <label>{tr(
            'Markdown として扱う拡張子（カンマ区切り。ほかのファイルもツリーに出し、テキストならプレビューなしで編集できる）',
            'Markdown file extensions (comma-separated; other files still appear in the tree and text files can be edited without preview)',
            '视为 Markdown 的扩展名（逗号分隔；其他文件也会显示在文件树中，文本文件可在无预览的情况下编辑）',
          )} <input bind:value={extText} /></label>
        <label class="check"><input type="checkbox" bind:checked={draft.followSymlinks} /> {tr('シンボリックリンク・ジャンクションをたどる', 'Follow symbolic links and junctions', '跟随符号链接和目录联接')}</label>
        <label>{tr('並び順', 'Sort order', '排序方式')}
          <select bind:value={draft.treeSort}>
            <option value="name">{tr('名前順', 'Name', '按名称')}</option>
            <option value="mtime">{tr('更新日時が新しい順', 'Last modified (newest first)', '按修改时间（最新优先）')}</option>
          </select>
        </label>
      </section>

      <section>
        <h3>{tr('ターミナル', 'Terminal', '终端')}</h3>
        <label>{tr('既定のシェル', 'Default shell', '默认 Shell')}
          <select bind:value={draft.terminal.defaultShell}>
            <option value="">{tr('自動（PowerShell）', 'Auto (PowerShell)', '自动（PowerShell）')}</option>
            {#each store.shells as s (s.id)}
              <option value={s.id}>{s.name}</option>
            {/each}
          </select>
        </label>
        <label>{tr('開いたときの作業フォルダ', 'Starting directory', '启动时的工作目录')}
          <select bind:value={draft.terminal.cwd}>
            <option value="root">{tr('開いているフォルダのルート', 'Root of the open folder', '已打开文件夹的根目录')}</option>
            <option value="noteDir">{tr('編集中のファイルがあるフォルダ', 'Folder of the file being edited', '正在编辑的文件所在的文件夹')}</option>
          </select>
        </label>
        <label>{tr('文字の大きさ', 'Font size', '字号')} <input type="number" min="8" max="32" bind:value={draft.terminal.fontSize} /></label>
        <label>{tr('フォント', 'Font', '字体')}
          <input list="fonts-mono" bind:value={draft.terminal.fontFamily} placeholder={tr('既定（Cascadia Mono, BIZ UDGothic）', 'Default (Cascadia Mono, BIZ UDGothic)', '默认（Cascadia Mono, BIZ UDGothic）')} />
          <span class="font-sample" style:font-family={fontStack(draft.terminal.fontFamily, TERMINAL_FONT)}>{fontSample} $ ls -la</span>
        </label>
        <label class="check"><input type="checkbox" bind:checked={draft.terminal.copyOnSelect} /> {tr('選択したらコピーする', 'Copy on select', '选中即复制')}</label>
      </section>

      <section>
        <h3>{tr('AI ツールと Windows', 'AI tools and Windows', 'AI 工具与 Windows')}</h3>
        <p class="note">
          {tr(
            'AI ツールの定義（コマンド名・起動方法・メニュー）は設定ファイルで追加・変更できます。',
            'AI tool definitions (command names, launch methods, menus) can be added or changed in a config file.',
            'AI 工具的定义（命令名、启动方式、菜单）可以在配置文件中添加或修改。',
          )}
        </p>
        <div class="row">
          <button class="btn" onclick={openAiToolsFile}>{tr('AI ツールの定義ファイルを開く', 'Open AI tools config file', '打开 AI 工具定义文件')}</button>
          <button class="btn" onclick={() => void store.refreshAiTools()}>{tr('AI ツールを探し直す', 'Rescan AI tools', '重新查找 AI 工具')}</button>
        </div>
        <div class="row">
          <button class="btn" onclick={toggleExplorerMenu}>
            {explorerMenu
              ? tr('エクスプローラーの右クリックメニューから外す', 'Remove from Explorer context menu', '从资源管理器右键菜单中移除')
              : tr('エクスプローラーの右クリックメニューに追加', 'Add to Explorer context menu', '添加到资源管理器右键菜单')}
          </button>
        </div>
      </section>

      <section>
        <h3>{tr('アップデート', 'Updates', '更新')}</h3>
        <label class="check"><input type="checkbox" bind:checked={draft.checkUpdates} /> {tr('起動時に新しいバージョンがあるか確認する', 'Check for updates on startup', '启动时检查新版本')}</label>
        <div class="row">
          <button class="btn" onclick={() => void store.checkForUpdate(true)}>{tr('いま確認する', 'Check now', '立即检查')}</button>
        </div>
      </section>
    </div>
    <div class="buttons">
      <button class="btn" onclick={() => (store.settingsOpen = false)}>{tr('キャンセル', 'Cancel', '取消')}</button>
      <button class="btn primary" onclick={save}>{tr('保存', 'Save', '保存')}</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.3);
    z-index: 70;
  }

  .dialog {
    position: fixed;
    top: 5%;
    bottom: 5%;
    left: 50%;
    transform: translateX(-50%);
    width: min(640px, 92vw);
    display: flex;
    flex-direction: column;
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    z-index: 71;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
  }

  h2 {
    margin: 0;
    padding: 16px 20px 8px;
    font-size: 16px;
  }

  .body {
    flex: 1;
    overflow: auto;
    padding: 0 20px;
  }

  section {
    margin-bottom: 18px;
  }

  h3 {
    font-size: 13px;
    color: var(--fg-muted);
    margin: 12px 0 8px;
    border-bottom: 1px solid var(--border);
    padding-bottom: 4px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  label.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }

  input[type='number'] {
    width: 120px;
  }

  .note {
    color: var(--fg-muted);
    margin: 0 0 8px;
  }

  .font-sample {
    font-size: 14px;
    color: var(--fg-muted);
  }

  .colors {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px 16px;
    margin-bottom: 8px;
  }

  .color {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .color label {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    margin: 0;
  }

  .color input[type='color'] {
    width: 32px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border);
    background: none;
    cursor: pointer;
  }

  .link {
    border: none;
    background: none;
    padding: 0;
    color: var(--md-link);
    cursor: pointer;
    font-size: 12px;
  }

  .swatch {
    display: flex;
    border: 1px solid;
    border-radius: 6px;
    overflow: hidden;
    font-size: 13px;
    margin-top: 4px;
  }

  .swatch-pane {
    padding: 8px 10px;
    border-right: 1px solid;
    white-space: nowrap;
  }

  .swatch-row {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .swatch-row.indent {
    padding-left: 14px;
  }

  .swatch-body {
    flex: 1;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
  }

  .swatch-heading {
    font-weight: 700;
    font-size: 15px;
  }

  .swatch-btn {
    padding: 2px 10px;
    border-radius: 4px;
  }

  .swatch-term {
    padding: 8px 10px;
    font-family: var(--editor-mono);
    white-space: nowrap;
  }

  .row {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px 16px;
    border-top: 1px solid var(--border);
  }
</style>
