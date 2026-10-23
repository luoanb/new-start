/**
 * 统一 API 客户端抽象与共享类型。
 *
 * 本包为功能无关的应用骨架：只有单一 IPC 通道（Tauri invoke + app://state-changed 事件），
 * 不保留远程 HTTP 通道与连接模式切换（那属可选能力）。
 *
 * 业务层（dataStore / 组件）只依赖本接口 + 命令契约（contracts.ts），
 * 调用一律走 `call(c.someCommand, params)`，禁止裸 invoke 字符串。
 */
import type { Contract } from "./contracts";

/** 与后端 core/events.rs STATE_CHANGED_EVENT 保持一致（Tauri 事件名）。 */
export const STATE_CHANGED_EVENT = "app://state-changed";

/** 状态变更载荷的 kind 取值（骨架内只有基建事件；业务包按需扩展）。 */
export type StateEventKind = "app_ready";

/** 状态变更载荷：`kind` 区分数据域（与后端 StateChange 的 serde tag 一致）。 */
export type StateChangePayload = { kind: "app_ready" };

/** 后端日志条目（与 `sinks/app_log.rs::LogEntry` 同构）。 */
export interface LogEntry {
  ts_ms: number;
  level: LogLevel;
  target: string;
  message: string;
  fields?: Record<string, string>;
}

/** 日志级别（与 `sinks/app_log.rs::LogLevel` 同构）。 */
export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";

/** 日志 phase 注册信息（与 `core/log_phase.rs::PhaseInfo` 同构）。 */
export interface PhaseInfo {
  value: string;
  group: string;
  label: string;
}

/** `logs_list` 命令返回：条目 + 当前级别 + 日志目录三合一。 */
export interface LogListResult {
  entries: LogEntry[];
  level: string;
  dir: string | null;
}

export interface ApiClient {
  /** 调用后端命令（Tauri invoke）。 */
  invoke<T>(cmd: string, params?: Record<string, unknown>): Promise<T>;
  /**
   * 类型安全命令调用：业务层唯一入口。
   * 参数/返回类型由 `contracts.ts` 契约编译期校验。
   */
  call<P, R>(contract: Contract<P, R>, params: P): Promise<R>;
  /** 订阅状态变更事件，返回退订函数。 */
  subscribe(handler: (payload: StateChangePayload) => void): () => void;
  /** 后端可达性检查：仅 Tauri 运行时为 true，浏览器预览等非 Tauri 环境为 false。 */
  health(): Promise<boolean>;
}
