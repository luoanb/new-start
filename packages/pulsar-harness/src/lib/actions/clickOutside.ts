/**
 * Svelte action: 点击目标元素外部时触发回调。
 *
 * ```svelte
 * <div use:clickOutside={onClose}>
 *   <button>面板内容</button>
 * </div>
 * ```
 *
 * 传 `null` 可临时禁用监听（回调不执行）。
 */
export function clickOutside(
  node: HTMLElement,
  callback: (() => void) | null
) {
  function onPointerDown(e: PointerEvent) {
    if (!callback) return;
    const target = e.target as Node;
    if (!node.contains(target)) {
      callback();
    }
  }
  document.addEventListener("pointerdown", onPointerDown);
  return {
    update(newCallback: (() => void) | null) {
      callback = newCallback;
    },
    destroy() {
      document.removeEventListener("pointerdown", onPointerDown);
    },
  };
}