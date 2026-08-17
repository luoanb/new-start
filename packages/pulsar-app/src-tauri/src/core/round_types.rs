//! 单轮对话的数据契约：会话种子 / 运行态 / 产物。
//!
//! 原定义于 `call_service.rs`；取消 NeuronCallService 后迁入本文件，作为内外握手的显式数据边界。
//! 约定：本文件只含类型，不含逻辑（读写会话元数据在 `conversation_runner.rs`，消息映射在
//! `model_call_input.rs`，选型决策在 `round_resolver.rs`，执行在 `round_executor.rs`）。
//!
//! 真相源约定：管道内全程 `Vec<Message>`（`MessageBody` 带 kind，自描述）；落库原样增量落，
//! 发送前由 `ModelCallInput::from_message` 投影为 `ModelMessage`。不存在中间层
//! `ResolvedRound` / `WireRound`（v2 方案已删除）。

use serde::{Deserialize, Serialize};

use super::models::{ChatModelSelection, ToolCall};

/// 会话级运行态（`conversation.extra.session.state`）：仅保留选型锚点 + 会话级模型选择。
///
/// 已废弃（由消息盖章推导替代）：`last_intervention_at` / `intervention_neuron_ids`
/// 曾在会话态滚动累积"干预窗口"；现改为每条 assistant 产物落库盖章选中神经元
/// （`Message.neuron_id`），评分区间由 `interval_neuron_ids` 按消息介入边界推导。
///
/// 已删除（v2 方案）：B2 冻结字段 `stable_system_prompt` / `stable_system_frozen`——
/// 首轮 System 落库后历史自带稳定角色，无需跨轮状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub last_selected_neuron_id: Option<String>,
    /// 会话级模型选择（`provider_id + model_id`）；`None` = 未指定，回退全局默认。
    /// 由用户改选写入，随会话持久化；前端切换会话回显本值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ChatModelSelection>,
}

/// 会话种子：决定首轮选型起点与推进规则。
///
/// - `Global`：全域首轮选 1 → 写 `state.last_selected`；后续按领域推进。
/// - `Neuron(id)`：系统神经元用 behavior（禁 Global，宽容回退 Neighborhood）；
///   普通神经元推导默认领域行为。
/// - `None`（缺省）：直连，不选型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum SessionSeed {
    Global,
    Neuron(String),
}

/// ③ 单轮产物：仅模型侧结果；落库由上层（ConversationRunner）负责。
#[derive(Debug, Clone)]
pub struct RoundOutcome {
    /// 最终文本（含工具结果拼接），返回给用户。
    pub response: String,
    /// 模型原始输出（tool_call 消息落库用）。
    pub model_output: Option<String>,
    /// 模型本轮声明的工具调用（全部声明，落库 tool_call 消息用）。
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 本轮全部工具执行结果（一轮内多个 tool_calls 全部执行）。
    pub tool_results: Vec<ToolResultItem>,
    /// 本轮选中神经元 id（产物落库盖章；未选中为 None）。
    pub selected_neuron_id: Option<String>,
}

/// 单条工具执行结果：与 `ToolCall.id` 配对，落库为一条 Tool 消息。
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultItem {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_seed_serde_roundtrip() {
        for seed in [SessionSeed::Global, SessionSeed::Neuron("n-1".into())] {
            let value = serde_json::to_value(&seed).unwrap();
            let back: SessionSeed = serde_json::from_value(value).unwrap();
            assert_eq!(back, seed);
        }
    }
}
