//! 核心默认策略：按轮次模式（[`RoundMode`]）与触发类型（[`RoundTriggerKind`]）推导
//! 单轮的工具授权与思考配置默认值（架构设计 §3.2）。
//!
//! 模型选型不在此处：模型来自会话运行态（`SessionState.model`，落库于
//! `extra.session.state.model`），由 [`super::round_service::ConversationRunner`] 的
//! `load_context` 读取；本模块只决定「授权白名单来源」与「思考默认开关」。

use super::models::ThinkingConfig;
use super::round_contract::{RoundMode, ToolCatalog};
use super::round_service::RoundTriggerKind;

/// 工具授权默认策略（按 [`RoundMode`]）：
/// - `Agent` → 目录全量（`catalog.list_definitions()` 的 name 集）；
/// - `Chat` / `Assistant` / `Poller` → `None`（执行面回退选中神经元的 `tool_ids`）。
///
/// 模式标签并入（`ConversationMode::tool_tags()`）不在此处，仍在执行面按会话模式并入。
pub fn default_tool_override(
    mode: RoundMode,
    catalog: &dyn ToolCatalog,
) -> Option<Vec<String>> {
    match mode {
        RoundMode::Agent => Some(
            catalog
                .list_definitions()
                .into_iter()
                .map(|definition| definition.name)
                .collect(),
        ),
        RoundMode::Chat | RoundMode::Assistant | RoundMode::Poller => None,
    }
}

/// 思考配置默认策略（按触发，不按模式；逐位保持改造前的应用层传值）：
/// - `User` → `None`（跟随会话 / 模型配置）；
/// - `AgentLoop` → `None`（Agent 续轮沿用用户对话的思考配置，改造前传 `None`）；
/// - `ManualStep` / `Poller` → 显式关闭深度思考（非对话调用，改造前传 `Some(disabled)`）。
pub fn default_thinking(trigger: RoundTriggerKind) -> Option<ThinkingConfig> {
    match trigger {
        RoundTriggerKind::User | RoundTriggerKind::AgentLoop => None,
        RoundTriggerKind::ManualStep | RoundTriggerKind::Poller => Some(ThinkingConfig {
            enabled: Some(false),
            effort: None,
        }),
    }
}
