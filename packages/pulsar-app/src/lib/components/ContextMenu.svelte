<script lang="ts">
  // 轻量上下文菜单：透明 backdrop + fixed 定位菜单（对齐 ViewContainer ⋯ 菜单样式）。
  // 由触发方管理显隐（items 重建即开，onClose 关闭）；菜单项选择后自动关闭。
  export type ContextMenuItem = {
    label: string;
    /** 可选内联 SVG（12px，与视图图标风格一致）。 */
    icon?: string;
    /** 破坏性操作（删除等）着 --color-error。 */
    danger?: boolean;
    disabled?: boolean;
    onSelect?: () => void;
  };

  let {
    items,
    x,
    y,
    onClose,
  }: {
    items: ContextMenuItem[];
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
  {#each items as item (item.label)}
    <button
      class="ctx-item"
      class:danger={item.danger}
      class:disabled={item.disabled}
      role="menuitem"
      disabled={item.disabled}
      onclick={(e) => {
        e.stopPropagation();
        if (item.disabled) return;
        item.onSelect?.();
        onClose();
      }}
    >
      {#if item.icon}<span class="ctx-icon">{@html item.icon}</span>{/if}
      <span class="ctx-label">{item.label}</span>
    </button>
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
  .ctx-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
