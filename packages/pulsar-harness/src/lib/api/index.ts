/**
 * API 客户端入口：单一 IPC 通道（`tauriClient`）。
 *
 * 骨架内不保留远程通道与连接模式切换，`api` 为常量实例；业务包如需多通道，
 * 在此按同样方式替换即可（业务层只依赖 `ApiClient` 接口）。
 */
import { tauriClient } from "./tauriClient";
import type { ApiClient } from "./types";

/**
 * 当前 API 客户端实例（单一 IPC 实现）。
 *
 * 运行环境判定收敛在 `tauriClient` 内部：非 Tauri 环境（浏览器预览）自动降级为
 * no-op，调用方无需自行判断环境。
 */
export const api: ApiClient = tauriClient;

export { STATE_CHANGED_EVENT } from "./types";
export type {
  ApiClient,
  LogEntry,
  LogLevel,
  LogListResult,
  PhaseInfo,
  StateChangePayload,
  StateEventKind,
} from "./types";
export { c, def, type Contract } from "./contracts";
