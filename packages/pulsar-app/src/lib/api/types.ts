/**
 * 统一 API 客户端抽象（前端双模式收口）。
 *
 * - tauriClient：本机模式，Tauri IPC（invoke + app://state-changed 事件），行为与现状逐字节一致。
 * - httpClient：远程模式，POST /rpc + GET /events(SSE) + GET /healthz，与后端 `net` 模块对齐。
 *
 * 业务层（dataStore / 组件）只依赖本接口，切换模式对业务透明。
 */
import type { PollerStatus } from "$lib/types";

/** 与后端 core/events.rs STATE_CHANGED_EVENT 保持一致（Tauri 事件名 / SSE 事件名共用）。 */
export const STATE_CHANGED_EVENT = "app://state-changed";

export type StateEventKind =
  | "topics"
  | "conversations"
  | "message_delta"
  | "poller"
  | "sessions"
  | "neurons"
  | "providers"
  | "tools"
  | "workspaces";

export type StateChangePayload =
  | { kind: "topics" }
  | { kind: "conversations"; affected?: string[] }
  | {
      kind: "message_delta";
      conversation_id: string;
      /** 该消息在会话消息列表中的索引（流式占位消息）。 */
      message_index: number;
      /** 该消息当前累积正文全文。 */
      content: string;
      /** 该消息当前累积思考全文（空串 = 无思考）。 */
      reasoning: string;
      /** true = 本轮完成，前端收敛为全量重拉。 */
      done: boolean;
    }
  | { kind: "poller"; status: PollerStatus }
  | { kind: "sessions" }
  | { kind: "neurons" }
  | { kind: "providers" }
  | { kind: "tools" }
  | { kind: "workspaces" };

export interface ApiClient {
  /** 调用后端命令：本机走 Tauri invoke，远程走 POST /rpc。 */
  invoke<T>(cmd: string, params?: Record<string, unknown>): Promise<T>;
  /** 订阅状态变更事件，返回退订函数。 */
  subscribe(handler: (payload: StateChangePayload) => void): () => void;
  /** 后端可达性检查（本机恒 true）。 */
  health(): Promise<boolean>;
}

/** 连接配置（存 localStorage：pulsar:connMode / pulsar:remoteUrl / pulsar:remoteToken）。 */
export interface ConnConfig {
  mode: "local" | "remote";
  /** 远程模式必填：http://host:port（如 http://127.0.0.1:8787）。 */
  url?: string;
  /** 远程模式可选：白名单 token；后端白名单为空时可不填。 */
  token?: string;
}

/** RPC 失败错误：与 Tauri AppErrorPayload { code, message } 同构，formatInvokeError 可直接渲染。 */
export class RpcError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = "RpcError";
    this.code = code;
  }
}
