//! Hook 收拢层：注入点契约（`defs`）+ 裁决实例（`instances`）+ 注册表（`registry`）
//! + 共享类型（`judgement`）+ 账本（`store`）。
//!
//! - `defs`：注入点即类型——`InjectPointId` 规格卡 + `HookDef`（注册单元）+ `HookRegistry`。
//!   所有非核心流程调度（选型 / 课题 / 判定 / 打分 / 压缩）均以 `HookRegistry` 注入，由
//!   上层装配期注册，runner 只负责在核心 5 步的注入点分发。
//! - `instances`：**一个 hook 一个文件**，内聚「常量 + schema + fallback + INSTANCE + run」。
//! - `registry`：`HookInstance` + `ACTIVE_HOOKS`（启用）+ `LEGACY_HOOKS`（休眠，回切即移入
//!   启用清单）；编排层按注入点遍历 `ACTIVE_HOOKS` 执行。
//! - `judgement`：裁决共享类型（`HookDef` / `JudgementStatus` / `JudgementOutcome`）+
//!   `hook_def()` / `hook_defs_meta()` 查询（只查启用清单）。
//! - `store`：`hook_judgements` 账本表（裁决调用全量记录）。
//!
//! 命名约定：`defs::HookDef`（注册单元）与 `judgement::HookDef`（裁决规则表）同名不同物，
//! 前者按 `hook::defs::HookDef` 引用，后者 re-export 到 `hook::` 顶层（既有消费者不变）。

pub mod compaction;
pub mod defs;
pub mod instances;
pub mod judgement;
/// `pub(crate)`：`HookRun` 签名引用 crate 内部类型 `AssistantHooks`，不对外泄露。
pub(crate) mod registry;
pub mod store;

pub use defs::{HookHandler, HookRegistry, InjectPointId, RegisterError, UnregisterError};
pub use judgement::{
    hook_def, hook_defs_meta, AttemptRecord, HookDef, HookDefMeta, JudgementAnchor,
    JudgementOutcome, JudgementStatus,
};
pub(crate) use registry::{active_hooks_at, HookRun};
pub use store::{
    new_hook_judgement_id, HookJudgementFilter, HookJudgementRecord, HookJudgementStore,
};
