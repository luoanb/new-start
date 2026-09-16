//! 通用基础设施：与业务语义无关的底层能力（时钟、配置读写等）。
//!
//! 依赖规则：`infra/` 只依赖 `core/` 的稳定类型（`error` 等）与标准库，
//! 不引用 `application/`，也不引用 `providers/`、`tools/`、`stores/`、`policies/`、`sinks/`；
//! 核心与各扩展目录均可引用本目录。

pub mod config;
pub mod platform;
pub mod time;
