<script lang="ts">
  // 轻量上下文菜单：透明 backdrop + fixed 定位菜单（对齐 ViewContainer ⋯ 菜单样式）。
  // 由触发方管理显隐（items 重建即开，onClose 关闭）；菜单项选择后自动关闭。
  import type { Component } from "svelte";

  export type ContextMenuItem = {
    label: string;
    /** 可选图标组件（`unplugin-icons` 构建期产出，12px，与视图图标风格一致）。 */
    icon?: Component;
    /** 破坏性操作（删除等）着 --color-error。 */
    danger?: boolean;
    disabled?: boolean;
    onSelect?: () => void;
  };

  /** 分隔符条目：仅作视觉分组，不可 hover / 点击 / 聚焦。 */
  export type ContextMenuDivider = {
    divider: true;
  };

  /** 菜单条目：命令项或分隔符。 */
  export type ContextMenuEntry = ContextMenuItem | ContextMenuDivider;

  let {
    items,
    x,
    y,
    onClose,
  }: {
    items: ContextMenuEntry[];
    /** 触发坐标（clientX/clientY）。 */
    x: number;
    y: number;
    onClose: () => void;
  } = $props();

  const MENU_W = 200;
  const ITEM_H = 32;

  // 视口内收敛：菜单右下越界时向上/向左回退，并夹在视口内（不裁剪到容器外）。
  let style = $derived.by(() => {
    const vw = typeof window !== "undefined" ? window.innerWidth : 800;
    const vh = typeof window !== "undefined" ? window.innerHeight : 600;
    const left = Math.max(8, Math.min(x, vw - MENU_W - 8));
    const top = Math.max(8, Math.min(y, vh - items.length * ITEM_H - 12));
    return `left:${left}px;top:${top}px;`;
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="menu-backdrop" role="presentation" onclick={onClose}></div>
<div class="ctx-menu" role="menu" style={style}>
  {#each items as entry, i (i)}
    {#if "divider" in entry}
      <div class="ctx-divider" role="separator"></div>
    {:else}
      <button
        class="ctx-item"
        class:danger={entry.danger}
        class:disabled={entry.disabled}
        role="menuitem"
        disabled={entry.disabled}
        onclick={(e) => {
          e.stopPropagation();
          if (entry.disabled) return;
          entry.onSelect?.();
          onClose();
        }}
      >
        {#if entry.icon}
          {@const Icon = entry.icon}
          <span class="ctx-icon"><Icon /></span>
        {/if}
        <span class="ctx-label">{entry.label}</span>
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: transparent;
  }
  .ctx-menu {
    position: fixed;
    z-index: 100;
    min-width: 180px;
    max-width: 260px;
    background: var(--color-elevated);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ctx-divider {
    flex: none;
    height: var(--border-width);
    margin: var(--space-1) var(--space-2);
    background: var(--color-border);
  }
  .ctx-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 6px var(--space-2);
    border: none;
    background: transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: var(--fs-sm);
    color: var(--color-text);
    text-align: left;
    white-space: nowrap;
  }
  .ctx-item:hover:not(:disabled) {
    background: var(--color-hover);
  }
  .ctx-item.danger {
    color: var(--color-error);
  }
  .ctx-item.danger:hover:not(:disabled) {
    background: var(--color-error-bg);
  }
  .ctx-item.disabled {
    opacity: 0.45;
    cursor: default;
  }
  .ctx-icon {
    display: inline-flex;
    align-items: center;
    width: 14px;
    flex-shrink: 0;
  }
  .ctx-icon :global(svg) {
    width: 12px;
    height: 12px;
    display: block;
  }
  .ctx-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
