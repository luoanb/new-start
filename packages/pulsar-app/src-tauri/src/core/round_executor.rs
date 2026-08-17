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
    /// 组合执行：`call_model` + `execute_tools`（供无中间落库需求的调用方使用）。
    pub async fn execute(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
    ) -> AppResult<RoundOutcome> {
        let (model_response, _authorized_tool_ids) = self
            .call_model(neuron, messages, model, tool_override, tool_tags)
            .await?;
        self.execute_tools(model_response, neuron.map(|n| n.id.clone()))
            .await
    }

    /// 第一步：工具授权 → 投影 → 模型调用 → 授权校验。
    ///
    /// 返回模型响应与授权工具 id。调用方可在两步之间落库「模型声明」（独立落库：
    /// 声明先于工具执行持久化，工具失败/超时也不丢模型曾调用的记录）。
    pub async fn call_model(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
    ) -> AppResult<(ModelCallResponse, Vec<String>)> {
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
        // 排查辅助：打印最终投给模型的完整消息（role + 内容，单条截断 3000 字符防日志爆炸）。
        let wire_view: Vec<String> = model_messages
            .iter()
            .map(|m| {
                format!(
                    "[{:?}] {}",
                    m.role,
                    m.content.chars().take(3000).collect::<String>()
                )
            })
            .collect();
        tracing::info!(
            phase = "round_execute",
            message_count = model_messages.len(),
            messages = ?wire_view,
            "model input (final messages)"
        );
        let model_response = self
            .model_caller
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: model_messages,
                tools,
                params: model.params.clone(),
                thinking: model.thinking.clone(),
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
        // 授权校验：模型声明的工具必须在本轮授权集合内（未授权属于契约异常，直接失败；
        // 此时声明尚未落库，不会产生孤儿记录）。
        if let Some(calls) = model_response.tool_calls.as_ref() {
            for call in calls {
                if !authorized_tool_ids.iter().any(|id| id == &call.name) {
                    return Err(AppError::InvalidInput(format!(
                        "Tool '{}' is not authorized for this round",
                        call.name
                    )));
                }
            }
        }
        Ok((model_response, authorized_tool_ids))
    }

    /// 第二步：执行全部 tool_calls + 响应拼接。工具执行失败不冒泡——
    /// 失败信息作为 Tool 结果回传模型（见下方 match），保证声明与结果成对/独立落库。
    pub async fn execute_tools(
        &self,
        model_response: ModelCallResponse,
        neuron_id: Option<String>,
    ) -> AppResult<RoundOutcome> {
        let mut output = model_response.output.clone();
        let mut tool_results: Vec<ToolResultItem> = Vec::new();
        // 单轮单次工具阶段：模型可能一次声明多个 tool_calls（并行调用），引擎全部执行。
        // 每个声明都会产生一条结果（成功或失败文本），供独立落库；孤儿的排除统一在
        // 「消息 → 模型入参」投影时由 sanitize_tool_pairs 过滤（见 project_history）。
        let tool_calls = model_response.tool_calls.clone();
        if let Some(calls) = tool_calls.as_ref() {
            for call in calls {
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
                let result = match tool.execute(call.arguments.clone()).await {
                    Ok(result) => result,
                    Err(error) => {
                        // 工具失败不阻塞整轮：把失败信息作为工具结果回传给模型，
                        // 由模型决定重试、换工具或直接基于失败继续作答。
                        let message =
                            format!("[tool:{}] 工具调用失败：{error}", call.name);
                        tracing::warn!(
                            phase = "round_execute",
                            tool = %call.name,
                            error = %error,
                            "tool failed; error passed back to model"
                        );
                        message
                    }
                };
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
            selected_neuron_id: neuron_id,
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
