import type { Context } from '@deepseek-ai/cordis'
import type { ClientDescriptor } from '@pulsar/plugin-client/runtime'

/** Client 面入口；路径由 profile 解析为绝对路径。 */
export interface ClientSource {
  entry: string
  /** 执行环境适配器名；缺省 `dom`。 */
  runtime?: string
}

/** 一个可在宿主中装载的插件条目。 */
export interface PluginEntry {
  /** 条目唯一名；profile 中由 `id` 提供，程序化装载时由调用方提供。 */
  name: string
  /** 依赖的 Cordis 服务；缺失时插件保持 PENDING，不会执行 apply。 */
  inject?: string[]
  /** 后端入口绝对路径；存在时由 `node` 适配器装载。 */
  module?: string
  /** 插件主体；由 profile 装载时从模块导出解析，程序化装载时直接提供。 */
  apply?: (ctx: Context, config?: any) => void | Promise<void>
  /** 传给 Cordis 的插件配置。 */
  config?: any
  /** Client 面入口；缺省表示该插件没有浏览器侧。 */
  client?: ClientSource
}

/** profile / bundle patch 中的一行。 */
export interface PatchEntry {
  id: string
  /** Host 面入口，相对于声明它的补丁文件。 */
  module: string
  /** Client 面入口；字符串形态等价于 `{ entry, runtime: 'dom' }`。 */
  client?: string | ClientSource
  /** Client 面使用的执行环境适配器；对象形态未写时也取这里的值。 */
  runtime?: string
  config?: unknown
  disabled?: boolean
}

/** 解析路径后的插件条目。 */
export interface ResolvedEntry extends Omit<PatchEntry, 'client'> {
  modulePath: string
  clientPath?: string
  clientRuntime?: string
}

/** Phase 1 的面板贡献；Client 插件接入后仍保留，用于服务端侧枚举。 */
export interface PanelContribution {
  id: string
  title: string
  body: string
}

export interface PluginState {
  name: string
  enabled: boolean
  /** 是否声明了 Client 面。 */
  client: boolean
  /** 已注册的 RPC 方法数（该插件贡献的）。 */
  rpc: string[]
}

export interface HostState {
  plugins: PluginState[]
  panels: PanelContribution[]
}

/** 浏览器启动图里的一行。 */
export interface BootPlugin {
  id: string
  client: ClientDescriptor
}

export interface BootGraph {
  rev: string
  plugins: BootPlugin[]
  /** 适配器名 -> 浏览器侧 runtime 入口 URL。 */
  runtimes: Record<string, string>
  /** 模块名 -> URL；由宿主合并各适配器声明并去重，全页唯一一张 import map。 */
  imports: Record<string, string>
}
