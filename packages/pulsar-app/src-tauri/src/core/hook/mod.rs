//! Hook 插槽协议：注入点规格 + 注册器 + 失败策略（业务实例在 `application/hook/`）。

pub mod cycle;
pub mod defs;

pub use cycle::{
    gate_check, gate_match, validate_value, CycleFacts, CycleField, CycleParamKind,
    CycleParamSpec, CycleParamUsage, CycleValue, RoundOrigin, RoundRef, RoundShape,
};
pub use defs::{
    HookEntry, HookHandler, HookParamView, HookRegistry, InjectPointId, RegisterError,
};
