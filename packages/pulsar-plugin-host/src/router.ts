import type { IncomingMessage, ServerResponse } from 'node:http'
import { Service } from '@deepseek-ai/cordis'
import type { Context } from '@deepseek-ai/cordis'

export type RouteHandler = (
  req: IncomingMessage,
  res: ServerResponse,
) => void | Promise<void>

/**
 * HTTP 路由注册表：插件贡献 HTTP 端点的唯一入口。
 *
 * `get()` 返回撤销函数；插件应在 `ctx.effect()` 中调用它，
 * 使路由随插件 fiber 一起卸载。
 */
export class RouterService extends Service {
  private readonly routes = new Map<string, RouteHandler>()
  private readonly prefixes = new Map<string, RouteHandler>()

  constructor(ctx: Context) {
    super(ctx, 'router')
  }

  /** 注册一条精确路径路由；重复注册立即失败。 */
  get(path: string, handler: RouteHandler): () => void {
    if (this.routes.has(path) || this.prefixes.has(path)) {
      throw new Error(`route already registered: ${path}`)
    }
    this.routes.set(path, handler)
    return () => {
      this.routes.delete(path)
    }
  }

  /** 注册一条前缀路由；最长前缀优先。 */
  prefix(path: string, handler: RouteHandler): () => void {
    if (this.routes.has(path) || this.prefixes.has(path)) {
      throw new Error(`route already registered: ${path}`)
    }
    this.prefixes.set(path, handler)
    return () => {
      this.prefixes.delete(path)
    }
  }

  /** 命中则执行处理器并返回 true；未命中返回 false。 */
  async dispatch(req: IncomingMessage, res: ServerResponse): Promise<boolean> {
    const path = new URL(req.url ?? '/', 'http://localhost').pathname

    const exact = this.routes.get(path)
    if (exact) {
      await exact(req, res)
      return true
    }

    let matched: [string, RouteHandler] | undefined
    for (const entry of this.prefixes) {
      if (!path.startsWith(entry[0])) continue
      if (!matched || entry[0].length > matched[0].length) matched = entry
    }
    if (!matched) return false
    await matched[1](req, res)
    return true
  }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    router: RouterService
  }
}
