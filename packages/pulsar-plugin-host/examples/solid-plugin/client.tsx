// solid-plugin 的浏览器侧：用 Solid 组件注册一个 main 槽。
// 经 solid 适配器编译，solid-js 由 import map 指向适配器投递的共享 ESM。

import { createRoot, createSignal } from 'solid-js'
import { insert } from 'solid-js/web'

interface SlotView {
  scopeId?: string
  renderSlot(name: string, container: Element, options?: { scopeId?: string }): void
}

interface ClientContext {
  id: string
  slots: {
    register(
      spec: { name: string; id: string; order?: number },
      component: (container: Element, view: SlotView) => unknown,
    ): () => void
  }
}

function Counter() {
  const [count, setCount] = createSignal(0)
  return (
    <button type="button" onClick={() => setCount(count() + 1)}>
      solid count: {count()}
    </button>
  )
}

export function apply(ctx: ClientContext) {
  ctx.slots.register({ name: 'main', id: 'solid-panel' }, (container) => {
    // 插槽容器由宿主提供，这里显式建立 root 并把卸载交回宿主。
    let dispose: () => void = () => {}
    createRoot((unmount) => {
      dispose = unmount
      insert(container, () => <Counter />)
    })
    return dispose
  })
}
