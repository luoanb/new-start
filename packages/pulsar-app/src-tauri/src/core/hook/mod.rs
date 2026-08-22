//! Hook 收拢层：注入点契约（`defs`）+ 裁决 hook 静态清单（`judgement`）+ 账本（`store`）。
//!
//! - `defs`：注入点即类型——`InjectPointId` 规格卡 + `HookDef`（注册单元）+ `HookRegistry`。
//!   所有非核心流程调度（选型 / 课题 / 判定 / 打分 / 压缩）均以 `HookRegistry` 注入，由
//!   上层装配期注册，runner 只负责在核心 5 步的注入点分发。
//! - `judgement`：既有裁决 hook 静态清单（`HookDef` 规则表 + `call_judgement` 相关契约）。
//! - `store`：`hook_judgements` 账本表（裁决调用全量记录）。
//!
//! 命名约定：`defs::HookDef`（注册单元）与 `judgement::HookDef`（裁决规则表）同名不同物，
//! 前者按 `hook::defs::HookDef` 引用，后者 re-export 到 `hook::` 顶层（既有消费者不变）。

pub mod compaction;
pub mod defs;
pub mod judgement;
pub mod store;

pub use defs::{HookHandler, HookRegistry, InjectPointId, RegisterError, UnregisterError};
pub use judgement::{
    hook_def, hook_defs_meta, AttemptRecord, HookDef, HookDefMeta, HOOK_DEFS, JudgementAnchor,
    JudgementOutcome, JudgementStatus,
};
pub use store::{
    new_hook_judgement_id, HookJudgementFilter, HookJudgementRecord, HookJudgementStore,
};
