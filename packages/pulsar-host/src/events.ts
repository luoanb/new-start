import { Service } from '@deepseek-ai/cordis'
import type { Context } from '@deepseek-ai/cordis'
import type { ServerResponse } from 'node:http'

/**
 * Host → Browser 的事件流。
 *
 * 使用 SSE 单向下发；插件通过 `ctx.clientEvents.emit()` 推送，
 * 浏览器运行时在 `/api/events` 上消费。
 */
export class ClientEvents extends Service {
  private readonly clients = new Set<ServerResponse>()

  constructor(ctx: Context) {
    super(ctx, 'clientEvents')
  }

  /** 接入一个 SSE 响应；连接关闭时自动摘除。 */
  subscribe(res: ServerResponse): () => void {
    this.clients.add(res)
    const dispose = () => {
      this.clients.delete(res)
    }
    res.on('close', dispose)
    return dispose
  }

  /** 广播一条事件；无订阅者时静默丢弃。 */
  emit(type: string, payload: unknown): void {
    const frame = `data: ${JSON.stringify({ type, payload })}\n\n`
    for (const res of this.clients) {
      if (res.writableEnded) {
        this.clients.delete(res)
        continue
      }
      res.write(frame)
    }
  }

  /** 当前订阅者数量。 */
  get size(): number {
    return this.clients.size
  }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    clientEvents: ClientEvents
  }
}
