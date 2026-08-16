<script lang="ts">
  import { t } from "$lib/i18n";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const STORAGE_KEY = "theme-preference";

  type Theme = "light" | "dark" | "system";

  let current: Theme = $state(initTheme());

  function initTheme(): Theme {
    if (typeof localStorage === "undefined") return "system";
    return (localStorage.getItem(STORAGE_KEY) as Theme) ?? "system";
  }

  function isTauri(): boolean {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  }

  function resolveOsTheme(theme: Theme): "light" | "dark" {
    if (theme === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    }
    return theme;
  }

  async function apply(theme: Theme) {
    current = theme;
    localStorage.setItem(STORAGE_KEY, theme);
    const el = document.documentElement;
    if (theme === "system") {
      el.removeAttribute("data-theme");
    } else {
      el.dataset.theme = theme;
    }
    // 让原生 OS 窗口标题栏跟随主题
    if (isTauri()) {
      try {
        await getCurrentWindow().setTheme(resolveOsTheme(theme));
      } catch {
        /* 某些平台/版本不支持 setTheme，忽略 */
      }
    }
  }

  $effect(() => {
    if (current !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      if (isTauri()) {
        getCurrentWindow()
          .setTheme(mq.matches ? "dark" : "light")
          .catch(() => {});
      }
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  });

  $effect(() => { apply(current); });

  const themes: Theme[] = ["light", "dark", "system"];
  let label = $derived({
    light: t("themeSwitcher.light"),
    dark: t("themeSwitcher.dark"),
    system: t("themeSwitcher.system"),
  });
</script>

<div class="theme-switcher" role="group" aria-label={t("settings.theme")}>
  {#each themes as theme}
    <button
      class="option"
      class:active={current === theme}
      onclick={() => apply(theme)}
    >
      {label[theme]}
    </button>
  {/each}
</div>

<style>
  .theme-switcher {
    display: flex;
    gap: var(--space-1);
  }

  .option {
    flex: 1;
    padding: var(--space-2) var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-text-muted);
    font-size: var(--fs-sm);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--ease-out),
                background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }

  .option:hover {
    border-color: var(--color-primary);
    color: var(--color-text);
  }

  .option.active {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
    font-weight: 600;
  }
</style>
