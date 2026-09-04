/** 终端设置（F34）：字号/主题持久化 + 三档 xterm 主题解析（不动全局主题）。 */

export type TermThemeName = "system" | "dark" | "light";

export interface TermSettings {
  fontSize: number;
  theme: TermThemeName;
}

const FONT_KEY = "helm-console.term-font";
const THEME_KEY = "helm-console.term-theme";

export const FONT_SIZES = [12, 14, 16, 18] as const;
export const DEFAULT_FONT = 14;

export const TERM_THEMES: { value: TermThemeName; label: string }[] = [
  { value: "system", label: "跟随系统" },
  { value: "dark", label: "终端黑" },
  { value: "light", label: "终端亮" },
];

export interface XTermTheme {
  background: string;
  foreground: string;
  cursor: string;
  selectionBackground: string;
}

const DARK_THEME: XTermTheme = {
  background: "#0d0d0d",
  foreground: "#ededed",
  cursor: "#ededed",
  selectionBackground: "#0070f3",
};

const LIGHT_THEME: XTermTheme = {
  background: "#fafafa",
  foreground: "#171717",
  cursor: "#171717",
  selectionBackground: "#0070f3",
};

/** 三档主题解析：system 跟随 prefers-color-scheme。 */
export function resolveTermTheme(name: TermThemeName, prefersDark: boolean): XTermTheme {
  if (name === "light") return LIGHT_THEME;
  if (name === "dark") return DARK_THEME;
  return prefersDark ? DARK_THEME : LIGHT_THEME;
}

/** 读取持久化设置（缺省/非法值回默认；localStorage 缺失环境安全）。 */
export function loadTermSettings(storage: Pick<Storage, "getItem">): TermSettings {
  const rawFont = storage.getItem(FONT_KEY);
  const font = Number(rawFont);
  const rawTheme = storage.getItem(THEME_KEY);
  const theme: TermThemeName =
    rawTheme === "dark" || rawTheme === "light" || rawTheme === "system" ? rawTheme : "system";
  return {
    fontSize: rawFont !== null && FONT_SIZES.includes(font as 14) ? font : DEFAULT_FONT,
    theme,
  };
}

/** 持久化设置（storage 缺失环境安全跳过）。 */
export function saveTermSettings(settings: TermSettings, storage: Pick<Storage, "setItem">): void {
  try {
    storage.setItem(FONT_KEY, String(settings.fontSize));
    storage.setItem(THEME_KEY, settings.theme);
  } catch {
    // 隐私模式等写入失败：内存态仍生效
  }
}
