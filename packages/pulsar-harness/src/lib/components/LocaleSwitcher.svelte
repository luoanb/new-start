<script lang="ts">
  import { t, setLocale, getLocale } from "$lib/i18n";

  type Locale = "zh" | "en";

  let current: Locale = $state(getLocale() as Locale);

  const options: { id: Locale; label: string }[] = [
    { id: "zh", label: "中文" },
    { id: "en", label: "English" },
  ];

  function select(locale: Locale) {
    current = locale;
    setLocale(locale);
  }
</script>

<div class="locale-switcher" role="group" aria-label={t("settings.language")}>
  {#each options as option}
    <button
      class="option"
      class:active={current === option.id}
      onclick={() => select(option.id)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .locale-switcher {
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
