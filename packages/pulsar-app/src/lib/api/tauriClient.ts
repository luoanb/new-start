/**
 * 本机模式 API 客户端：Tauri IPC（invoke + listen）。
 *
 * 与改造前 dataStore 直接调用 `invoke` / `listen` 的行为逐字节一致，
 * 本机模式回归风险为零。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { STATE_CHANGED_EVENT, type ApiClient, type ServerInfo, type StateChangePayload } from "./types";

export const tauriClient: ApiClient = {
  async invoke<T>(cmd: string, params?: Record<string, unknown>): Promise<T> {
    return invoke<T>(cmd, params);
  },

  subscribe(handler: (payload: StateChangePayload) => void): () => void {
    let unlisten: (() => void) | null = null;
    void listen<StateChangePayload>(STATE_CHANGED_EVENT, (event) => {
      handler(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  },

  async health(): Promise<boolean> {
    return true;
  },

  async serverInfo(): Promise<ServerInfo> {
    return invoke<ServerInfo>("server_info");
  },
};
