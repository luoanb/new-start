import type { ClientApi } from './types.ts'

/** RPC 客户端：把宿主的 `/api/rpc` 协议封装成一次 Promise。 */
export class RpcClient implements ClientApi {
  constructor(private readonly endpoint = '/api/rpc') {}

  async call<T = unknown>(method: string, args?: unknown): Promise<T> {
    const response = await fetch(this.endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ method, args }),
    })
    const body = (await response.json()) as { result?: T; error?: string }
    if (!response.ok) throw new Error(body.error ?? `rpc failed: ${method}`)
    return body.result as T
  }
}
