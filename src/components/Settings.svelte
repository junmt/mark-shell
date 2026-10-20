<script lang="ts">
  import { untrack } from 'svelte';
  import { openPath } from '@tauri-apps/plugin-opener';
  import { store } from '../lib/store.svelte';
  import { api, errorMessage } from '../lib/api';
  import type { Config } from '../lib/config';

  let draft = $state<Config | null>(null);
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
        <h3>表示</h3>
        <label>テーマ
          <select bind:value={draft.theme}>
            <option value="system">Windows の設定に合わせる</option>
            <option value="light">ライト</option>
            <option value="dark">ダーク</option>
          </select>
        </label>
        <label>エディタの文字の大きさ（Ctrl+ホイールでも変えられます） <input type="number" min="8" max="40" bind:value={draft.editorFontSize} /></label>
        <label>エディタのフォント <input bind:value={draft.editorFontFamily} placeholder="既定（Cascadia Mono, BIZ UDGothic）" /></label>
        <label>プレビューの文字の大きさ <input type="number" min="8" max="40" bind:value={draft.previewFontSize} /></label>
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
        <label>フォント <input bind:value={draft.terminal.fontFamily} placeholder="既定（Cascadia Mono, BIZ UDGothic）" /></label>
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
