/**
 * 远程模式 API 客户端：HTTP + SSE。
 *
 * 后端 HTTP API 统一挂 `/api` 前缀（见后端 `net::router`）：
 * - invoke：POST {base}/api/rpc，载荷 `{ cmd, params }`（与后端 `net::rpc` 对齐）。
 * - subscribe：原生 EventSource 连接 {base}/api/events，事件名 = STATE_CHANGED_EVENT；
 *   EventSource 无法自定义请求头，token 经 query 参数 `?token=` 传递（后端 auth 中间件同时接受 header / query）。
 * - health：GET {base}/api/healthz。
 *
 * 超时：fetch 无默认超时，本期失败即抛错，不做流式进度。
 */
import type { Contract } from "./contracts";
import { RpcError, STATE_CHANGED_EVENT, type ApiClient, type ConnConfig, type ServerInfo, type StateChangePayload } from "./types";

interface RpcResponse {
  ok: boolean;
  data?: unknown;
  error?: { code?: string; message?: string };
}

export function httpClient(baseUrl: string, token?: string): ApiClient {
  const base = baseUrl.replace(/\/+$/, "");
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers["Authorization"] = `Bearer ${token}`;

  return {
    async invoke<T>(cmd: string, params?: Record<string, unknown>): Promise<T> {
      let res: Response;
      try {
        res = await fetch(`${base}/api/rpc`, {
          method: "POST",
          headers,
          body: JSON.stringify({ cmd, params: params ?? {} }),
        });
      } catch (e) {
        throw new RpcError("network_error", `RPC ${cmd} 请求失败: ${e}`);
      }
      if (!res.ok) {
        throw new RpcError("http_error", `RPC ${cmd} 失败: HTTP ${res.status}`);
      }
      const body = (await res.json()) as RpcResponse;
      if (!body.ok) {
        throw new RpcError(
          body.error?.code ?? "rpc_error",
          body.error?.message ?? `RPC ${cmd} 失败`,
        );
      }
      return body.data as T;
    },

    async call<P, R>(contract: Contract<P, R>, params: P): Promise<R> {
      return this.invoke<R>(contract.cmd, params as Record<string, unknown>);
    },

    subscribe(handler: (payload: StateChangePayload) => void): () => void {
      const url = token
        ? `${base}/api/events?token=${encodeURIComponent(token)}`
        : `${base}/api/events`;
      const es = new EventSource(url);
      const onEvent = (event: MessageEvent) => {
        try {
          handler(JSON.parse(event.data) as StateChangePayload);
        } catch {
          // 忽略无法解析的事件帧（如健康探测期垃圾帧）。
        }
      };
      es.addEventListener(STATE_CHANGED_EVENT, onEvent);
      return () => es.close();
    },

    async health(): Promise<boolean> {
      try {
        const res = await fetch(`${base}/api/healthz`);
        return res.ok;
      } catch {
        return false;
      }
    },

    async serverInfo(): Promise<ServerInfo> {
      const res = await fetch(`${base}/api/config`);
      if (!res.ok) throw new RpcError("http_error", `GET /api/config 失败: HTTP ${res.status}`);
      return (await res.json()) as ServerInfo;
    },
  };
}

/** 供 index.ts 使用的工厂签名（统一 ConnConfig 入口，避免 url/token 两个裸参数）。 */
export function createHttpClient(cfg: ConnConfig): ApiClient {
  return httpClient(cfg.url ?? "", cfg.token);
}
