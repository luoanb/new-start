import type { IncomingMessage, ServerResponse } from 'node:http'

/** 一次激活的输入：插件 id、入口绝对路径与 profile 透传的配置。 */
export interface PluginSource {
  /** 插件 id，适配器用它做产物目录命名空间。 */
  id: string
  /** 入口绝对路径，由宿主解析 profile 得到。 */
  entry: string
  /** profile 中该插件声明的执行环境配置，原样透传。 */
  options?: unknown
}

/** 浏览器侧执行描述：宿主原样写进 boot graph。 */
export interface ClientDescriptor {
  /** 模块可被 import 的 URL。 */
  url: string
  /** 非 JS 产物（CSS、字体、图片）的可访问 URL。 */
  assets?: string[]
}

/** 宿主进程内实例；具体形态由适配器决定，宿主只负责挂进 fiber。 */
export type PluginObject = object

/** 一次激活的产出；两种产出可以同时存在。 */
export interface Activation {
  /** 有值则作为插件实例挂进宿主 fiber。 */
  server?: PluginObject
  /** 有值则写进 boot graph，交浏览器执行。 */
  client?: ClientDescriptor
  /** 不依赖 ctx.effect 时的显式回滚。 */
  dispose?: () => void | Promise<void>
}

export type RouteHandler = (
  req: IncomingMessage,
  res: ServerResponse,
) => void | Promise<void>

/** 宿主提供给适配器的受限上下文。 */
export interface RuntimeContext {
  /** 注册 effect：execute 返回的撤销函数随插件 fiber 回滚。 */
  effect(execute: () => void | (() => void)): () => void
  /** 注册 HTTP 路由；返回撤销函数。 */
  route(path: string, handler: RouteHandler): () => void
  /** 产物目录根；适配器只允许在其下自建子目录。 */
  artifactsDir: string
  /** 开发期变更通知；缺省时适配器降级为不支持热替换。 */
  notify?(change: RuntimeChange): void
  logger: Logger
}

/** 适配器可用的最小日志能力。 */
export interface Logger {
  info(message: string): void
  warn(message: string): void
  error(message: string): void
}

/** 适配器向宿主上报的变更；宿主只做转发，不解释原因。 */
export interface RuntimeChange {
  /** 变更类别，例如 `plugin/changed`。 */
  type: string
  payload?: unknown
}

/** 执行环境适配器：编译、存放、投递、执行全权负责。 */
export interface PluginRuntime {
  readonly name: string
  /** 该适配器需要的页面级共享依赖；宿主合并去重，不解释内容。 */
  readonly imports?: Record<string, string>
  /** 浏览器侧适配器产物入口 URL；纯服务端适配器不提供。 */
  readonly clientEntry?: string
  activate(source: PluginSource, ctx: RuntimeContext): Promise<Activation>
}
