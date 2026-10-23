import type { ClientEvents } from './types.ts'

type Handler = (payload: unknown) => void

/**
 * 事件订阅：惰性建立唯一一条 SSE 连接，按事件类型分发。
 *
 * 只有出现首个订阅者时才连接，避免无插件的页面常驻空连接。
 */
export class EventStream implements ClientEvents {
  private source: EventSource | undefined
  private readonly handlers = new Map<string, Set<Handler>>()

  constructor(private readonly endpoint = '/api/events') {}

  on(type: string, handler: Handler): () => void {
    const list = this.handlers.get(type) ?? new Set<Handler>()
    list.add(handler)
    this.handlers.set(type, list)
    this.connect()
    return () => {
      const current = this.handlers.get(type)
      if (!current) return
      current.delete(handler)
      if (current.size === 0) this.handlers.delete(type)
    }
  }

  /** 关闭连接并清空全部订阅。 */
  dispose(): void {
    this.source?.close()
    this.source = undefined
    this.handlers.clear()
  }

  private connect(): void {
    if (this.source) return
    const source = new EventSource(this.endpoint)
    source.onmessage = (event) => {
      try {
        const frame = JSON.parse(event.data) as { type?: string; payload?: unknown }
        if (typeof frame.type !== 'string') return
        for (const handler of this.handlers.get(frame.type) ?? []) handler(frame.payload)
      } catch (error) {
        console.error('invalid event frame', error)
      }
    }
    this.source = source
  }
}
