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
    /// 流式增量：assistant 响应分块更新（正文 + 思考）。
    /// `done: false` 前端原地合并不重拉；`done: true` 本轮完成，前端收敛为全量重拉。
    MessageDelta {
        conversation_id: String,
        /// 该消息在会话消息列表中的索引（流式占位消息）。
        message_index: usize,
        /// 该消息当前累积正文全文。
        content: String,
        /// 该消息当前累积思考全文（空串 = 无思考）。
        reasoning: String,
        done: bool,
    },
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
    /// 仓库状态变化（status/stage/commit/reset/push/pull/stash 等写操作后），
    /// 前端应重拉 git 面板（repos/status/log/stash）。
    Git,
    /// git 写操作确认请求：`git_confirm` 处理后事件收敛（无 UI 超时作废）。
    /// `op_kind` 为写操作分类（commit/push/pull/reset/...），供前端分类展示。
    GitConfirm {
        op_id: String,
        op_kind: String,
        title: String,
        detail: serde_json::Value,
    },
    /// 裁决记录两阶段事件（锚点驱动）：开始 = status "pending"（前端就地渲染「裁决中」卡），
    /// 结束 = status 收敛为终态（ok / retried_ok / downgraded）。事件源收敛于
    /// `HookJudgementStore::emit_change`，前端原地收敛，不重拉全量。
    HookJudgements {
        conversation_id: String,
        anchor_message_index: Option<i64>,
        id: String,
        status: String,
    },
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

    #[test]
    fn message_delta_serializes_snake_case() {
        let json = serde_json::to_string(&StateChange::MessageDelta {
            conversation_id: "c1".into(),
            message_index: 3,
            content: "hello".into(),
            reasoning: "think".into(),
            done: false,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"kind":"message_delta","conversation_id":"c1","message_index":3,"content":"hello","reasoning":"think","done":false}"#
        );
    }

    #[test]
    fn hook_judgements_serializes_pending_and_terminal() {
        let pending = serde_json::to_string(&StateChange::HookJudgements {
            conversation_id: "c1".into(),
            anchor_message_index: Some(3),
            id: "hj_1".into(),
            status: "pending".into(),
        })
        .unwrap();
        assert_eq!(
            pending,
            r#"{"kind":"hook_judgements","conversation_id":"c1","anchor_message_index":3,"id":"hj_1","status":"pending"}"#
        );

        let terminal = serde_json::to_string(&StateChange::HookJudgements {
            conversation_id: "c1".into(),
            anchor_message_index: None,
            id: "hj_2".into(),
            status: "retried_ok".into(),
        })
        .unwrap();
        assert_eq!(
            terminal,
            r#"{"kind":"hook_judgements","conversation_id":"c1","anchor_message_index":null,"id":"hj_2","status":"retried_ok"}"#
        );
    }
}
