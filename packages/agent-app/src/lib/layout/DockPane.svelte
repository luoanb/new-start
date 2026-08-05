<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    icon,
    title,
    collapsed = false,
    onToggle,
    children,
  }: {
    icon?: string;
    title: string;
    collapsed?: boolean;
    onToggle?: () => void;
    children?: Snippet;
  } = $props();
</script>

<div class="dock-pane" class:collapsed>
  <header class="pane-header">
    {#if icon}
      <span class="pane-icon">{icon}</span>
    {/if}
    <span class="pane-title">{title}</span>
    {#if onToggle}
      <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
      <button class="pane-action" onclick={onToggle} title={collapsed ? "Expand" : "Collapse"}>
        {collapsed ? "◀" : "▶"}
      </button>
    {/if}
  </header>
  {#if !collapsed}
    <div class="pane-body">
      {@render children?.()}
    </div>
  {/if}
</div>

<style>
  .dock-pane {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    min-width: 0;
  }

  .pane-header {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    height: 32px;
    flex-shrink: 0;
    border-bottom: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .pane-icon { font-size: 13px; line-height: 1; }
  .pane-title {
    flex: 1;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--color-text-muted);
    letter-spacing: 0.02em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .pane-action {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 10px;
    color: var(--color-text-muted);
    padding: 2px 4px;
    line-height: 1;
    border-radius: var(--radius-sm);
  }
  .pane-action:hover { background: var(--color-hover); color: var(--color-text); }

  .pane-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    /* flex 容器：子面板（如 LogPanel）可拉伸填满并在内部滚动，
       避免 height:100% 不可靠导致的 外层滚动条 + 内层滚动条 双滚动条。 */
    display: flex;
    flex-direction: column;
  }
</style>
