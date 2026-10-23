// Runes（$state）在 .svelte.ts 中由 Svelte 编译器直接转换，无需 import。
import { api } from "$lib/api";
import type { StateChangePayload, StateEventKind } from "$lib/api";

/**
 * 后端状态变更事件的最小消费端。
 *
 * 骨架内只暴露「应用基建就绪」一件事：业务包在此按 `kind` 分发到各自的领域数据拉取，
 * 组件从 `dataStore` 读取派生状态。事件订阅幂等（重复 bootstrap 只挂一次监听）。
 */
let ready = $state(false);
let lastKind = $state<StateEventKind | null>(null);
let unlisten: (() => void) | null = null;

export const dataStore = {
  /** 后端 `AppReady` 事件是否已到达。 */
  get ready(): boolean {
    return ready;
  },
  /** 最近一次收到的状态变更 kind（无事件时为 null）。 */
  get lastKind(): StateEventKind | null {
    return lastKind;
  },

  /** 订阅状态变更事件（幂等：已订阅时直接返回）。 */
  bootstrap(): void {
    if (unlisten) return;
    unlisten = api.subscribe((payload: StateChangePayload) => {
      lastKind = payload.kind;
      if (payload.kind === "app_ready") ready = true;
    });
  },

  /** 退订并复位（热更新 / 卸载时调用）。 */
  dispose(): void {
    unlisten?.();
    unlisten = null;
    ready = false;
    lastKind = null;
  },
};
