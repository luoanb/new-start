<script lang="ts">
  import { locale, t } from "$lib/i18n";
  $locale;

  const STORAGE_KEY = "theme-preference";

  type Theme = "light" | "dark" | "system";

  let open = $state(false);
  let current: Theme = $state(initTheme());

  function initTheme(): Theme {
    if (typeof localStorage === "undefined") return "system";
    return (localStorage.getItem(STORAGE_KEY) as Theme) ?? "system";
  }

  function apply(theme: Theme) {
    current = theme;
    localStorage.setItem(STORAGE_KEY, theme);
    const el = document.documentElement;
    if (theme === "system") {
      el.removeAttribute("data-theme");
    } else {
      el.dataset.theme = theme;
    }
  }

  function toggle() { open = !open; }

  function select(theme: Theme) {
    apply(theme);
    open = false;
  }

  $effect(() => {
    if (current !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {};
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  });

  $effect(() => { apply(current); });

  const label: Record<Theme, string> = {
    light: t("themeSwitcher.light"),
    dark: t("themeSwitcher.dark"),
    system: t("themeSwitcher.system"),
  };
</script>

<div class="theme-switcher">
  <button class="trigger" onclick={toggle} title={t("themeSwitcher.system")}>
    <span class="icon">
      {#if current === "light"}
        &#9788;
      {:else if current === "dark"}
        &#9790;
      {:else}
        &#9728;
      {/if}
    </span>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
    <div class="dropdown">
      {#each ["light", "dark", "system"] as theme}
        <button
          class="option"
          class:active={current === theme}
          onclick={() => select(theme as Theme)}
        >
          {label[theme as Theme]}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .theme-switcher {
    position: relative;
    display: inline-block;
  }

  .trigger {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 14px;
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }

  .trigger:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .icon { line-height: 1; }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
  }

  .dropdown {
    position: absolute;
    right: 0;
    top: calc(100% + var(--space-1));
    z-index: 20;
    min-width: 100px;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  }

  .option {
    display: block;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--color-text);
    font-size: var(--fs-sm);
    text-align: left;
    cursor: pointer;
    transition: background var(--duration-fast) var(--ease-out);
  }

  .option:hover {
    background: var(--color-hover);
  }

  .option.active {
    font-weight: 600;
    color: var(--color-primary);
  }
</style>
