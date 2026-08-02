<script lang="ts">
  import type { ViewMeta } from "./views";

  let {
    tabs,
    activeId,
    split,
    onSelect,
    onClose,
  }: {
    tabs: ViewMeta[];
    activeId: string | null;
    split: boolean;
    onSelect: (id: string) => void;
    onClose: (id: string) => void;
  } = $props();
</script>

<div class="editor-tabs">
  {#if split}
    {#each tabs as tab}
      <button
        class="tab"
        class:active={activeId === tab.id}
        onclick={() => onSelect(tab.id)}
      >
        {#if tab.icon}<span class="icon">{@html tab.icon}</span>{/if}
        <span class="label">{tab.label}</span>
        <span
          class="close"
          role="button"
          tabindex="-1"
          title="Close"
          onclick={(e) => { e.stopPropagation(); onClose(tab.id); }}
        >✕</span>
      </button>
    {/each}
  {:else}
    {@const active = tabs.find((t) => t.id === activeId) ?? tabs[0]}
    {#if active}
      <button class="tab" class:active>
        {#if active.icon}<span class="icon">{@html active.icon}</span>{/if}
        <span class="label">{active.label}</span>
        {#if active.id !== "chat"}
          <span
            class="close"
            role="button"
            tabindex="-1"
            title="Close"
            onclick={(e) => { e.stopPropagation(); onClose(active.id); }}
          >✕</span>
        {/if}
      </button>
    {/if}
  {/if}
  <div class="tabs-spacer"></div>
</div>

<style>
  .editor-tabs {
    display: flex;
    align-items: flex-end;
    flex-shrink: 0;
    height: 32px;
    background: var(--color-bg);
    border-bottom: var(--border-width) solid var(--color-border);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .editor-tabs::-webkit-scrollbar { display: none; }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 32px;
    padding: 0 var(--space-2);
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: var(--fs-sm);
    color: var(--color-text-muted);
    border-right: var(--border-width) solid var(--color-border);
    border-top: 2px solid transparent;
    transition: background var(--duration-fast) var(--ease-out), color var(--duration-fast) var(--ease-out);
    white-space: nowrap;
  }

  .tab:hover { background: var(--color-hover); color: var(--color-text); }

  .tab.active {
    background: var(--color-surface);
    color: var(--color-text);
    border-top-color: var(--color-primary);
  }

  .icon { display: inline-flex; align-items: center; font-size: 12px; line-height: 1; }
  .label { font-size: var(--fs-xs); font-weight: 500; }

  .close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    font-size: 10px;
    border-radius: var(--radius-sm);
    color: inherit;
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .tab:hover .close { opacity: 0.7; }
  .tab.active .close { opacity: 0.6; }
  .close:hover { opacity: 1 !important; background: var(--color-hover); }

  .tabs-spacer { flex: 1; height: 100%; }
</style>
