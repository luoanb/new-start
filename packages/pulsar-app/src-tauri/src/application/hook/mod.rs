//! Hook 业务：裁决定义与装配入口 + 账本 + 压缩装配。
//! 插槽协议（定义 / 注册 / 开启）在 `core::hook::defs`；裁决直接注册进核心 `HookRegistry`，
//! 无独立注册表、无壳 hook 二次分发。

pub mod compaction;
pub mod instances;
pub mod judgement;
pub mod store;

pub use judgement::{
    hook_defs_meta, judgement_spec, AttemptRecord, HookDefMeta, JudgementAnchor, JudgementOutcome,
    JudgementSpec, JudgementStatus,
};
pub use store::{
    new_hook_judgement_id, HookJudgementFilter, HookJudgementRecord, HookJudgementStore,
};
