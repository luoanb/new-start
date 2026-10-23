import { Service } from '@deepseek-ai/cordis'
import type { Context } from '@deepseek-ai/cordis'

export type RpcHandler = (args: unknown) => unknown | Promise<unknown>

/**
 * 前后端调用的 Host 端注册表。
 *
 * 插件通过 `ctx.rpc.method()` 暴露方法，返回的撤销函数应交给
 * `ctx.effect()` 管理，使方法随插件 fiber 一起卸载。
 */
export class RpcService extends Service {
  private readonly methods = new Map<string, RpcHandler>()

  constructor(ctx: Context) {
    super(ctx, 'rpc')
  }

  /** 注册一个方法；重名立即失败。 */
  method(name: string, handler: RpcHandler): () => void {
    if (this.methods.has(name)) {
      throw new Error(`rpc method already registered: ${name}`)
    }
    this.methods.set(name, handler)
    return () => {
      this.methods.delete(name)
    }
  }

  /** 调用一个方法；未注册时抛错。 */
  async invoke(name: string, args: unknown): Promise<unknown> {
    const handler = this.methods.get(name)
    if (!handler) throw new Error(`unknown rpc method: ${name}`)
    return await handler(args)
  }

  /** 当前已注册的方法名。 */
  names(): string[] {
    return [...this.methods.keys()]
  }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    rpc: RpcService
  }
}
