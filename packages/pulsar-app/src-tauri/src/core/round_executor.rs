//! ③ 执行：模型调用 + 工具授权 + 单轮工具执行 → [`RoundProduct`]。
//!
//! 原 `NeuronCallService::converse` 的执行段迁入。不选型、不拼接（wire = `Vec<Message>` 由
//! resolve + runner 追加输入后传入），发送前统一投影 `ModelMessage`；不落库、不感知会话与
//! 业务触发语义。

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use super::{
    error::{AppError, AppResult},
    model_call_input::ModelCallInput,
    models::{
        ChatModelSelection, Message, ModelRequest, ModelResponse, Neuron,
        ResponseFormatSpec, StreamChunk, ThinkingConfig, ToolTag,
    },
    round_types::{RoundProduct, ToolResult},
};
use crate::core::log_phase::{PHASE_ROUND_EXECUTE, PHASE_TOOL_AUTHORIZATION};
use crate::core::round_contract::{CapabilityExecutor, ToolCatalog};

/// 模型调用抽象：生产用 [`crate::providers::providers::ProviderRegistry`]，测试可注入替身。
#[async_trait]
pub trait ModelCaller: Send + Sync {
    async fn call_model(&self, request: ModelRequest) -> AppResult<ModelResponse>;

    /// 流式模型调用：on_chunk 每块增量回调（协议层 `StreamChunk`），聚合完成后返回完整响应。
    /// 默认实现回退非流式 `call_model`（测试替身无需实现；生产 ProviderRegistry 覆盖为本实现）。
    async fn call_model_stream(
        &self,
        request: ModelRequest,
        _on_chunk: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> AppResult<ModelResponse> {
        self.call_model(request).await
    }
}

/// 执行面：模型调用 + 工具授权（override 优先 → behavior 三策略 → 标签并入，∩ 注册表）+
/// 单轮全部 tool_calls 执行 + 响应拼接。不持有组装/选型知识。
pub struct RoundExecutor {
    model_caller: Arc<dyn ModelCaller>,
    /// 工具目录（只读端口）：授权决策与 wire 工具声明来源。
    tool_catalog: Arc<dyn ToolCatalog>,
    /// 能力执行器（§3.4）：单调用执行语义唯一归属。
    capability: Arc<dyn CapabilityExecutor>,
}

impl std::fmt::Debug for RoundExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoundExecutor").finish_non_exhaustive()
    }
}

impl RoundExecutor {
    pub fn new(
        model_caller: Arc<dyn ModelCaller>,
        tool_catalog: Arc<dyn ToolCatalog>,
        capability: Arc<dyn CapabilityExecutor>,
    ) -> Self {
        Self {
            model_caller,
            tool_catalog,
            capability,
        }
    }

    /// 工具目录只读端口：授权默认策略（`RoundMode::Agent` → 目录全量）经此查询。
    pub fn tool_catalog(&self) -> &dyn ToolCatalog {
        self.tool_catalog.as_ref()
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
        // 思考配置覆盖：Some = 调用方直接给定（后台调用传 disabled）；None = 跟随模型/会话配置。
        thinking_override: Option<ThinkingConfig>,
        // 结构化输出契约覆盖（裁决 hook 传入；None = 无约束）。
        response_format: Option<ResponseFormatSpec>,
    ) -> AppResult<RoundProduct> {
        let (model_response, _authorized_tool_ids) = self
            .call_model(
                neuron,
                messages,
                model,
                tool_override,
                tool_tags,
                thinking_override,
                response_format,
            )
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
        // 思考配置覆盖：Some = 调用方直接给定（后台调用传 disabled）；None = 跟随模型/会话配置。
        thinking_override: Option<ThinkingConfig>,
        // 结构化输出契约覆盖（裁决 hook 传入；None = 无约束）。
        response_format: Option<ResponseFormatSpec>,
    ) -> AppResult<(ModelResponse, Vec<String>)> {
        let (request, authorized_tool_ids) = self.build_model_call_request(
            neuron,
            messages,
            model,
            tool_override,
            tool_tags,
            thinking_override,
            response_format,
        )?;
        let model_response = self.model_caller.call_model(request).await?;
        tracing::info!(
            phase = PHASE_ROUND_EXECUTE,
            output_len = model_response.output.len(),
            tool_calls = model_response.tool_calls.as_ref().map_or(0, |c| c.len()),
            reasoning = model_response.reasoning.as_deref().map_or(0, str::len),
            "model call done"
        );
        self.validate_authorized(&model_response, &authorized_tool_ids)?;
        Ok((model_response, authorized_tool_ids))
    }

    /// 流式第一步：与 [`Self::call_model`] 相同（工具授权 / 投影 / 授权校验照旧），
    /// 仅模型调用走流式 `call_model_stream`（on_chunk 每块增量回调，由 runner 决定落库/事件）。
    pub async fn call_model_stream(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
        thinking_override: Option<ThinkingConfig>,
        // 结构化输出契约覆盖（裁决 hook 传入；None = 无约束）。
        response_format: Option<ResponseFormatSpec>,
        on_chunk: Box<dyn FnMut(StreamChunk) + Send>,
    ) -> AppResult<(ModelResponse, Vec<String>)> {
        let (request, authorized_tool_ids) = self.build_model_call_request(
            neuron,
            messages,
            model,
            tool_override,
            tool_tags,
            thinking_override,
            response_format,
        )?;
        let model_response = self
            .model_caller
            .call_model_stream(request, on_chunk)
            .await?;
        tracing::info!(
            phase = PHASE_ROUND_EXECUTE,
            output_len = model_response.output.len(),
            tool_calls = model_response.tool_calls.as_ref().map_or(0, |c| c.len()),
            reasoning = model_response.reasoning.as_deref().map_or(0, str::len),
            "model stream call done"
        );
        self.validate_authorized(&model_response, &authorized_tool_ids)?;
        Ok((model_response, authorized_tool_ids))
    }

    /// 工具授权 + 发送前投影 → 构造 `ModelRequest`（`call_model` / `call_model_stream` 共用）。
    fn build_model_call_request(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
        thinking_override: Option<ThinkingConfig>,
        // 结构化输出契约覆盖（裁决 hook 传入；None = 无约束）。
        response_format: Option<ResponseFormatSpec>,
    ) -> AppResult<(ModelRequest, Vec<String>)> {
        // 工具授权：override 优先；否则取选中神经元的 tool_ids（∩ 注册表）。
        let tool_ids = match tool_override {
            Some(ids) => ids,
            None => neuron.map(|n| n.tool_ids.clone()).unwrap_or_default(),
        };
        // 工具目录经只读端口访问：授权决策与 wire 声明不再感知注册表与锁。
        // 标签并入：数据驱动——调用方按模式算好（ConversationMode::tool_tags），service 不感知模式；
        // 空 tool_tags = 不注入（Chat 对话、内部裁决），完全沿用 override/behavior。
        let mut authorized_tool_ids = Vec::new();
        for tag in &tool_tags {
            authorized_tool_ids.extend(self.tool_catalog.tools_with_tag(*tag));
        }
        for id in filter_authorized_tool_ids(self.tool_catalog.as_ref(), &tool_ids) {
            // 去重保序（工具数少，O(n²) 可接受）：Core/System 在前，策略工具随后。
            if !authorized_tool_ids.contains(&id) {
                authorized_tool_ids.push(id);
            }
        }
        let tools = if authorized_tool_ids.is_empty() {
            None
        } else {
            Some(self.tool_catalog.definitions_for(&authorized_tool_ids))
        };
        tracing::info!(
            phase = PHASE_ROUND_EXECUTE,
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
            phase = PHASE_ROUND_EXECUTE,
            message_count = model_messages.len(),
            messages = ?wire_view,
            "model input (final messages)"
        );
        Ok((
            ModelRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: model_messages,
                tools,
                params: model.params.clone(),
                // 调用方给定覆盖优先；None = 用会话/模型自带配置。
                thinking: thinking_override.or_else(|| model.thinking.clone()),
                // 结构化输出契约（裁决调用传入；主对话 None）。
                response_format,
            },
            authorized_tool_ids,
        ))
    }

    /// 授权校验：模型声明的工具必须在本轮授权集合内（未授权属于契约异常，直接失败；
    /// 此时声明尚未落库，不会产生孤儿记录）。
    fn validate_authorized(
        &self,
        model_response: &ModelResponse,
        authorized_tool_ids: &[String],
    ) -> AppResult<()> {
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
        Ok(())
    }

    /// 第二步：执行全部 tool_calls + 响应拼接。工具执行失败不冒泡——
    /// 失败信息作为 Tool 结果回传模型（见下方 match），保证声明与结果成对/独立落库。
    pub async fn execute_tools(
        &self,
        model_response: ModelResponse,
        neuron_id: Option<String>,
    ) -> AppResult<RoundProduct> {
        let mut output = model_response.output.clone();
        let mut tool_results: Vec<ToolResult> = Vec::new();
        // 单轮单次工具阶段：模型可能一次声明多个 tool_calls（并行调用），引擎全部执行。
        // 每个声明都会产生一条结果（成功或失败文本），供独立落库；孤儿的排除统一在
        // 「消息 → 模型入参」投影时由 sanitize_tool_pairs 过滤（见 project_history）。
        let tool_calls = model_response.tool_calls.clone();
        if let Some(calls) = tool_calls.as_ref() {
            for call in calls {
                // 单调用执行语义（授权 / 失败转结果 / 截断）统一在能力执行器：
                // 执行面与 CapabilityExecutor 端口共用同一实现（架构设计 §2.3 / §3.4）。
                let item = self.capability.execute(call.clone()).await?;
                let result = item.content.clone();
                tool_results.push(item);
                output = if output.trim().is_empty() {
                    result
                } else {
                    format!("{output}\n\n[tool:{}] {result}", call.name)
                };
            }
        }

        Ok(RoundProduct {
            response: output,
            model_output: Some(model_response.output.clone()),
            tool_calls,
            tool_results,
            reasoning: model_response.reasoning.clone(),
            selected_neuron_id: neuron_id,
        })
    }
}

/// 工具白名单 ∩ 工具目录：仅授权真实存在的工具。
pub fn filter_authorized_tool_ids(catalog: &dyn ToolCatalog, tool_ids: &[String]) -> Vec<String> {
    let known: HashSet<String> = catalog
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
                phase = PHASE_TOOL_AUTHORIZATION,
                tool_id = %id,
                "ignoring unknown tool id"
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::core::models::ToolSource;
    use crate::tools::tool_registry::{Tool, ToolRegistry};

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
        let registry = Arc::new(RwLock::new(ToolRegistry::new()));
        registry
            .write()
            .unwrap()
            .register_source(DummyTool("echo"), ToolSource::Config);
        let out = filter_authorized_tool_ids(registry.as_ref(), &["echo".into(), "nope".into()]);
        assert_eq!(out, vec!["echo".to_string()]);
    }
}
