//! 前端状态变更事件（统一推送通道）。
//!
//! 后端任何会改变前端可见数据的写操作（Topic / Conversation / Poller）
//! 完成后，通过 `StateEmitter` 广播一个 `StateChange`；前端 `dataStore`
//! 监听 `STATE_CHANGED_EVENT` 并按 `kind` 重新拉取对应数据，避免轮询。

use std::sync::Arc;

use serde::Serialize;

use super::poller::PollerStatus;

/// 前端监听的后端状态变更事件名。
pub const STATE_CHANGED_EVENT: &str = "app://state-changed";

/// 状态变更载荷：`kind` 区分数据域，避免事件爆炸。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StateChange {
    /// 课题列表/详情变化，前端应重新拉取 topics。
    Topics,
    /// 会话/对话列表变化，前端应重新拉取 conversations。
    Conversations,
    /// 轮询状态变化，直接携带最新 PollerStatus。
    Poller { status: PollerStatus },
    /// 运行中会话集合变化（register/unregister/update_step/close），
    /// 前端应重新拉取 running sessions。
    Sessions,
    /// 神经元权重/连接变化（人工评价、人工调整），前端应刷新神经元面板。
    Neurons,
}

/// 状态事件发射器：由 `lib.rs` setup 构造（捕获 AppHandle），
/// 注入到 Tauri managed state 与后台 poller runtime。
pub type StateEmitter = Arc<dyn Fn(StateChange) + Send + Sync>;
