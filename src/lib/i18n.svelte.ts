// 表示言語（日本語・英語・中国語）。設定が 'system' なら PC の言語に合わせ、対応していない言語は英語にする。
// 文言は呼び出し箇所に 3 言語を並べて書く: tr('保存', 'Save', '保存')

export type Locale = 'ja' | 'en' | 'zh';
export type LanguageSetting = 'system' | Locale;

export const LANGUAGE_LABELS: Record<LanguageSetting, string> = {
  system: 'System / PC に合わせる / 跟随系统',
  ja: '日本語',
  en: 'English',
  zh: '简体中文',
};

/** 今の表示言語。tr() がこれを読むので、変えると画面の文言も変わる */
export const i18n = $state({ locale: 'en' as Locale });

/** PC（WebView2 は Windows の表示言語）の言語から決める */
export function detectLocale(language: string = navigator.language): Locale {
  const l = language.toLowerCase();
  if (l.startsWith('ja')) return 'ja';
  if (l.startsWith('zh')) return 'zh';
  return 'en';
}

export function resolveLocale(setting: string | undefined, language?: string): Locale {
  return setting === 'ja' || setting === 'en' || setting === 'zh' ? setting : detectLocale(language);
}

/** Mac で動いているか */
export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/.test(navigator.userAgent);

/** ショートカット表記を OS に合わせる（Mac では Ctrl → Cmd、Alt → Option） */
export function sc(s: string): string {
  return isMac ? s.replace(/\bCtrl\+/g, 'Cmd+').replace(/\bAlt\+/g, 'Option+') : s;
}

/** 今の言語の文言を選ぶ */
export function tr(ja: string, en: string, zh: string): string {
  const l = i18n.locale;
  const s = l === 'ja' ? ja : l === 'zh' ? zh : en;
  return isMac ? sc(s) : s;
}
