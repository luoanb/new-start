import type { SlotComponent, SlotView } from '../types.ts'
import { BrowserRuntime, type RuntimeMountHandle } from './browser.ts'
import { esbuildCompiler } from './compiler/esbuild.ts'
import type { Compiler } from './compiler/types.ts'
import { mountNodes } from './mount.ts'

/**
 * 原生 DOM 适配器：承载旧 `(container, view)` 写法，零构建配置。
 *
 * P0 不把 `@pulsar/plugin-client` 声明为 external，插件若 import 它会被内联；
 * 共享依赖切分（import map）在宿主收缩步骤一并接入。
 */
export class DomRuntime extends BrowserRuntime {
  readonly name = 'dom'

  protected readonly compiler: Compiler = esbuildCompiler({
    external: [],
    imports: {},
    clientEntry: '/client/runtime.js',
  })

  mount(container: Element, component: SlotComponent, view: SlotView): RuntimeMountHandle {
    return mountNodes(container, component, view)
  }

  slot(container: Element, name: string, view: SlotView): void {
    view.renderSlot(name, container)
  }
}
