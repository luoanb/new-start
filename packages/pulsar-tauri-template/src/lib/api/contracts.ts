/**
 * 命令契约表：后端命令的唯一真源（类型安全收束）。
 *
 * 业务层禁止再裸写 `api.invoke("cmd", params)` 魔法字符串，一律
 * `api.call(c.someCommand, params)` —— 参数与返回类型由契约编译期校验，
 * 后端改命令时只需改这一处。字段名保持与后端 Tauri command 参数一致
 * （snake_case 为后端参数名，如 `level`）。
 *
 * 沿用机制：新增命令 = 在此加一条 `def<Params, Result>("cmd_name")`；
 * 只声明前端会调用的命令。
 */
import type { LogListResult, PhaseInfo } from "./types";

/** 命令契约：P = 参数类型，R = 返回类型（均为编译期 phantom，运行时只携带命令名）。 */
export interface Contract<P, R> {
  /** 后端命令名（Tauri IPC 命令名）。 */
  readonly cmd: string;
  readonly __p?: P;
  readonly __r?: R;
}

/** 声明命令契约（零运行时开销）。 */
export function def<P, R>(cmd: string): Contract<P, R> {
  return { cmd } as unknown as Contract<P, R>;
}

/** 全部后端命令契约（本包命令集与 `invoke_handler` 一一对应）。 */
export const c = {
  // ── 日志 ──
  /** 日志列表：条目 + 当前级别 + 日志目录（后端 `logs_list` 三合一提返回）。 */
  logsList: def<undefined, LogListResult>("logs_list"),
  /** 日志 phase 注册清单（前端筛选器据 `value` + `group` 构建下拉）。 */
  logsPhases: def<undefined, PhaseInfo[]>("logs_phases"),
  /** 清空内存日志缓冲。 */
  logsClear: def<undefined, void>("logs_clear"),
  /** 设置日志级别，返回生效后的级别。 */
  logsSetLevel: def<{ level: string }, string>("logs_set_level"),

  // ── 示例（演示新增命令的接线方式）──
  /** 诊断：读取后端数据根路径（`debug_storage_path`）。 */
  debugStoragePath: def<undefined, string>("debug_storage_path"),
};
