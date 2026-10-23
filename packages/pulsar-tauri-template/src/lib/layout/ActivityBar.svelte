<script lang="ts">
  import { t } from "$lib/i18n";
  import type { Component, Snippet } from "svelte";

  let {
    items,
    activeId,
    onSelect,
    footer,
  }: {
    items: { id: string; icon?: Component; label: string }[];
    activeId: string | null;
    onSelect: (id: string) => void;
    footer?: Snippet;
  } = $props();
</script>

<nav class="activity-bar">
  {#each items as item (item.id)}
    <button
      class="activity-item"
      class:active={activeId === item.id}
      title={t(item.label)}
      aria-label={t(item.label)}
      onclick={() => onSelect(item.id)}
    >
      <span class="activity-icon">
        {#if item.icon}
          {@const Icon = item.icon}
          <Icon />
        {/if}
      </span>
    </button>
  {/each}

  <div class="spacer"></div>

  {#if footer}
    {@render footer()}
  {/if}
</nav>

<style>
  .activity-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 40px;
    height: 100%;
    padding-top: var(--space-2);
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
  }

  :global(.activity-item) {
    position: relative;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    outline: none;
    cursor: pointer;
    color: var(--color-text-muted);
    transition: color var(--duration-fast) var(--ease-out);
  }

  :global(.activity-icon) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }

  :global(.activity-icon svg) {
    width: 18px;
    height: 18px;
    display: block;
  }

  :global(.activity-item:hover) { color: var(--color-text); }

  :global(.activity-item:focus-visible) { outline: none; }

  :global(.activity-item.active) {
    color: var(--color-primary);
  }

  :global(.activity-item.active::before) {
    content: "";
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 0 2px 2px 0;
    background: var(--color-primary);
  }

  .spacer {
    flex: 1;
  }
</style>
