// 浏览器侧唯一入口：零 Node 依赖，插件作者只从这里取类型与 API。

import { defineClientPlugin } from './define.ts'
import { EventStream } from './events.ts'
import { LocaleDictionary } from './locale.ts'
import { RpcClient } from './rpc.ts'
import { SlotRegistry } from './slots.ts'
import { DomStyleSheet } from './styles.ts'
import type {
  ClientApi,
  ClientContext,
  ClientEvents,
  Locale,
  SlotRegistryApi,
  StyleSheet,
} from './types.ts'

export { defineClientPlugin }
export { EventStream } from './events.ts'
export { legacy } from './legacy.ts'
export { LocaleDictionary } from './locale.ts'
export { RpcClient } from './rpc.ts'
export { SlotRegistry } from './slots.ts'
export { DomStyleSheet } from './styles.ts'

export type {
  ClientApi,
  ClientContext,
  ClientEvents,
  ClientPlugin,
  LegacySlotComponent,
  Locale,
  SlotComponent,
  SlotRegistryApi,
  SlotSpec,
  SlotView,
  StyleSheet,
} from './types.ts'

export interface ClientContextOptions {
  /** 插件 id，由宿主写入。 */
  id: string
  /** RPC 端点，默认 `/api/rpc`。 */
  rpcEndpoint?: string
  /** 事件端点，默认 `/api/events`。 */
  eventsEndpoint?: string
  /** 复用外部注册表；缺省时新建。 */
  slots?: SlotRegistryApi
  api?: ClientApi
  events?: ClientEvents
  locale?: Locale
  styles?: StyleSheet
}

/** 组装一个插件上下文；未传入的部分使用默认实现。 */
export function createClientContext(options: ClientContextOptions): ClientContext {
  return {
    id: options.id,
    slots: options.slots ?? new SlotRegistry(),
    api: options.api ?? new RpcClient(options.rpcEndpoint),
    events: options.events ?? new EventStream(options.eventsEndpoint),
    locale: options.locale ?? new LocaleDictionary(),
    styles: options.styles ?? new DomStyleSheet(),
  }
}
