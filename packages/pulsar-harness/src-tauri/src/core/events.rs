//! 前端状态变更事件（统一推送通道）。
//!
//! 后端任何会改变前端可见数据的写操作完成后，通过 `StateEmitter` 广播一个
//! `StateChange`；前端 `dataStore` 监听 `STATE_CHANGED_EVENT` 并按 `kind`
//! 重新拉取对应数据，避免轮询。
//!
//! 本包为功能无关的应用骨架，只保留通道机制与一个基建变体；具体业务数据域
//! （课题 / 会话 / 神经元 / 服务商 / 工作区 / Git 等）应在其业务包内扩展
//! `StateChange` 变体，机制（通道名、发射器类型）原样复用。

use std::sync::Arc;

use serde::Serialize;

/// 前端监听的后端状态变更事件名。
pub const STATE_CHANGED_EVENT: &str = "app://state-changed";

/// 状态变更载荷：`kind` 区分数据域，避免事件爆炸。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StateChange {
    /// 应用基建就绪（`setup` 完成、窗口已建立），前端可据此停止 loading 遮罩。
    AppReady,
}

/// 状态事件发射器：由 `lib.rs` setup 构造（捕获 AppHandle），
/// 注入到 Tauri managed state。
pub type StateEmitter = Arc<dyn Fn(StateChange) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_ready_serializes_with_kind() {
        let json = serde_json::to_string(&StateChange::AppReady).unwrap();
        assert_eq!(json, r#"{"kind":"app_ready"}"#);
    }
}
