<script lang="ts">
  import { t, setLocale, getLocale } from "$lib/i18n";

  type Locale = "zh" | "en";

  let open = $state(false);
  let current: Locale = $state(getLocale() as Locale);

  function toggle() { open = !open; }

  function select(locale: Locale) {
    current = locale;
    setLocale(locale);
    open = false;
  }
</script>

<div class="locale-switcher">
  <button class="trigger" onclick={toggle} title={t("locale.label")}>
    <span class="label">{current === "zh" ? "中" : "EN"}</span>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
    <div class="dropdown">
      <button class="option" class:active={current === "en"} onclick={() => select("en")}>
        English
      </button>
      <button class="option" class:active={current === "zh"} onclick={() => select("zh")}>
        中文
      </button>
    </div>
  {/if}
</div>

<style>
  .locale-switcher {
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
    font-size: 11px;
    font-weight: 600;
    transition: background var(--duration-fast) var(--ease-out),
                color var(--duration-fast) var(--ease-out);
  }

  .trigger:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .label {
    line-height: 1;
  }

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
    min-width: 90px;
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
