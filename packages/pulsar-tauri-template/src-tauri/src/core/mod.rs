//! Core：与功能无关的核心基建（错误类型、状态事件通道、日志阶段注册表）。

pub mod error;
pub mod events;
pub mod log_phase;

pub use error::{AppError, AppErrorPayload, AppResult};
pub use events::{StateChange, StateEmitter, STATE_CHANGED_EVENT};
