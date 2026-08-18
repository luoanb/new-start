// 主题偏好：唯一来源模块。
// 背景：此前主题只在 SettingsDialog 内的 ThemeSwitcher 挂载时才 apply，
// 而 SettingsDialog 是条件渲染，导致 App 启动时从不应用已保存的主题，
// 打开设置弹窗时才突然生效造成"没点切换却变主题"的跳变。
// 现在启动时（applyThemeOnBoot）就应用偏好，ThemeSwitcher 挂载时再 apply 一次（幂等）。

import { getCurrentWindow } from "@tauri-apps/api/window";

export type Theme = "light" | "dark" | "system";

export const THEME_STORAGE_KEY = "theme-preference";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function readTheme(): Theme {
  if (typeof localStorage === "undefined") return "system";
  const value = localStorage.getItem(THEME_STORAGE_KEY);
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

export function resolveOsTheme(theme: Theme): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

/** 把主题应用到 html[data-theme]（驱动 CSS 变量），并同步原生窗口标题栏、持久化偏好。 */
export async function applyTheme(theme: Theme): Promise<void> {
  if (typeof document === "undefined") return;
  const el = document.documentElement;
  if (theme === "system") {
    el.removeAttribute("data-theme");
  } else {
    el.dataset.theme = theme;
  }
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  }
  if (isTauri()) {
    try {
      await getCurrentWindow().setTheme(resolveOsTheme(theme));
    } catch {
      /* 某些平台/版本不支持 setTheme，忽略 */
    }
  }
}

/** App 启动时应用已保存的主题；未保存或无值按 system 处理（跟随 OS）。 */
export function applyThemeOnBoot(): void {
  void applyTheme(readTheme());
}
