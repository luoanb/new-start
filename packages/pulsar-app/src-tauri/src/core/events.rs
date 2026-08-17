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
    /// 会话/对话列表变化。`affected` 为实际发生写入的会话 id；
    /// 前端仅重拉受影响会话的消息，未受影响会话不重拉、不触发滚动。
    Conversations { affected: Vec<String> },
    /// 轮询状态变化，直接携带最新 PollerStatus。
    Poller { status: PollerStatus },
    /// 运行中会话集合变化（register/unregister/update_step/close），
    /// 前端应重新拉取 running sessions。
    Sessions,
    /// 神经元权重/连接变化（人工评价、人工调整），前端应刷新神经元面板。
    Neurons,
    /// 工具装配进度/结果变化（启动后台装配、刷新、保存配置），
    /// 前端应重新拉取 tools 与 MCP server 状态。
    Tools,
    /// 服务商/模型配置变化（保存服务商配置后广播），前端应重新拉取 providers 与 models。
    Providers,
    /// 工作区集合 / 文件树变化（添加/移除/切换工作区、ignore 编辑、fs 写操作），
    /// 前端应重新拉取工作区列表与文件树。
    Workspaces,
}

/// 状态事件发射器：由 `lib.rs` setup 构造（捕获 AppHandle），
/// 注入到 Tauri managed state 与后台 poller runtime。
pub type StateEmitter = Arc<dyn Fn(StateChange) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_serializes_with_affected() {
        let json = serde_json::to_string(&StateChange::Conversations {
            affected: vec!["s1".into(), "s2".into()],
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"conversations","affected":["s1","s2"]}"#);
    }

    #[test]
    fn empty_affected_serializes_as_empty_array() {
        let json = serde_json::to_string(&StateChange::Conversations { affected: vec![] }).unwrap();
        assert_eq!(json, r#"{"kind":"conversations","affected":[]}"#);
    }
}
