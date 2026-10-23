// 浏览器侧公共契约：插件作者唯一需要依赖的类型来源。
// 该文件不得引入任何 Node 模块，保证 `@pulsar/plugin-client` 主入口可被浏览器直接加载。

/** 插槽声明：插件向宿主页面声明的挂载点。 */
export interface SlotSpec {
  /** 插槽名，父级通过 children 声明后才允许挂载。 */
  name: string
  /** 注册项唯一 id；同一页面内重复即报错。 */
  id: string
  /** 作用域要求：root 只允许根作用域，scoped 要求存在 scopeId。 */
  scope?: 'root' | 'scoped' | 'scoped-maybe'
  /** 同级渲染顺序，数值小的先渲染。 */
  order?: number
  /** 该注册项愿意承载的子插槽名。 */
  children?: string[]
}

/** 传给组件的插槽视图。 */
export interface SlotView {
  /** 当前作用域标识；根作用域为 undefined。 */
  scopeId?: string
  /** 在当前容器内渲染指定子插槽。 */
  renderSlot(name: string, container: Element, options?: { scopeId?: string }): void
  /** 注册卸载回调，按 LIFO 在组件卸载时执行。 */
  onCleanup(fn: () => void): void
}

/** 新签名：返回节点、卸载函数或空。 */
export type SlotComponent = (
  container: Element,
  view: SlotView,
) => Element | void | (() => void)

/** 旧签名：仅 `dom` 适配器认可。 */
export type LegacySlotComponent = (container: Element, view: SlotView) => void | (() => void)

/** 宿主 RPC 客户端。 */
export interface ClientApi {
  call<T = unknown>(method: string, args?: unknown): Promise<T>
}

/** 宿主事件订阅。 */
export interface ClientEvents {
  /** 返回取消订阅函数。 */
  on(type: string, handler: (payload: unknown) => void): () => void
}

/** 本地化词典。 */
export interface Locale {
  readonly messages: Record<string, string>
  /** 缺词时回落为 key。 */
  t(key: string): string
}

/** 样式注入：按 id 幂等注册，卸载时整体移除。 */
export interface StyleSheet {
  add(id: string, css: string): () => void
}

/** 插槽注册表对外能力。 */
export interface SlotRegistryApi {
  register(spec: SlotSpec, component: SlotComponent): () => void
  render(name: string, container: Element, options?: { scopeId?: string }): void
  /** 返回注册到无人声明槽位的 id。 */
  validate(): string[]
  /** 卸载全部已渲染内容并清空注册表。 */
  dispose(): void
}

/** 插件在浏览器侧收到的上下文。 */
export interface ClientContext {
  /** 宿主装配时写入的插件 id。 */
  id: string
  slots: SlotRegistryApi
  api: ClientApi
  events: ClientEvents
  locale: Locale
  styles: StyleSheet
}

/** 插件 apply 函数；返回函数时作为插件级 dispose。 */
export type ClientPlugin = (
  ctx: ClientContext,
) => void | (() => void) | Promise<void | (() => void)>
