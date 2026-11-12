// OS ごとの違い（Mac では Ctrl の代わりに Cmd を使う）

export const IS_MAC = typeof navigator !== 'undefined' && /Macintosh|Mac OS X/.test(navigator.userAgent);

/** ショートカットの修飾キー（Windows は Ctrl、Mac は Cmd）を押しているか */
export function isMod(e: { ctrlKey: boolean; metaKey: boolean }, mac = IS_MAC): boolean {
  return mac ? e.metaKey : e.ctrlKey;
}

/**
 * ショートカットの表示を OS に合わせる。Mac では Ctrl+ を Cmd+ にする。
 * ただしターミナルの表示（Ctrl+`）と分割（Ctrl+Shift+5。Mac の Cmd+Shift+5 はスクリーンショット）、
 * タブの切り替え（Ctrl+Tab。Cmd+Tab はアプリの切り替え）は Mac でも Ctrl のまま
 */
export function keyLabel(text: string, mac = IS_MAC): string {
  return mac ? text.replace(/Ctrl\+(?!`|Shift\+5|Tab)/g, 'Cmd+') : text;
}
