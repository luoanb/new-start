import type { Context } from '@deepseek-ai/cordis'

/** 一个可在宿主中装载的插件条目。 */
export interface PluginEntry {
  /** 条目唯一名；profile 中由 `id` 提供，程序化装载时由调用方提供。 */
  name: string
  /** 依赖的 Cordis 服务；缺失时插件保持 PENDING，不会执行 apply。 */
  inject?: string[]
  /** 插件主体；所有贡献必须走 ctx.effect，以便卸载时回滚。 */
  apply: (ctx: Context) => void | Promise<void>
  /** 传给 Cordis 的插件配置。 */
  config?: any
  /** Client 面入口的绝对路径；由 profile 解析后注入。 */
  client?: string
}

/** profile / bundle patch 中的一行。 */
export interface PatchEntry {
  id: string
  /** Host 面入口，相对于声明它的补丁文件。 */
  module: string
  /** Client 面入口；缺省表示该插件没有浏览器侧。 */
  client?: string
  config?: unknown
  disabled?: boolean
}

/** 解析路径后的插件条目。 */
export interface ResolvedEntry extends PatchEntry {
  modulePath: string
  clientPath?: string
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
  clientUrl: string
}

export interface BootGraph {
  rev: string
  plugins: BootPlugin[]
}
