<script lang="ts">
  import { untrack } from 'svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { store } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import { EDITOR_FONT, TERMINAL_FONT, type Config } from '../lib/config';
  import { COLOR_KEYS, COLOR_LABELS, DEFAULT_FONTS, FONT_SUGGESTIONS, PRESETS, fontStack, presetFor, resolveTheme, type ColorKey } from '../lib/themes';

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
      store.toast(explorerMenu ? 'エクスプローラーの右クリックメニューに「mark-shell で開く」を追加しました' : '右クリックメニューから外しました');
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label="設定">
    <h2>設定</h2>
    <div class="body">
      <section>
        <h3>配色</h3>
        <label>配色
          <select bind:value={draft.theme} onchange={onThemeChange}>
            <option value="system">Windows の設定に合わせる（ライト / ダーク）</option>
            {#each Object.entries(PRESETS) as [id, p] (id)}
              <option value={id}>{p.label}</option>
            {/each}
            <option value="custom">カスタム</option>
          </select>
        </label>
        {#if draft.theme === 'custom' && sample}
          <label>元にする配色
            <select bind:value={draft.customColors.base}>
              {#each Object.entries(PRESETS) as [id, p] (id)}
                <option value={id}>{p.label}</option>
              {/each}
            </select>
          </label>
          <div class="colors">
            {#each COLOR_KEYS as k (k)}
              <div class="color">
                <label>
                  <input type="color" value={sample.colors[k]} oninput={(e) => setColor(k, e.currentTarget.value)} />
                  <span>{COLOR_LABELS[k]}</span>
                </label>
                {#if draft.customColors.colors[k]}
                  <button class="link" onclick={() => resetColor(k)}>元に戻す</button>
                {/if}
              </div>
            {/each}
          </div>
          <div class="row">
            <button class="btn" onclick={() => draft && (draft.customColors.colors = {})}>すべて元の配色に戻す</button>
          </div>
        {/if}
        {#if sample}
          <!-- 見本。保存すると画面全体に反映する -->
          <div class="swatch" style:background={sample.colors.bg} style:color={sample.colors.fg} style:border-color={sample.colors.border}>
            <div class="swatch-pane" style:background={sample.colors.bgPane} style:color={sample.colors.fgMuted} style:border-color={sample.colors.border}>
              📁 メモ<br />　memo.md
            </div>
            <div class="swatch-body">
              <div style:color={sample.colors.mdHeading} class="swatch-heading">見出し</div>
              <div>本文の文字と<span style:color={sample.colors.mdLink}>リンク</span>と<code style:color={sample.colors.mdCode}>コード</code></div>
              <span class="swatch-btn" style:background={sample.colors.accent} style:color={sample.vars['accent-fg']}>ボタン</span>
            </div>
            <div class="swatch-term" style:background={sample.colors.terminalBg} style:color={sample.colors.terminalFg}>$ claude</div>
          </div>
        {/if}
      </section>

      <section>
        <h3>表示</h3>
        <datalist id="fonts-proportional">
          {#each FONT_SUGGESTIONS.proportional as f (f)}<option value={f}></option>{/each}
        </datalist>
        <datalist id="fonts-mono">
          {#each FONT_SUGGESTIONS.mono as f (f)}<option value={f}></option>{/each}
        </datalist>
        <label>画面全体（メニュー・ツリー・タブ）のフォント
          <input list="fonts-proportional" bind:value={draft.uiFontFamily} placeholder="既定（Segoe UI, Yu Gothic UI）" />
          <span class="font-sample" style:font-family={fontStack(draft.uiFontFamily, DEFAULT_FONTS.ui)}>あいうえお 漢字 ABC abc 123</span>
        </label>
        <label>エディタの文字の大きさ（Ctrl+ホイールでも変えられます） <input type="number" min="8" max="40" bind:value={draft.editorFontSize} /></label>
        <label>エディタのフォント
          <input list="fonts-mono" bind:value={draft.editorFontFamily} placeholder="既定（Cascadia Mono, BIZ UDGothic）" />
          <span class="font-sample" style:font-family={fontStack(draft.editorFontFamily, EDITOR_FONT)}>あいうえお 漢字 ABC abc 123 {'{}'} =&gt;</span>
        </label>
        <label>プレビューの文字の大きさ <input type="number" min="8" max="40" bind:value={draft.previewFontSize} /></label>
        <label>プレビューのフォント
          <input list="fonts-proportional" bind:value={draft.previewFontFamily} placeholder="既定（Segoe UI, Yu Gothic UI, Meiryo）" />
          <span class="font-sample" style:font-family={fontStack(draft.previewFontFamily, DEFAULT_FONTS.preview)}>あいうえお 漢字 ABC abc 123</span>
        </label>
        <label class="check"><input type="checkbox" bind:checked={draft.lineNumbers} /> 行番号を表示する</label>
        <label class="check"><input type="checkbox" bind:checked={draft.lineWrapping} /> 長い行を折り返す</label>
        <label>外部の画像
          <select bind:value={draft.externalImages}>
            <option value="click">クリックしたときだけ読み込む</option>
            <option value="always">常に読み込む</option>
          </select>
        </label>
      </section>

      <section>
        <h3>編集</h3>
        <label>自動保存までの時間（ミリ秒。0 で自動保存しない） <input type="number" min="0" step="100" bind:value={draft.autosaveDelay} /></label>
        <label>貼り付けた画像の保存先（ファイルと同じ場所からのフォルダ名） <input bind:value={draft.imageFolder} /></label>
      </section>

      <section>
        <h3>ファイルツリー</h3>
        <label>Markdown として扱う拡張子（カンマ区切り。ほかのファイルもツリーに出し、テキストならプレビューなしで編集できる） <input bind:value={extText} /></label>
        <label class="check"><input type="checkbox" bind:checked={draft.followSymlinks} /> シンボリックリンク・ジャンクションをたどる</label>
        <label>並び順
          <select bind:value={draft.treeSort}>
            <option value="name">名前順</option>
            <option value="mtime">更新日時が新しい順</option>
          </select>
        </label>
      </section>

      <section>
        <h3>ターミナル</h3>
        <label>既定のシェル
          <select bind:value={draft.terminal.defaultShell}>
            <option value="">自動（PowerShell）</option>
            {#each store.shells as s (s.id)}
              <option value={s.id}>{s.name}</option>
            {/each}
          </select>
        </label>
        <label>開いたときの作業フォルダ
          <select bind:value={draft.terminal.cwd}>
            <option value="root">開いているフォルダのルート</option>
            <option value="noteDir">編集中のファイルがあるフォルダ</option>
          </select>
        </label>
        <label>文字の大きさ <input type="number" min="8" max="32" bind:value={draft.terminal.fontSize} /></label>
        <label>フォント
          <input list="fonts-mono" bind:value={draft.terminal.fontFamily} placeholder="既定（Cascadia Mono, BIZ UDGothic）" />
          <span class="font-sample" style:font-family={fontStack(draft.terminal.fontFamily, TERMINAL_FONT)}>あいうえお 漢字 ABC abc 123 $ ls -la</span>
        </label>
        <label class="check"><input type="checkbox" bind:checked={draft.terminal.copyOnSelect} /> 選択したらコピーする</label>
      </section>

      <section>
        <h3>AI ツールと Windows</h3>
        <p class="note">AI ツールの定義（コマンド名・起動方法・メニュー）は設定ファイルで追加・変更できます。</p>
        <div class="row">
          <button class="btn" onclick={openAiToolsFile}>AI ツールの定義ファイルを開く</button>
          <button class="btn" onclick={() => void store.refreshAiTools()}>AI ツールを探し直す</button>
        </div>
        <div class="row">
          <button class="btn" onclick={toggleExplorerMenu}>
            {explorerMenu ? 'エクスプローラーの右クリックメニューから外す' : 'エクスプローラーの右クリックメニューに追加'}
          </button>
        </div>
      </section>

      <section>
        <h3>アップデート</h3>
        <label class="check"><input type="checkbox" bind:checked={draft.checkUpdates} /> 起動時に新しいバージョンがあるか確認する</label>
        <div class="row">
          <button class="btn" onclick={() => void store.checkForUpdate(true)}>いま確認する</button>
        </div>
      </section>
    </div>
    <div class="buttons">
      <button class="btn" onclick={() => (store.settingsOpen = false)}>キャンセル</button>
      <button class="btn primary" onclick={save}>保存</button>
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
