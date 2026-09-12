//! Hook 业务：注册表与 IP-1～IP-5 实例。
//! 与 `AssistantHooks`（application）同层，`HookRun` 可直接引用业务上下文，无跨层反向边；
//! 插槽协议在 `core::hook::defs`。

pub mod compaction;
pub mod instances;
pub mod judgement;
pub(crate) mod registry;
pub mod store;

pub use judgement::{
    hook_def, hook_defs_meta, AttemptRecord, HookDef, HookDefMeta, JudgementAnchor,
    JudgementOutcome, JudgementStatus,
};
pub(crate) use registry::{active_hooks_at, HookRun};
pub use store::{
    new_hook_judgement_id, HookJudgementFilter, HookJudgementRecord, HookJudgementStore,
};
