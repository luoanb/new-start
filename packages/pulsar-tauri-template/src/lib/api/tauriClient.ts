/**
 * API 客户端：Tauri IPC 单通道（invoke + listen）。
 *
 * 本包为功能无关的应用骨架，只有本机 IPC 一种实现；远程 HTTP 通道与连接模式
 * 切换属可选能力，不在此包。
 *
 * 运行环境判定收敛在本模块内部：非 Tauri 环境（浏览器预览）下 `call` / `invoke`
 * 以可达性错误 reject、`subscribe` 返回 no-op 退订函数，调用方无需自行判断环境。
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Contract } from "./contracts";
import { STATE_CHANGED_EVENT, type ApiClient, type StateChangePayload } from "./types";

/** 运行时是否具备 Tauri IPC（整个生命周期内不变）。 */
const inTauri = isTauri();

/** 非可用运行时下调用的统一错误：让调用方可以 lint 到「预览环境无后端」而非 undefined 崩溃。 */
function unavailable(cmd: string): Error {
  return new Error(`Tauri IPC unavailable in this runtime (command: ${cmd})`);
}

async function invokeCommand<T>(cmd: string, params?: Record<string, unknown>): Promise<T> {
  if (!inTauri) throw unavailable(cmd);
  return invoke<T>(cmd, params);
}

export const tauriClient: ApiClient = {
  invoke: invokeCommand,

  async call<P, R>(contract: Contract<P, R>, params: P): Promise<R> {
    return invokeCommand<R>(contract.cmd, params as Record<string, unknown> | undefined);
  },

  subscribe(handler: (payload: StateChangePayload) => void): () => void {
    if (!inTauri) return () => {};
    let unlisten: (() => void) | null = null;
    void listen<StateChangePayload>(STATE_CHANGED_EVENT, (event) => {
      handler(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  },

  async health(): Promise<boolean> {
    return inTauri;
  },
};
