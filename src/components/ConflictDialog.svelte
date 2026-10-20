<script lang="ts">
  import { store } from '../lib/store.svelte';
  import DiffLines from './DiffLines.svelte';
  import { tr } from '../lib/i18n.svelte';
</script>

{#if store.conflict}
  {@const c = store.conflict}
  <div class="backdrop" role="presentation"></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={tr('外部での変更', 'External change', '外部更改')}>
    <h2>{tr(`${c.doc.path} が外部で変更されました`, `${c.doc.path} was changed outside the app`, `${c.doc.path} 已在外部被修改`)}</h2>
    <p>
      {tr(
        '編集中の内容（保存前）と、ファイルの新しい内容が食い違っています。どちらを残すか選んでください。',
        'Your unsaved edits differ from the new contents of the file. Choose which to keep.',
        '正在编辑的内容（未保存）与文件的新内容不一致。请选择保留哪一个。',
      )}
    </p>
    <DiffLines
      before={c.disk.content}
      after={c.doc.text}
      beforeLabel={tr('ファイルの内容（外部での変更）', 'File contents (external change)', '文件内容（外部更改）')}
      afterLabel={tr('編集中の内容', 'Your edits', '正在编辑的内容')}
    />
    <div class="buttons">
      <button
        class="btn"
        onclick={() => store.resolveConflict('both')}
        title={tr(
          '自分の変更は別名のファイルに保存し、エディタには外部の内容を読み込みます',
          'Save your edits to a separate file and load the external contents into the editor',
          '将你的更改另存为其他文件，并在编辑器中加载外部内容',
        )}>{tr('両方を残す', 'Keep both', '保留两者')}</button
      >
      <button class="btn" onclick={() => store.resolveConflict('theirs')}>{tr('外部の変更を取り込む', 'Accept external changes', '采用外部更改')}</button>
      <button class="btn primary" onclick={() => store.resolveConflict('mine')}>{tr('自分の変更を残す', 'Keep my changes', '保留我的更改')}</button>
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
    top: 8%;
    left: 50%;
    transform: translateX(-50%);
    width: min(820px, 92vw);
    background: var(--bg-alt);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 18px 20px 16px;
    z-index: 71;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    word-break: break-all;
  }

  p {
    margin: 0 0 12px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
</style>
