import type { SlotComponent, SlotRegistryApi, SlotSpec, SlotView } from './types.ts'

interface Registration {
  spec: SlotSpec
  component: SlotComponent
}

interface RenderRecord {
  /** 产出该记录的注册项，用于按注册项精确卸载。 */
  registration: Registration
  /** 该记录挂载的插槽名。 */
  slot: string
  /** 组件渲染期间新增的节点；卸载时整体移除。 */
  nodes: Node[]
  /** 组件返回的卸载函数。 */
  disposers: Array<() => void>
  /** 通过 view.onCleanup 登记的回调，LIFO 执行。 */
  cleanups: Array<() => void>
}

/**
 * 插槽注册表：唯一负责「注册 → 渲染 → 卸载」的组件树。
 *
 * 旧签名组件直接向容器 append，因此用「渲染前后子节点差集」追踪其产出；
 * 新签名组件可以返回节点或卸载函数，两者都会被记账。
 */
export class SlotRegistry implements SlotRegistryApi {
  private readonly registrations = new Map<string, Registration>()
  private readonly rendered = new Map<Element, Map<string, RenderRecord[]>>()

  /** 注册插槽；id 重复、组件非法时立即抛错。返回注销函数，会一并卸载已渲染内容。 */
  register(spec: SlotSpec, component: SlotComponent): () => void {
    if (!spec || typeof spec.name !== 'string' || typeof spec.id !== 'string') {
      throw new Error('invalid slot registration')
    }
    if (typeof component !== 'function') {
      throw new Error(`slot ${spec.id} has no component`)
    }
    if (this.registrations.has(spec.id)) {
      throw new Error(`duplicate slot id: ${spec.id}`)
    }
    const registration: Registration = { spec, component }
    this.registrations.set(spec.id, registration)
    return () => {
      if (this.registrations.get(spec.id) === registration) {
        this.registrations.delete(spec.id)
      }
      this.unmountRegistration(registration)
    }
  }

  /** 渲染指定插槽；同名同容器的旧内容先卸载，保证幂等。 */
  render(name: string, container: Element, options?: { scopeId?: string }): void {
    this.renderInto(name, container, options?.scopeId, undefined)
  }

  /** 返回注册到了无人声明槽位的 id。 */
  validate(): string[] {
    const declared = new Set(['root'])
    for (const { spec } of this.registrations.values()) {
      for (const child of spec.children ?? []) declared.add(child)
    }
    return [...this.registrations.values()]
      .filter(({ spec }) => !declared.has(spec.name))
      .map(({ spec }) => spec.id)
  }

  /** 卸载全部渲染内容并清空注册表。 */
  dispose(): void {
    for (const slots of this.rendered.values()) {
      for (const records of slots.values()) {
        for (const record of records) this.unmountRecord(record)
      }
    }
    this.rendered.clear()
    this.registrations.clear()
  }

  private renderInto(
    name: string,
    container: Element,
    scopeId: string | undefined,
    parent: Registration | undefined,
  ): void {
    const declared = parent ? parent.spec.children ?? [] : ['root']
    if (!declared.includes(name)) {
      throw new Error(`slot not declared by parent: ${name}`)
    }

    // 每次渲染前清掉同容器同插槽的旧记录，使「渲染」对任何层级都幂等。
    this.unmount(container, name)

    const cells = [...this.registrations.values()]
      .filter(({ spec }) => spec.name === name && this.scopeMatches(spec.scope, scopeId))
      .sort((a, b) => (a.spec.order ?? 0) - (b.spec.order ?? 0))

    for (const registration of cells) {
      this.mount(registration, name, container, scopeId)
    }
  }

  private mount(
    registration: Registration,
    slot: string,
    container: Element,
    scopeId: string | undefined,
  ): void {
    const record: RenderRecord = { registration, slot, nodes: [], disposers: [], cleanups: [] }
    const before = new Set<Node>(Array.from(container.childNodes))
    const view: SlotView = {
      scopeId,
      renderSlot: (child, node, options) => {
        this.renderInto(
          child,
          node,
          options && 'scopeId' in options ? options.scopeId : scopeId,
          registration,
        )
      },
      onCleanup: (fn) => {
        record.cleanups.push(fn)
      },
    }

    const produced = registration.component(container, view)
    if (isElement(produced)) {
      if (!produced.parentNode) container.append(produced)
    } else if (typeof produced === 'function') {
      record.disposers.push(produced)
    }

    record.nodes = Array.from(container.childNodes).filter((node) => !before.has(node))
    let slots = this.rendered.get(container)
    if (!slots) {
      slots = new Map()
      this.rendered.set(container, slots)
    }
    const list = slots.get(slot) ?? []
    list.push(record)
    slots.set(slot, list)
  }

  /** 卸载某容器上指定插槽的渲染记录；slot 缺省表示该容器的全部记录。 */
  private unmount(container: Element, slot?: string): void {
    const slots = this.rendered.get(container)
    if (!slots) return
    if (!slot) {
      for (const records of slots.values()) {
        for (const record of records) this.unmountRecord(record)
      }
      this.rendered.delete(container)
      return
    }
    const records = slots.get(slot)
    if (!records) return
    for (const record of records) this.unmountRecord(record)
    slots.delete(slot)
  }

  /** 卸载某个注册项在所有容器上的渲染记录。 */
  private unmountRegistration(registration: Registration): void {
    for (const [container, slots] of this.rendered) {
      let touched = false
      for (const [slot, records] of slots) {
        const keep = records.filter((record) => record.registration !== registration)
        if (keep.length === records.length) continue
        for (const record of records) {
          if (record.registration === registration) this.unmountRecord(record)
        }
        touched = true
        if (keep.length) slots.set(slot, keep)
        else slots.delete(slot)
      }
      if (touched && slots.size === 0) this.rendered.delete(container)
    }
  }

  private unmountRecord(record: RenderRecord): void {
    for (const dispose of record.disposers) dispose()
    for (let index = record.cleanups.length - 1; index >= 0; index -= 1) {
      record.cleanups[index]!()
    }
    for (const node of record.nodes) node.parentNode?.removeChild(node)
    record.disposers.length = 0
    record.cleanups.length = 0
    record.nodes.length = 0
  }

  private scopeMatches(scope: SlotSpec['scope'], scopeId: string | undefined): boolean {
    if (!scope || scope === 'root') return true
    if (scope === 'scoped') return scopeId !== undefined
    if (scope === 'scoped-maybe') return true
    throw new Error(`unknown slot scope: ${scope}`)
  }
}

/** 用 nodeType 判断节点，避免依赖全局 Element（便于在非浏览器环境做单测）。 */
function isElement(value: unknown): value is Element {
  return typeof value === 'object' && value !== null && (value as Node).nodeType === 1
}
