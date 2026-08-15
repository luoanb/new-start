use std::sync::Arc;

use async_trait::async_trait;

use super::{
    call_service::{
        message_to_model, read_session_state, session_seed, write_session_state,
        NeuronCallService, RoundInput, RoundOutcome, SessionSeed, SessionState,
    },
    conversation_store::{now_ms, ConversationStore},
    error::AppResult,
    model_call_input::ModelCallInput,
    models::{
        ChatModelSelection, ChatResponse, ConversationMode, Message, MessageBody, MessageRole,
        ModelMessage, ModelMessageRole,
    },
};

/// 触发类型：仅业务编排感知（决定输入侧落库形态），service 不感知。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTriggerKind {
    User,
    ManualStep,
    Poller,
    AgentLoop,
}

/// 单轮输入记录：决定输入侧如何落库与初始 `model_input`。
#[derive(Debug, Clone)]
pub enum InputRecord {
    /// 用户输入：落 user 消息；model_input = 文本。
    User(String),
    /// 轮询推进：model_input 由 before hook 拼（简报）；nudge 落库由 hook 决定
    /// （nudge_persist，简报刷新时才落，「生成一次，落库一次」）。
    Nudge,
    /// 无输入落库，但携带推进文本（Agent 后续轮）。
    Continue(String),
    /// 无输入、无初始文本（默认推进，简报由 hook 注入）。
    None,
}

/// 一轮对话的上下文：runner 组装，before/after hooks 共享，业务字段透传。
#[derive(Debug)]
pub struct RoundContext {
    pub session_id: String,
    pub mode: ConversationMode,
    pub seed: Option<SessionSeed>,
    pub state: SessionState,
    pub messages: Vec<ModelMessage>,
    pub model_input: String,
    /// 本轮模型（用户所选）：主对话与 hook 裁决共用，保证同源。
    pub model: ChatModelSelection,
    pub tool_override: Option<Vec<String>>,
    pub trigger: RoundTriggerKind,
    /// 课题绑定（assistant 业务 hooks 共享；runner 透传，不感知语义）。
    pub topic_id: Option<String>,
    /// 本轮是否进行选型（业务 hooks 按频率算好；runner 透传）。默认 true = 每轮选型。
    pub reselect: bool,
    /// 本轮是否落 nudge 输入消息。Poller 轮由 before hook 在简报刷新（生成）时置位：
    /// 「生成一次，落库一次」，复用缓存简报的推进轮不落重复 nudge。
    pub nudge_persist: bool,
    pub outcome: Option<RoundOutcome>,
}

/// 单轮钩子：业务（Assistant 等）通过注入 before/after 感知一轮的生命周期。
#[async_trait]
pub trait RoundHooks: Send + Sync {
    async fn before_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
    async fn after_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
}

/// 统一编排层：「读会话 → before hooks → converse → after hooks → 落库」。
///
/// 业务无关：不感知触发语义、不感知课题/评分等业务副作用（由 hooks / 各业务 session 文件承担）。
#[derive(Debug, Clone)]
pub struct ConversationRunner {
    store: ConversationStore,
    service: Arc<NeuronCallService>,
}

impl ConversationRunner {
    pub fn new(store: ConversationStore, service: Arc<NeuronCallService>) -> Self {
        Self { store, service }
    }

    /// 一轮端到端：读会话（seed/state/messages）→ before hooks → `converse` →
    /// after hooks → 落库（输入消息 + 产物 + 会话态）。
    ///
    /// `tool_override`：本轮工具授权覆盖（Agent 传全部工具；Chat/Assistant 传 None 按 seed 推导）。
    pub async fn run_round(
        &self,
        session_id: &str,
        input: InputRecord,
        tool_override: Option<Vec<String>>,
        model: &ChatModelSelection,
        hooks: Option<&dyn RoundHooks>,
    ) -> AppResult<ChatResponse> {
        let mut ctx = self.load_context(session_id, input, tool_override, model)?;
        tracing::info!(
            phase = "run_round",
            session_id = %ctx.session_id,
            trigger = ?ctx.trigger,
            mode = ?ctx.mode,
            seed = ?ctx.seed,
            history = ctx.messages.len(),
            input_len = ctx.model_input.len(),
            "round start"
        );
        if let Some(hooks) = hooks {
            hooks.before_round(&mut ctx).await?;
            // before hook 可能切换会话（assistant match_topic switch）→ 重读上下文。
            if ctx.session_id != session_id {
                tracing::info!(
                    phase = "run_round",
                    from_session = %session_id,
                    to_session = %ctx.session_id,
                    "session switched by before hook; reloading context"
                );
                self.reload(&mut ctx)?;
            }
        }
        let outcome = self
            .service
            .converse(
                RoundInput {
                    seed: ctx.seed.clone(),
                    state: ctx.state.clone(),
                    messages: ctx.messages.clone(),
                    tool_override: ctx.tool_override.clone(),
                    reselect: ctx.reselect,
                    // 标签工具按会话模式映射（领域规则在 ConversationMode::tool_tags），runner 仅透传。
                    tool_tags: ctx.mode.tool_tags(),
                },
                &ctx.model_input,
                model,
            )
            .await?;
        tracing::info!(
            phase = "run_round",
            session_id = %ctx.session_id,
            response_len = outcome.response.len(),
            tool_calls = outcome.tool_calls.as_ref().map_or(0, |c| c.len()),
            selected_neuron_id = ?outcome.selected_neuron_id,
            "converse done"
        );
        ctx.outcome = Some(outcome.clone());
        // 先落库（消息 + 会话态），再跑 after hooks：课题副作用（如 complete_scope 模型调用）
        // 失败只影响副作用本身，不丢失本轮模型产物（与旧 AssistantMode 行为一致）。
        self.persist(&ctx)?;
        tracing::info!(phase = "run_round", session_id = %ctx.session_id, "persist done");
        if let Some(hooks) = hooks {
            hooks.after_round(&mut ctx).await?;
        }
        tracing::info!(phase = "run_round", session_id = %ctx.session_id, "round ok");
        Ok(ChatResponse {
            conversation_id: ctx.session_id.clone(),
            response: outcome.response,
        })
    }

    /// 会话历史（模型侧，sanitize 后）。
    pub fn history_for(&self, session_id: &str) -> AppResult<Vec<ModelMessage>> {
        let conversation = self.store.require_conversation(session_id)?;
        Ok(Self::to_model_messages(&conversation.messages))
    }

    /// Agent 收敛判据：会话最后一条消息是否为工具结果。
    pub fn last_message_is_tool_result(&self, session_id: &str) -> AppResult<bool> {
        let conversation = self.store.require_conversation(session_id)?;
        Ok(conversation
            .messages
            .last()
            .map(|m| matches!(m.body, MessageBody::ToolResult { .. }))
            .unwrap_or(false))
    }

    fn to_model_messages(messages: &[Message]) -> Vec<ModelMessage> {
        ModelCallInput::sanitize_tool_pairs(
            &messages
                .iter()
                .filter_map(message_to_model)
                // 防御：过滤历史脏数据——模型偶发空响应的残留（非 tool_call 且 content 空的
                // assistant 消息），否则会被 providers 本地校验拒绝，锁死会话后续调用。
                .filter(|m| {
                    !(m.role == ModelMessageRole::Assistant
                        && m.tool_calls.as_ref().map_or(true, |c| c.is_empty())
                        && m.content.trim().is_empty())
                })
                .collect::<Vec<_>>(),
        )
    }

    fn load_context(
        &self,
        session_id: &str,
        input: InputRecord,
        tool_override: Option<Vec<String>>,
        model: &ChatModelSelection,
    ) -> AppResult<RoundContext> {
        let conversation = self.store.require_conversation(session_id)?;
        let trigger = match &input {
            InputRecord::User(_) => RoundTriggerKind::User,
            InputRecord::Nudge => RoundTriggerKind::Poller,
            InputRecord::Continue(_) => RoundTriggerKind::AgentLoop,
            InputRecord::None => RoundTriggerKind::ManualStep,
        };
        let model_input = match &input {
            InputRecord::User(text) | InputRecord::Continue(text) => text.clone(),
            InputRecord::Nudge | InputRecord::None => String::new(),
        };
        // 先取全部借用值，最后再 move `mode`（ConversationMode 非 Copy）。
        let seed = session_seed(&conversation);
        let state = read_session_state(&conversation);
        let messages = Self::to_model_messages(&conversation.messages);
        Ok(RoundContext {
            session_id: session_id.to_string(),
            mode: conversation.mode,
            seed,
            state,
            messages,
            model_input,
            model: model.clone(),
            tool_override,
            trigger,
            topic_id: None,
            reselect: true,
            nudge_persist: false,
            outcome: None,
        })
    }

    /// 重新同步会话级上下文（before hook 切换会话后；业务字段 topic_id 由 hook 自理）。
    fn reload(&self, ctx: &mut RoundContext) -> AppResult<()> {
        let conversation = self.store.require_conversation(&ctx.session_id)?;
        let seed = session_seed(&conversation);
        let state = read_session_state(&conversation);
        let messages = Self::to_model_messages(&conversation.messages);
        ctx.mode = conversation.mode;
        ctx.seed = seed;
        ctx.state = state;
        ctx.messages = messages;
        Ok(())
    }

    /// 落库：输入消息（按触发形态）→ 产物（tool_call/tool_result 或 assistant text）→ 会话态。
    fn persist(&self, ctx: &RoundContext) -> AppResult<()> {
        // 本轮选中神经元：落库产物消息统一盖章（用户输入消息不盖章）。
        let stamped = ctx
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.selected_neuron_id.clone());
        match ctx.trigger {
            RoundTriggerKind::User => {
                self.store.add_message(
                    &ctx.session_id,
                    Message {
                        role: MessageRole::User,
                        body: MessageBody::Text {
                            content: ctx.model_input.clone(),
                        },
                        timestamp: now_ms(),
                        neuron_id: None,
                    },
                )?;
            }
            RoundTriggerKind::Poller => {
                // 「生成一次，落库一次」：仅简报刷新（nudge_persist 由 before hook 置位）
                // 的这一轮落 nudge 输入消息，复用缓存简报的推进轮不落。
                if ctx.nudge_persist {
                    self.store.add_message(
                        &ctx.session_id,
                        Message {
                            role: MessageRole::User,
                            body: MessageBody::Nudge {
                                content: ctx.model_input.clone(),
                            },
                            timestamp: now_ms(),
                            neuron_id: stamped.clone(),
                        },
                    )?;
                }
            }
            RoundTriggerKind::ManualStep | RoundTriggerKind::AgentLoop => {}
        }
        let Some(outcome) = &ctx.outcome else {
            return Ok(());
        };
        let stored_as = if let Some(tool_calls) = outcome.tool_calls.as_ref() {
            if tool_calls.first().is_some() {
                "tool_call + tool_result"
            } else {
                "assistant text"
            }
        } else {
            "assistant text"
        };
        tracing::info!(
            phase = "run_round",
            session_id = %ctx.session_id,
            trigger = ?ctx.trigger,
            stamped = ?stamped,
            stored_as,
            "persisting messages"
        );
        if let Some(tool_calls) = outcome.tool_calls.as_ref() {
            if let Some(_first) = tool_calls.first() {
                self.store.add_message(
                    &ctx.session_id,
                    Message {
                        role: MessageRole::Assistant,
                        body: MessageBody::ToolCall {
                            content: outcome.model_output.clone().unwrap_or_default(),
                            tool_calls: tool_calls.clone(),
                        },
                        timestamp: now_ms(),
                        neuron_id: stamped.clone(),
                    },
                )?;
                // 每个声明的 tool_call 都执行过：逐条落 Tool 结果，与声明一一配对
                // （sanitize 要求每个声明都有对应结果，否则声明被降级、tool 消息成孤儿）。
                for item in &outcome.tool_results {
                    self.store.add_message(
                        &ctx.session_id,
                        Message {
                            role: MessageRole::Tool,
                            body: MessageBody::ToolResult {
                                tool_call_id: item.tool_call_id.clone(),
                                tool_name: item.tool_name.clone(),
                                content: item.content.clone(),
                            },
                            timestamp: now_ms(),
                            neuron_id: stamped.clone(),
                        },
                    )?;
                }
            }
        } else {
            self.store.add_message(
                &ctx.session_id,
                Message {
                    role: MessageRole::Assistant,
                    body: MessageBody::Text {
                        content: outcome.response.clone(),
                    },
                    timestamp: now_ms(),
                    neuron_id: stamped.clone(),
                },
            )?;
        }
        write_session_state(&self.store, &ctx.session_id, &outcome.state)
    }
}
