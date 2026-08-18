<script lang="ts">
  import { t } from "$lib/i18n";
  import { applyTheme, readTheme, type Theme } from "$lib/theme";

  let current: Theme = $state(readTheme());

  async function apply(theme: Theme) {
    current = theme;
    await applyTheme(theme);
  }

  $effect(() => {
    // 挂载时应用当前偏好（与启动时的全局应用幂等），并跟随 current 变化。
    void applyTheme(current);
  });

  $effect(() => {
    if (current !== "system") return;
    // system 模式下 OS 主题切换时，同步原生窗口标题栏
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      void applyTheme("system");
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  });

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
