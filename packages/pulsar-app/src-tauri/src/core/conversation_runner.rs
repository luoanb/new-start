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
        ModelMessage,
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
    /// 轮询推进：落 nudge 消息；model_input 由 before hook 拼（简报）。
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
    pub tool_override: Option<Vec<String>>,
    pub trigger: RoundTriggerKind,
    /// 课题绑定（assistant 业务 hooks 共享；runner 透传，不感知语义）。
    pub topic_id: Option<String>,
    /// 本轮是否进行选型（业务 hooks 按频率算好；runner 透传）。默认 true = 每轮选型。
    pub reselect: bool,
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
        let mut ctx = self.load_context(session_id, input, tool_override)?;
        if let Some(hooks) = hooks {
            hooks.before_round(&mut ctx).await?;
            // before hook 可能切换会话（assistant match_topic switch）→ 重读上下文。
            if ctx.session_id != session_id {
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
                },
                &ctx.model_input,
                model,
            )
            .await?;
        ctx.outcome = Some(outcome.clone());
        // 先落库（消息 + 会话态），再跑 after hooks：课题副作用（如 complete_scope 模型调用）
        // 失败只影响副作用本身，不丢失本轮模型产物（与旧 AssistantMode 行为一致）。
        self.persist(&ctx)?;
        if let Some(hooks) = hooks {
            hooks.after_round(&mut ctx).await?;
        }
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
                .collect::<Vec<_>>(),
        )
    }

    fn load_context(
        &self,
        session_id: &str,
        input: InputRecord,
        tool_override: Option<Vec<String>>,
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
            tool_override,
            trigger,
            topic_id: None,
            reselect: true,
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
                    },
                )?;
            }
            RoundTriggerKind::Poller => {
                self.store.add_message(
                    &ctx.session_id,
                    Message {
                        role: MessageRole::User,
                        body: MessageBody::Nudge {
                            content: ctx.model_input.clone(),
                        },
                        timestamp: now_ms(),
                    },
                )?;
            }
            RoundTriggerKind::ManualStep | RoundTriggerKind::AgentLoop => {}
        }
        let Some(outcome) = &ctx.outcome else {
            return Ok(());
        };
        if let Some(tool_calls) = outcome.tool_calls.as_ref() {
            if let Some(first) = tool_calls.first() {
                self.store.add_message(
                    &ctx.session_id,
                    Message {
                        role: MessageRole::Assistant,
                        body: MessageBody::ToolCall {
                            content: outcome.model_output.clone().unwrap_or_default(),
                            tool_calls: tool_calls.clone(),
                        },
                        timestamp: now_ms(),
                    },
                )?;
                self.store.add_message(
                    &ctx.session_id,
                    Message {
                        role: MessageRole::Tool,
                        body: MessageBody::ToolResult {
                            tool_call_id: first.id.clone(),
                            tool_name: first.name.clone(),
                            content: outcome.tool_result.clone().unwrap_or_default(),
                        },
                        timestamp: now_ms(),
                    },
                )?;
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
                },
            )?;
        }
        write_session_state(&self.store, &ctx.session_id, &outcome.state)
    }
}
