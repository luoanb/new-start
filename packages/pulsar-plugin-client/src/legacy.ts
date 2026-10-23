import type { LegacySlotComponent, SlotComponent, SlotView } from './types.ts'

/**
 * 把旧 `(container, view)` 组件显式标记为 legacy。
 *
 * 旧组件假设容器是空的、自己 append 后不再管理节点，因此包装后每次渲染
 * 先清空容器，卸载时统一执行它返回的清理函数。
 */
export function legacy(component: LegacySlotComponent): SlotComponent {
  return (container: Element, view: SlotView) => {
    container.replaceChildren()
    return component(container, view)
  }
}
