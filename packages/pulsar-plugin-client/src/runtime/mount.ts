import type { SlotComponent, SlotView } from '../types.ts'
import type { RuntimeMountHandle } from './browser.ts'

/**
 * 通用挂载：执行组件、收集它新插入的节点，卸载时整体移除。
 *
 * 适用于「组件自己产出 DOM」的适配器（`dom` 与编译后的 Solid 组件都是这种形态）；
 * 需要在挂载时接管容器的框架（React / Vue）应自写 `mount`。
 */
export function mountNodes(
  container: Element,
  component: SlotComponent,
  view: SlotView,
): RuntimeMountHandle {
  const before = new Set<Node>(Array.from(container.childNodes))
  const produced = component(container, view)
  const owned = Array.from(container.childNodes).filter((node) => !before.has(node))
  let unmounted = false

  return {
    unmount: () => {
      if (unmounted) return
      unmounted = true
      if (typeof produced === 'function') produced()
      if (isElement(produced)) produced.remove()
      for (const node of owned) node.parentNode?.removeChild(node)
    },
  }
}

function isElement(value: unknown): value is Element {
  return typeof value === 'object' && value !== null && (value as Node).nodeType === 1
}
