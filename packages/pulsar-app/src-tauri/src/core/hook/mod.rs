//! Hook 插槽协议：注入点规格 + 注册器 + 失败策略（业务实例在 `application/hook/`）。

pub mod defs;

pub use defs::{HookHandler, HookRegistry, InjectPointId, RegisterError};
