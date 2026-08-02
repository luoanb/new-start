<script lang="ts">
  let {
    items,
    activeId,
    onSelect,
  }: {
    items: { id: string; icon?: string; label: string }[];
    activeId: string | null;
    onSelect: (id: string) => void;
  } = $props();
</script>

<nav class="activity-bar">
  {#each items as item (item.id)}
    <button
      class="activity-item"
      class:active={activeId === item.id}
      title={item.label}
      aria-label={item.label}
      onclick={() => onSelect(item.id)}
    >
      <span class="activity-icon">
        {#if item.id === "sessions"}
          <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 11.5a8.38 8.38 0 0 1-8.5 8.5 8.5 8.5 0 0 1-3.8-.9L3 21l1.9-5.7a8.5 8.5 0 0 1-.9-3.8A8.38 8.38 0 0 1 12.5 3 8.38 8.38 0 0 1 21 11.5z" />
          </svg>
        {/if}
      </span>
    </button>
  {/each}
</nav>

<style>
  .activity-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 48px;
    height: 100%;
    padding-top: var(--space-2);
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
  }

  .activity-item {
    position: relative;
    width: 48px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-text-muted);
    transition: color var(--duration-fast) var(--ease-out);
  }

  .activity-icon { font-size: 18px; line-height: 1; }

  .activity-item:hover { color: var(--color-text); }

  .activity-item.active {
    color: var(--color-primary);
  }

  .activity-item.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 0 2px 2px 0;
    background: var(--color-primary);
  }
</style>
