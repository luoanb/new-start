//! ③ 执行：模型调用 + 工具授权 + 单轮工具执行 → [`RoundOutcome`]。
//!
//! 原 `NeuronCallService::converse` 的执行段迁入。不选型、不拼接（wire = `Vec<Message>` 由
//! resolve + runner 追加输入后传入），发送前统一投影 `ModelMessage`；不落库、不感知会话与
//! 业务触发语义。

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use super::{
    error::{AppError, AppResult},
    model_call_input::ModelCallInput,
    models::{ChatModelSelection, Message, ModelCallRequest, ModelCallResponse, Neuron, ToolTag},
    round_types::{RoundOutcome, ToolResultItem},
    tool_registry::ToolRegistry,
};

/// 模型调用抽象：生产用 [`super::providers::ProviderRegistry`]，测试可注入替身。
#[async_trait]
pub trait ModelCaller: Send + Sync {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse>;
}

#[async_trait]
impl ModelCaller for super::providers::ProviderRegistry {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        super::providers::ProviderRegistry::call_model(self, request).await
    }
}

/// 执行面：模型调用 + 工具授权（override 优先 → behavior 三策略 → 标签并入，∩ 注册表）+
/// 单轮全部 tool_calls 执行 + 响应拼接。不持有组装/选型知识。
pub struct RoundExecutor {
    model_caller: Arc<dyn ModelCaller>,
    tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl std::fmt::Debug for RoundExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoundExecutor").finish_non_exhaustive()
    }
}

impl RoundExecutor {
    pub fn new(
        model_caller: Arc<dyn ModelCaller>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        Self {
            model_caller,
            tool_registry,
        }
    }

    /// 单轮执行：工具授权 → 模型调用（发送前投影 ModelMessage）→ 授权校验 → 全部 tool_calls 执行。
    ///
    /// 工具授权（按会话模式，落点在本函数）：`tool_override` 优先（Agent 传注册表全部）；
    /// `None` 时取选中神经元 `neuron.tool_ids`（Assistant/System；Chat 无神经元 → 空）；
    /// `tool_tags` 并入（`ConversationMode::tool_tags()`），∩ 注册表。数据驱动——调用方按模式
    /// 算好 override 与标签，本函数不感知模式。
    pub async fn execute(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
    ) -> AppResult<RoundOutcome> {
        // 工具授权：override 优先；否则取选中神经元的 tool_ids（∩ 注册表）。
        let tool_ids = match tool_override {
            Some(ids) => ids,
            None => neuron.map(|n| n.tool_ids.clone()).unwrap_or_default(),
        };
        // 块作用域持有读锁：保证跨 await 前释放（RwLockReadGuard 非 Send）。
        // 标签并入：数据驱动——调用方按模式算好（ConversationMode::tool_tags），service 不感知模式；
        // 空 tool_tags = 不注入（Chat 对话、内部裁决），完全沿用 override/behavior。
        let (authorized_tool_ids, tools) = {
            let guard = self
                .tool_registry
                .read()
                .expect("tool registry lock should not be poisoned");
            let mut final_ids = Vec::new();
            for tag in &tool_tags {
                final_ids.extend(guard.tools_with_tag(*tag));
            }
            let authorized_tool_ids = filter_authorized_tool_ids(&guard, &tool_ids);
            // 去重保序（工具数少，O(n²) 可接受）：Core/System 在前，策略工具随后。
            for id in authorized_tool_ids {
                if !final_ids.contains(&id) {
                    final_ids.push(id);
                }
            }
            let tools = if final_ids.is_empty() {
                None
            } else {
                Some(guard.definitions_for(&final_ids))
            };
            (final_ids, tools)
        };
        tracing::info!(
            phase = "round_execute",
            authorized_tool_count = authorized_tool_ids.len(),
            wire_tool_ids = ?tools.as_ref().map(|t| t.iter().map(|d| d.name.clone()).collect::<Vec<_>>()),
            "tools authorized"
        );

        // 发送前统一投影：Message（落库真相源）→ ModelMessage（模型层），与选型共用 project_history。
        let model_messages = ModelCallInput::project_history(messages);
        let model_response = self
            .model_caller
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: model_messages,
                tools,
            })
            .await?;
        tracing::info!(
            phase = "round_execute",
            provider = %model.provider_id,
            model = %model.model_id,
            output_len = model_response.output.len(),
            tool_calls = model_response.tool_calls.as_ref().map_or(0, |c| c.len()),
            "model call done"
        );

        let mut output = model_response.output.clone();
        let mut tool_results: Vec<ToolResultItem> = Vec::new();
        // 单轮单次工具阶段：模型可能一次声明多个 tool_calls（并行调用），引擎全部执行。
        // 产物携带全部声明与全部结果；落库后 assistant 声明与 tool 结果一一配对
        // （sanitize 要求每个声明都有对应结果，否则未应答声明会被降级、tool 消息成孤儿）。
        let tool_calls = model_response.tool_calls.clone();
        if let Some(calls) = tool_calls.as_ref() {
            for call in calls {
                if !authorized_tool_ids.iter().any(|id| id == &call.name) {
                    return Err(AppError::InvalidInput(format!(
                        "Tool '{}' is not authorized for this round",
                        call.name
                    )));
                }
                let tool = self
                    .tool_registry
                    .read()
                    .expect("tool registry lock should not be poisoned")
                    .get_tool(&call.name)
                    .ok_or_else(|| AppError::SkillNotFound(call.name.clone()))?;
                tracing::info!(
                    phase = "round_execute",
                    tool = %call.name,
                    args_len = call.arguments.to_string().len(),
                    "executing tool"
                );
                let result = tool.execute(call.arguments.clone()).await?;
                tracing::info!(
                    phase = "round_execute",
                    tool = %call.name,
                    result_len = result.len(),
                    "tool executed"
                );
                tool_results.push(ToolResultItem {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    content: result.clone(),
                });
                output = if output.trim().is_empty() {
                    result
                } else {
                    format!("{output}\n\n[tool:{}] {result}", call.name)
                };
            }
        }

        Ok(RoundOutcome {
            response: output,
            model_output: Some(model_response.output.clone()),
            tool_calls,
            tool_results,
            selected_neuron_id: neuron.map(|n| n.id.clone()),
        })
    }
}

/// 工具白名单 ∩ 注册表：仅授权真实存在的工具。
pub fn filter_authorized_tool_ids(registry: &ToolRegistry, tool_ids: &[String]) -> Vec<String> {
    let known: HashSet<String> = registry
        .list_definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let mut out = Vec::new();
    for id in tool_ids {
        if known.contains(id) {
            out.push(id.clone());
        } else {
            tracing::warn!(
                phase = "tool_authorization",
                tool_id = %id,
                "ignoring unknown tool id"
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::core::{models::ToolSource, tool_registry::Tool};

    /// 测试工具：仅用于注册表存在性校验。
    struct DummyTool(&'static str);

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "dummy"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object","properties":{}})
        }
        async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn filter_drops_unknown_tool_ids() {
        let mut registry = ToolRegistry::new();
        registry.register_source(DummyTool("echo"), ToolSource::Config);
        let out = filter_authorized_tool_ids(&registry, &["echo".into(), "nope".into()]);
        assert_eq!(out, vec!["echo".to_string()]);
    }
}
