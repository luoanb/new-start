use std::sync::Arc;

use async_trait::async_trait;

use super::{
    conversation_store::{now_ms, ConversationStore},
    error::AppResult,
    model_call_input::ModelCallInput,
    models::{
        ChatModelSelection, ChatResponse, Conversation, ConversationMode, Message, MessageBody,
        MessageRole, ModelMessage, Neuron, ToolTag,
    },
    round_executor::RoundExecutor,
    round_resolver::RoundResolver,
    round_types::{RoundOutcome, SessionSeed, SessionState},
};

/// 触发类型：仅业务编排感知（决定输入侧落库形态），管道不感知。
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
    /// 真相源：before 时 = 会话历史；resolve + 追加输入消息后 = 完整 wire（本轮的
    /// System / RoleContext / 输入 / Nudge 均在此，落库即 `[old_len..]` 增量）。
    pub messages: Vec<Message>,
    pub model_input: String,
    /// 本轮模型（用户所选）：主对话与 hook 裁决共用，保证同源。
    pub model: ChatModelSelection,
    pub tool_override: Option<Vec<String>>,
    pub trigger: RoundTriggerKind,
    /// 课题绑定（assistant 业务 hooks 共享；runner 透传，不感知语义）。
    pub topic_id: Option<String>,
    /// 本轮是否进行选型（业务 hooks 按频率算好；runner 透传）。默认 true = 每轮选型。
    pub reselect: bool,
    /// 是否将简报构造为 Nudge 消息进 wire（进 wire 必落库）。Poller 轮由 before hook
    /// 在简报刷新（生成）时置位：「生成一次，进 wire 一次」；复用缓存简报的推进轮不构造
    /// （历史回灌自带简报）。
    pub nudge_persist: bool,
    /// ① resolve 后填充：本轮选中神经元（hooks 审计「选了谁」；产物落库盖章）。
    pub selected_neuron: Option<Neuron>,
    /// ② 单轮产物：persist 落库（tool_call/tool_result 或 assistant text）。
    pub outcome: Option<RoundOutcome>,
}

/// 单轮钩子：业务（Assistant 等）通过注入 before/after 感知一轮的生命周期。
#[async_trait]
pub trait RoundHooks: Send + Sync {
    async fn before_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
    async fn after_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
}

/// 统一编排层：「读会话 → before hooks → 三段管道（选型 → 组装 → 执行）→ after hooks → 落库」。
///
/// 业务无关：不感知触发语义、不感知课题/评分等业务副作用（由 hooks / 各业务 session 文件承担）。
#[derive(Debug, Clone)]
pub struct ConversationRunner {
    store: ConversationStore,
    resolver: Arc<RoundResolver>,
    executor: Arc<RoundExecutor>,
}

impl ConversationRunner {
    pub fn new(
        store: ConversationStore,
        resolver: Arc<RoundResolver>,
        executor: Arc<RoundExecutor>,
    ) -> Self {
        Self {
            store,
            resolver,
            executor,
        }
    }

    /// 一轮端到端：读会话（seed/state/messages）→ before hooks → resolve（选型+角色拼接）
    /// → 写回锚点（发送前）→ 构造输入消息 → persist_input（落 wire 增量）→ execute →
    /// persist_outcome → after hooks。
    ///
    /// `tool_override`：本轮工具授权覆盖（Agent 传全部工具；Chat/Assistant 传 None 按模式取
    /// 神经元 tool_ids）；标签并入按 `ConversationMode::tool_tags()`，均落点在 executor。
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
        // ① 选型 + 角色上下文拼接（resolve 目标单一：获取角色神经元）。
        // 输出 = old + 角色上下文（首轮 System / 后续 RoleContext），不含本轮输入消息。
        let old_len = ctx.messages.len();
        let (with_role, neuron) = self
            .resolver
            .resolve(
                ctx.seed.as_ref(),
                ctx.state.last_selected_neuron_id.as_deref(),
                &ctx.messages,
                ctx.reselect,
            )
            .await?;
        tracing::info!(
            phase = "run_round",
            session_id = %ctx.session_id,
            selected_neuron_id = ?neuron.as_ref().map(|n| n.id.as_str()),
            role_msgs = with_role.len() - old_len,
            "resolve done"
        );
        // 发送前写回锚点（D7）：resolve 已定选中神经元，模型调用前落会话态。
        // 选中 → 写回其 id；未选中（直连/选型失败）→ 清空锚点。
        let anchor = neuron.as_ref().map(|n| n.id.clone());
        if anchor != ctx.state.last_selected_neuron_id {
            ctx.state.last_selected_neuron_id = anchor;
            write_session_state(&self.store, &ctx.session_id, &ctx.state)?;
        }
        ctx.selected_neuron = neuron;
        ctx.messages = with_role;
        // ② 构造输入消息（User / Continue / Nudge → Message，kind 自明）append，构成完整 wire。
        self.append_input_message(&mut ctx);
        // 发送前落输入增量：wire[old.len()..]（System / RoleContext / 输入 / Nudge）全落——
        // 模型调用失败/超时也不丢用户消息；产物在发送后落。
        self.persist_input(&ctx, old_len)?;
        // ③ 执行：工具授权落点（override 优先 → neuron.tool_ids → 标签并入 ∩ 注册表）→
        // 发送前投影 ModelMessage → 模型调用 → 单轮全部工具执行。
        let outcome = self
            .executor
            .execute(
                ctx.selected_neuron.as_ref(),
                &ctx.messages,
                model,
                ctx.tool_override.clone(),
                ctx.mode.tool_tags(),
            )
            .await?;
        tracing::info!(
            phase = "run_round",
            session_id = %ctx.session_id,
            response_len = outcome.response.len(),
            tool_calls = outcome.tool_calls.as_ref().map_or(0, |c| c.len()),
            selected_neuron_id = ?outcome.selected_neuron_id,
            "execute done"
        );
        ctx.outcome = Some(outcome.clone());
        // 发送后落产物（会话态已在发送前写回），再跑 after hooks：课题副作用
        // （如 complete_scope 模型调用）失败只影响副作用本身，不丢失本轮模型产物。
        self.persist_outcome(&ctx)?;
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

    /// 原始单轮管道（无会话）：① resolve（选型+角色拼接）→ 追加输入 → ② execute，
    /// 不读库、不落库、不跑 hooks、不写回锚点。
    ///
    /// 供内部非对话调用（assistant 裁决等）复用同一管道；`last_selected` 仅作选型锚点入参，
    /// 产物 `selected_neuron_id` 由 executor 盖章返回。
    pub async fn run_raw_round(
        &self,
        seed: Option<SessionSeed>,
        last_selected: Option<String>,
        old_messages: Vec<Message>,
        model_input: &str,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
        reselect: bool,
        model: &ChatModelSelection,
    ) -> AppResult<RoundOutcome> {
        let (with_role, neuron) = self
            .resolver
            .resolve(seed.as_ref(), last_selected.as_deref(), &old_messages, reselect)
            .await?;
        let mut wire = with_role;
        if !model_input.trim().is_empty() {
            wire.push(Message {
                role: MessageRole::User,
                body: MessageBody::Text {
                    content: model_input.to_string(),
                },
                timestamp: now_ms(),
                neuron_id: None,
            });
        }
        self.executor
            .execute(neuron.as_ref(), &wire, model, tool_override, tool_tags)
            .await
    }

    /// 会话历史（模型侧，project_history 投影 + 防御过滤 + sanitize 后）。
    pub fn history_for(&self, session_id: &str) -> AppResult<Vec<ModelMessage>> {
        let conversation = self.store.require_conversation(session_id)?;
        Ok(ModelCallInput::project_history(&conversation.messages))
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
        let messages = conversation.messages.clone();
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
            selected_neuron: None,
            outcome: None,
        })
    }

    /// 重新同步会话级上下文（before hook 切换会话后；业务字段 topic_id 由 hook 自理）。
    fn reload(&self, ctx: &mut RoundContext) -> AppResult<()> {
        let conversation = self.store.require_conversation(&ctx.session_id)?;
        let seed = session_seed(&conversation);
        let state = read_session_state(&conversation);
        let messages = conversation.messages.clone();
        ctx.mode = conversation.mode;
        ctx.seed = seed;
        ctx.state = state;
        ctx.messages = messages;
        Ok(())
    }

    /// 按触发形态构造输入消息（`InputRecord` → `Message`，kind 自明）append 到 `ctx.messages`，
    /// 与 resolve 的角色上下文一起构成完整 wire（进 wire 即落库）。
    fn append_input_message(&self, ctx: &mut RoundContext) {
        match ctx.trigger {
            RoundTriggerKind::User => {
                ctx.messages.push(Message {
                    role: MessageRole::User,
                    body: MessageBody::Text {
                        content: ctx.model_input.clone(),
                    },
                    timestamp: now_ms(),
                    neuron_id: None,
                });
            }
            RoundTriggerKind::Poller => {
                // 「生成一次，进 wire 一次」：仅简报刷新（nudge_persist 由 before hook 置位）
                // 的这一轮构造 Nudge 进 wire（进 wire 必落库）；复用缓存简报的推进轮不构造，
                // 历史回灌自带简报。
                if ctx.nudge_persist {
                    ctx.messages.push(Message {
                        role: MessageRole::User,
                        body: MessageBody::Nudge {
                            content: ctx.model_input.clone(),
                        },
                        timestamp: now_ms(),
                        neuron_id: ctx.selected_neuron.as_ref().map(|n| n.id.clone()),
                    });
                }
            }
            RoundTriggerKind::ManualStep | RoundTriggerKind::AgentLoop => {}
        }
    }

    /// 落库（发送前）：落 `ctx.messages[old_len..]` 增量，全落（System / RoleContext / 输入 /
    /// Nudge）——wire 即落库，模型调用失败/超时也不丢用户消息；产物在发送后落。
    fn persist_input(&self, ctx: &RoundContext, old_len: usize) -> AppResult<()> {
        for message in &ctx.messages[old_len..] {
            self.store.add_message(&ctx.session_id, message.clone())?;
        }
        Ok(())
    }

    /// 落库（发送后）：产物（tool_call/tool_result 或 assistant text）+ 会话态。
    fn persist_outcome(&self, ctx: &RoundContext) -> AppResult<()> {
        let Some(outcome) = &ctx.outcome else {
            return Ok(());
        };
        let stamped = outcome.selected_neuron_id.clone();
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
        // 会话态已在本轮发送前写回（last_selected 锚点），产物落库不再推进状态。
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 会话元数据读写（原 call_service.rs 迁入）：`conversation.extra.session` 的
// 种子 / 运行态（选型锚点）/ 发起神经元绑定。仅 Runner 消费（读会话、发送前写回锚点）。
// ---------------------------------------------------------------------------

const EXTRA_SESSION_KEY: &str = "session";
const EXTRA_STATE_KEY: &str = "state";
const EXTRA_SPEC_NEURON_ID_KEY: &str = "spec_neuron_id";
const EXTRA_SEED_KEY: &str = "seed";

/// 读取会话运行态（缺失 / 非法回落默认）。
fn read_session_state(conversation: &Conversation) -> SessionState {
    conversation
        .extra
        .as_ref()
        .and_then(|extra| extra.get(EXTRA_SESSION_KEY))
        .and_then(|session| session.get(EXTRA_STATE_KEY))
        .and_then(|state| serde_json::from_value(state.clone()).ok())
        .unwrap_or_default()
}

/// 读取会话绑定的发起神经元 id（未绑定发起神经元时返回 None）。
fn session_spec_neuron_id(conversation: &Conversation) -> Option<String> {
    conversation
        .extra
        .as_ref()
        .and_then(|extra| extra.get(EXTRA_SESSION_KEY))
        .and_then(|session| session.get(EXTRA_SPEC_NEURON_ID_KEY))
        .and_then(|value| value.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// 读取会话种子：优先新字段 `extra.session.seed`；旧数据回退 `spec_neuron_id` → `Neuron(id)`。
fn session_seed(conversation: &Conversation) -> Option<SessionSeed> {
    conversation
        .extra
        .as_ref()
        .and_then(|extra| extra.get(EXTRA_SESSION_KEY))
        .and_then(|session| session.get(EXTRA_SEED_KEY))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .or_else(|| session_spec_neuron_id(conversation).map(SessionSeed::Neuron))
}

/// 将运行态写回 `extra.session.state`（保留其它 extra 键与发起神经元绑定）。
fn set_session_state(conversation: &mut Conversation, state: &SessionState) {
    let mut extra = conversation.extra.take().unwrap_or_else(|| serde_json::json!({}));
    if !extra.is_object() {
        extra = serde_json::json!({});
    }
    let session = extra
        .get(EXTRA_SESSION_KEY)
        .cloned()
        .unwrap_or_else(|| serde_json::json!({ EXTRA_SPEC_NEURON_ID_KEY: "" }));
    let mut session_obj = if session.is_object() {
        session
    } else {
        serde_json::json!({ EXTRA_SPEC_NEURON_ID_KEY: "" })
    };
    session_obj[EXTRA_STATE_KEY] = serde_json::to_value(state).unwrap_or_default();
    extra[EXTRA_SESSION_KEY] = session_obj;
    conversation.extra = Some(extra);
}

/// 写回会话运行态。
fn write_session_state(
    store: &ConversationStore,
    session_id: &str,
    state: &SessionState,
) -> AppResult<()> {
    let mut conversation = store.require_conversation(session_id)?;
    set_session_state(&mut conversation, state);
    store.save_conversation(&conversation)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex, RwLock,
        },
    };

    use async_trait::async_trait;
    use rusqlite::Connection as SqliteConnection;
    use serde_json::json;

    use super::*;
    use crate::core::{
        error::{AppError, AppResult},
        models::{
            ChatModelSelection, ConversationMode, ModelCallRequest, ModelCallResponse,
            ModelMessage, Neuron, NeuronCreate, SelectionPolicy,
            SessionBehavior, ToolCall, ToolDefinition, ToolPolicy, ToolSource, ToolTag,
        },
        neuron::{
            config::NeuronConfigReader,
            manager::ASSISTANT_SELECT_NEURON,
            model::NeuronModelCaller,
            store::NeuronStore,
        },
        neuron_manager::NeuronManager,
        round_executor::{ModelCaller, RoundExecutor},
        round_resolver::RoundResolver,
        tool_registry::Tool,
        tool_registry::ToolRegistry,
    };

    fn model() -> ChatModelSelection {
        ChatModelSelection {
            provider_id: "test".into(),
            model_id: "test-model".into(),
        }
    }

    /// 空会话（`Conversation` 未实现 `Default`，测试手工构造）。
    fn empty_conversation() -> Conversation {
        Conversation {
            id: String::new(),
            mode: ConversationMode::Chat,
            messages: Vec::new(),
            created_at: 0,
            updated_at: 0,
            extra: None,
        }
    }

    /// 选型/创建模型替身：消息含候选 JSON（`"id":"`）→ 选型调用，返回 `{"neuron_id": ...}`；
    /// 否则为创建调用（fill_candidates_batch），解析 `exactly N` 返回 N 条 draft 数组。
    struct MockSelector {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl NeuronModelCaller for MockSelector {
        async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            let blob = messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<String>();
            if let Some(idx) = blob.find("\"id\":\"") {
                let start = idx + 6;
                let end = blob[start..]
                    .find('"')
                    .map(|e| start + e)
                    .unwrap_or(blob.len());
                let id = &blob[start..end];
                return Ok(format!(r#"{{"neuron_id":"{id}"}}"#));
            }
            let count = blob
                .split("exactly ")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|token| token.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
                .unwrap_or(1usize);
            let items: Vec<String> = (0..count)
                .map(|i| {
                    format!(
                        r#"{{"desc":"auto-{call}-{i}","content":"auto-{call}-{i}","weight":1.0,"tool_ids":[]}}"#
                    )
                })
                .collect();
            Ok(format!("[{}]", items.join(",")))
        }
    }

    /// 主模型替身：固定输出 `echo-{call}`；可配置附带一个工具调用。
    struct EchoCaller {
        calls: Arc<AtomicUsize>,
        tool_call: Arc<Mutex<Option<Vec<ToolCall>>>>,
        last_messages: Arc<Mutex<Vec<ModelMessage>>>,
        last_tools: Arc<Mutex<Vec<ToolDefinition>>>,
    }

    #[async_trait]
    impl ModelCaller for EchoCaller {
        async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            *self.last_messages.lock().unwrap() = request.messages;
            *self.last_tools.lock().unwrap() = request.tools.unwrap_or_default();
            Ok(ModelCallResponse {
                provider_id: "test".into(),
                model_id: "test-model".into(),
                output: format!("echo-{call}"),
                tool_calls: self.tool_call.lock().unwrap().clone(),
                finish_reason: "stop".into(),
            })
        }
    }

    /// 测试工具：回显参数 text（name 可配，便于注册多个不同标签/名称的工具）。
    struct EchoTool {
        name: &'static str,
    }

    impl EchoTool {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "echo back text"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({"type":"object","properties":{"text":{"type":"string"}}})
        }
        async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            Ok(format!("echo:{text}"))
        }
    }

    struct Harness {
        store: Arc<Mutex<NeuronStore>>,
        manager: Arc<NeuronManager>,
        resolver: Arc<RoundResolver>,
        executor: Arc<RoundExecutor>,
        selector_calls: Arc<AtomicUsize>,
        echo_calls: Arc<AtomicUsize>,
        echo_tool_call: Arc<Mutex<Option<Vec<ToolCall>>>>,
        last_messages: Arc<Mutex<Vec<ModelMessage>>>,
        last_tools: Arc<Mutex<Vec<ToolDefinition>>>,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        root: std::path::PathBuf,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn harness() -> Harness {
        let conn = Arc::new(Mutex::new(SqliteConnection::open_in_memory().unwrap()));
        let store = Arc::new(Mutex::new(NeuronStore::new(conn)));
        store.lock().unwrap().init_table().unwrap();
        let root = std::env::temp_dir().join(format!(
            "pulsar-round-pipeline-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            r#"{"neurons":{"bootstrap":{"create_neuron_prompt":"create a neuron"}}}"#,
        )
        .unwrap();
        let selector_calls = Arc::new(AtomicUsize::new(0));
        let echo_calls = Arc::new(AtomicUsize::new(0));
        let echo_tool_call: Arc<Mutex<Option<Vec<ToolCall>>>> = Arc::new(Mutex::new(None));
        let last_messages: Arc<Mutex<Vec<ModelMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let last_tools: Arc<Mutex<Vec<ToolDefinition>>> = Arc::new(Mutex::new(Vec::new()));
        let selector = MockSelector {
            calls: Arc::clone(&selector_calls),
        };
        let echo: Arc<dyn ModelCaller> = Arc::new(EchoCaller {
            calls: Arc::clone(&echo_calls),
            tool_call: Arc::clone(&echo_tool_call),
            last_messages: Arc::clone(&last_messages),
            last_tools: Arc::clone(&last_tools),
        });
        let tool_registry = Arc::new(RwLock::new(ToolRegistry::new()));
        let manager = Arc::new(NeuronManager::new(
            Arc::clone(&store),
            Arc::new(selector),
            NeuronConfigReader::new(root.clone()),
            Arc::clone(&tool_registry),
        ));
        let resolver = Arc::new(RoundResolver::new(Arc::clone(&manager)));
        let executor = Arc::new(RoundExecutor::new(
            Arc::clone(&echo),
            Arc::clone(&tool_registry),
        ));
        Harness {
            store,
            manager,
            resolver,
            executor,
            selector_calls,
            echo_calls,
            echo_tool_call,
            last_messages,
            last_tools,
            tool_registry,
            root,
        }
    }

    /// 原始管道驱动（v2 语义，mirror `run_raw_round`）：resolve（选型+角色拼接）→
    /// 追加输入消息 → execute。产出完整 wire（Vec<Message>）+ 选中神经元，供断言消费。
    async fn run_pipeline(
        h: &Harness,
        seed: Option<SessionSeed>,
        last_selected: Option<String>,
        messages: Vec<Message>,
        model_input: &str,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
        reselect: bool,
    ) -> AppResult<(Vec<Message>, Option<Neuron>, RoundOutcome)> {
        let (with_role, neuron) = h
            .resolver
            .resolve(seed.as_ref(), last_selected.as_deref(), &messages, reselect)
            .await?;
        let mut wire = with_role;
        if !model_input.trim().is_empty() {
            wire.push(Message {
                role: MessageRole::User,
                body: MessageBody::Text {
                    content: model_input.to_string(),
                },
                timestamp: now_ms(),
                neuron_id: None,
            });
        }
        let outcome = h
            .executor
            .execute(neuron.as_ref(), &wire, &model(), tool_override, tool_tags)
            .await?;
        Ok((wire, neuron, outcome))
    }

    /// 落库层用户消息构造（测试便捷）。
    fn user_msg(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            body: MessageBody::Text {
                content: content.to_string(),
            },
            timestamp: now_ms(),
            neuron_id: None,
        }
    }

    /// 确保选型系统神经元存在（否则 `select_one_from_with_history` 回退按权重选）。
    fn ensure_selector(h: &Harness) {
        let store = h.store.lock().unwrap();
        if store
            .get_neuron_by_system_type(ASSISTANT_SELECT_NEURON)
            .unwrap()
            .is_none()
        {
            store
                .create_neuron(NeuronCreate {
                    desc: "selector".into(),
                    content: "pick one".into(),
                    system_type: Some(ASSISTANT_SELECT_NEURON.to_string()),
                    ..Default::default()
                })
                .unwrap();
        }
    }

    fn insert_plain(h: &Harness, desc: &str) -> Neuron {
        h.store
            .lock()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: desc.into(),
                content: format!("{desc} content"),
                ..Default::default()
            })
            .unwrap()
    }

    fn insert_downstream(h: &Harness, parent_id: &str, desc: &str) -> Neuron {
        h.manager
            .create_plain(
                NeuronCreate {
                    desc: desc.into(),
                    content: format!("{desc} content"),
                    ..Default::default()
                },
                Some(parent_id),
            )
            .unwrap()
    }

    fn insert_system(h: &Harness, system_type: &str, behavior: SessionBehavior, content: &str) -> Neuron {
        let store = h.store.lock().unwrap();
        let neuron = store
            .create_neuron(NeuronCreate {
                desc: system_type.into(),
                content: content.into(),
                system_type: Some(system_type.to_string()),
                ..Default::default()
            })
            .unwrap();
        store.set_behavior(&neuron.id, Some(&behavior)).unwrap()
    }

    #[tokio::test]
    async fn converse_keeps_history_and_input_in_model_input() {
        let h = harness();
        let history = vec![user_msg("past")];
        let (wire, neuron, _outcome) = run_pipeline(
            &h,
            None,
            None,
            history,
            "current",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        // 直连：不选型、不注入角色上下文；wire = 历史 + 本轮输入（无模板层）。
        assert!(neuron.is_none());
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0].text(), "past");
        assert_eq!(wire[1].text(), "current");
        let blob = h
            .last_messages
            .lock()
            .unwrap()
            .iter()
            .map(|m| format!("{:?}:{}", m.role, m.content))
            .collect::<String>();
        assert!(blob.contains("past"), "history should reach model: {blob}");
        assert!(blob.contains("current"), "current input should reach model: {blob}");
        assert!(
            !blob.contains("【神经元】"),
            "v2 无模板层：历史与输入原样投影: {blob}"
        );
    }

    #[test]
    fn session_state_roundtrip_via_extra() {
        let state = SessionState {
            last_selected_neuron_id: Some("n-1".into()),
            ..Default::default()
        };
        let mut conversation = empty_conversation();
        set_session_state(&mut conversation, &state);
        let read = read_session_state(&conversation);
        assert_eq!(read.last_selected_neuron_id, Some("n-1".into()));
    }

    #[test]
    fn session_seed_reads_new_field_and_falls_back_to_spec() {
        // 新字段优先。
        let mut conversation = empty_conversation();
        conversation.extra = Some(json!({
            "session": {
                "spec_neuron_id": "session.old",
                "state": {},
                "seed": {"kind": "global"}
            }
        }));
        assert_eq!(session_seed(&conversation), Some(SessionSeed::Global));

        // 旧数据回退：无 seed 字段 → Neuron(spec_neuron_id)。
        let mut legacy = empty_conversation();
        legacy.extra = Some(json!({
            "session": {"spec_neuron_id": "session.old", "state": {}}
        }));
        assert_eq!(
            session_seed(&legacy),
            Some(SessionSeed::Neuron("session.old".into()))
        );
        assert_eq!(session_spec_neuron_id(&legacy), Some("session.old".into()));

        // 无会话元数据 → None（直连）。
        let plain = empty_conversation();
        assert_eq!(session_seed(&plain), None);
    }

    #[tokio::test]
    async fn converse_role_first_round_system_then_context() {
        let h = harness();
        let iso = insert_plain(&h, "iso");
        // 第一轮（空历史）：选中神经元角色进 System（落库即稳定，替代 B2 冻结状态机）。
        let (first_wire, first_neuron, _) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(iso.id.clone())),
            None,
            vec![],
            "go",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        let first = first_wire.first().expect("first round has a system message");
        assert_eq!(first.role, MessageRole::System);
        assert_eq!(first.neuron_id, None);
        let anchor = first_neuron.expect("first round selects").id.clone();
        // 第二轮（有历史 + 复用轮）：选中神经元以 [当前角色] 前缀带出 RoleContext。
        let (second_wire, _, _) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(iso.id.clone())),
            Some(anchor),
            vec![user_msg("previous turn")],
            "again",
            None,
            Vec::new(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(second_wire.first().unwrap().role, MessageRole::User);
        let ctx = second_wire
            .iter()
            .find(|m| matches!(m.body, MessageBody::RoleContext { .. }))
            .expect("non-first round carries role context");
        assert!(ctx.text().starts_with("[当前角色]\n"), "ctx: {}", ctx.text());
    }

    #[tokio::test]
    async fn converse_direct_seed_no_selection() {
        let h = harness();
        let (wire, neuron, outcome) = run_pipeline(
            &h,
            None,
            None,
            vec![],
            "hello",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(outcome.selected_neuron_id, None);
        assert!(neuron.is_none());
        assert_eq!(wire.len(), 1, "直连：wire = 仅输入消息");
        assert_eq!(wire[0].text(), "hello");
        assert_eq!(outcome.response, "echo-0");
        assert_eq!(h.echo_calls.load(Ordering::Relaxed), 1);
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn converse_global_first_round_selects_anchor() {
        let h = harness();
        ensure_selector(&h);
        // 全域池填满 Global limit（7）个候选：避免候选不足触发 AI 创建，聚焦「全域选 1 + 写锚点」。
        for i in 0..7 {
            insert_plain(&h, &format!("cand-{i}"));
        }
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Global),
            None,
            vec![],
            "go",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 1);
        let selected = outcome.selected_neuron_id.expect("global round selects");
        assert_eq!(
            neuron.as_ref().map(|n| n.id.clone()).as_deref(),
            Some(selected.as_str())
        );
        assert_ne!(selected, "");
    }

    #[tokio::test]
    async fn converse_global_with_history_uses_neighborhood() {
        let h = harness();
        ensure_selector(&h);
        let root = insert_plain(&h, "root");
        let child_a = insert_downstream(&h, &root.id, "child-a");
        let _child_b = insert_downstream(&h, &root.id, "child-b");
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Global),
            Some(child_a.id.clone()),
            vec![],
            "advance",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        // 有历史 → 邻域选（锚点 = last_selected）。邻域候选会按策略扩展创建 → 至少一次模型调用。
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.unwrap();
        assert_eq!(
            neuron.as_ref().map(|n| n.id.clone()).as_deref(),
            Some(selected.as_str())
        );
    }

    #[tokio::test]
    async fn converse_plain_neuron_defaults_to_neighborhood() {
        let h = harness();
        // 普通神经元 seed → 推导默认邻域行为（锚点 = 自身）；邻域候选按策略扩展创建后选型。
        let iso = insert_plain(&h, "iso");
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(iso.id.clone())),
            None,
            vec![],
            "hi",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.expect("neighborhood selects");
        assert_eq!(
            neuron.as_ref().map(|n| n.id.clone()).as_deref(),
            Some(selected.as_str())
        );
        assert_eq!(outcome.response, "echo-0");
    }

    #[tokio::test]
    async fn converse_system_fixed_uses_own_content() {
        let h = harness();
        let sys = insert_system(
            &h,
            "session.test_fixed",
            SessionBehavior {
                selection: SelectionPolicy::Fixed,
                tools: ToolPolicy::None,
                insert_id: None,
            },
            "FIXED-CONTENT",
        );
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(sys.id.clone())),
            Some("pre".into()),
            vec![],
            "go",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        // Fixed：选中系统神经元自身（runner 发送前写回其 id，覆盖旧锚点）；不调 selector。
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(sys.id.as_str()));
        assert_eq!(neuron.as_ref().map(|n| n.id.as_str()), Some(sys.id.as_str()));
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0);
        assert_eq!(h.echo_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn converse_system_global_falls_back_to_neighborhood() {
        let h = harness();
        // 系统神经元配置 Global → 宽容回退 Neighborhood（锚点 = 自身）：邻域候选扩展创建后选型。
        let sys = insert_system(
            &h,
            "session.test_global",
            SessionBehavior {
                selection: SelectionPolicy::Global { limit: 5 },
                tools: ToolPolicy::None,
                insert_id: None,
            },
            "GLOBAL-BUT-SYSTEM",
        );
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(sys.id.clone())),
            None,
            vec![],
            "go",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.expect("neighborhood selects");
        assert_eq!(
            neuron.as_ref().map(|n| n.id.clone()).as_deref(),
            Some(selected.as_str())
        );
    }

    #[tokio::test]
    async fn converse_system_none_clears_anchor() {
        let h = harness();
        let sys = insert_system(
            &h,
            "session.test_none",
            SessionBehavior {
                selection: SelectionPolicy::None,
                tools: ToolPolicy::None,
                insert_id: None,
            },
            "no-selection",
        );
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(sys.id.clone())),
            Some("pre".into()),
            vec![],
            "go",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(outcome.selected_neuron_id, None);
        assert!(neuron.is_none(), "None 策略不选型，runner 发送前清空锚点");
    }

    #[tokio::test]
    async fn converse_tool_override_grants_tools() {
        let h = harness();
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo"), ToolSource::Config);
        *h.echo_tool_call.lock().unwrap() = Some(vec![ToolCall {
            id: "call-1".into(),
            name: "echo".into(),
            arguments: json!({"text": "hi"}),
        }]);
        let (_, _, outcome) = run_pipeline(
            &h,
            None,
            None,
            vec![],
            "go",
            Some(vec!["echo".into()]),
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(outcome.tool_results.len(), 1);
        assert_eq!(outcome.tool_results[0].tool_name, "echo");
        assert_eq!(outcome.tool_results[0].content, "echo:hi");
        assert_eq!(outcome.tool_results[0].tool_call_id, "call-1");
        assert!(outcome.response.contains("[tool:echo] echo:hi"));
        assert!(outcome.tool_calls.is_some());
        assert_eq!(outcome.model_output.as_deref(), Some("echo-0"));
    }

    /// 一轮内模型声明多个 tool_calls：全部执行、全部落产物（不截断首个）。
    #[tokio::test]
    async fn converse_executes_all_declared_tools() {
        let h = harness();
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo"), ToolSource::Config);
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo2"), ToolSource::Config);
        *h.echo_tool_call.lock().unwrap() = Some(vec![
            ToolCall {
                id: "call-1".into(),
                name: "echo".into(),
                arguments: json!({"text": "hi"}),
            },
            ToolCall {
                id: "call-2".into(),
                name: "echo2".into(),
                arguments: json!({"text": "there"}),
            },
        ]);
        let (_, _, outcome) = run_pipeline(
            &h,
            None,
            None,
            vec![],
            "go",
            Some(vec!["echo".into(), "echo2".into()]),
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        // 两条声明全部执行，结果一一配对。
        assert_eq!(outcome.tool_calls.as_ref().map(|c| c.len()), Some(2));
        assert_eq!(outcome.tool_results.len(), 2);
        assert_eq!(outcome.tool_results[0].tool_call_id, "call-1");
        assert_eq!(outcome.tool_results[0].tool_name, "echo");
        assert_eq!(outcome.tool_results[0].content, "echo:hi");
        assert_eq!(outcome.tool_results[1].tool_call_id, "call-2");
        assert_eq!(outcome.tool_results[1].tool_name, "echo2");
        assert_eq!(outcome.tool_results[1].content, "echo:there");
        // 拼接产物包含两条工具结果。
        assert!(outcome.response.contains("[tool:echo] echo:hi"));
        assert!(outcome.response.contains("[tool:echo2] echo:there"));
    }

    #[tokio::test]
    async fn converse_unauthorized_tool_rejected() {
        let h = harness();
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo"), ToolSource::Config);
        *h.echo_tool_call.lock().unwrap() = Some(vec![ToolCall {
            id: "call-1".into(),
            name: "echo".into(),
            arguments: json!({"text": "hi"}),
        }]);
        let err = run_pipeline(
            &h,
            None,
            None,
            vec![],
            "go",
            // echo 未授权：override 空 + 无标签工具 → 声明 echo 必须被拒绝。
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    /// 选型降频（复用轮）：`reselect: false` 且有历史锚点时，直接沿用
    /// last_selected_neuron_id，跳过 LLM 选型。
    #[tokio::test]
    async fn converse_selection_false_reuses_anchor_skips_selector() {
        let h = harness();
        ensure_selector(&h);
        for i in 0..7 {
            insert_plain(&h, &format!("cand-{i}"));
        }
        let anchor = insert_plain(&h, "anchor");
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Global),
            Some(anchor.id.clone()),
            vec![],
            "advance",
            None,
            Vec::new(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0, "复用轮不得调 selector");
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(anchor.id.as_str()));
        assert_eq!(
            neuron.as_ref().map(|n| n.id.as_str()),
            Some(anchor.id.as_str())
        );
        assert_eq!(outcome.response, "echo-0");
    }

    /// 选型轮：`reselect: true` 时即使有历史锚点也重新选型（频率计算在业务层，引擎只认意图）。
    #[tokio::test]
    async fn converse_selection_true_reselects_despite_anchor() {
        let h = harness();
        ensure_selector(&h);
        for i in 0..7 {
            insert_plain(&h, &format!("cand-{i}"));
        }
        let anchor = insert_plain(&h, "anchor");
        for _ in 0..2 {
            let before = h.selector_calls.load(Ordering::Relaxed);
            let (_, _, outcome) = run_pipeline(
                &h,
                Some(SessionSeed::Global),
                Some(anchor.id.clone()),
                vec![],
                "advance",
                None,
                Vec::new(),
                true,
            )
            .await
            .unwrap();
            assert!(
                outcome.selected_neuron_id.is_some(),
                "选型轮必须产出选型"
            );
            assert!(
                h.selector_calls.load(Ordering::Relaxed) > before,
                "选型轮必须触发 selector"
            );
        }
    }

    /// 复用轮但无历史锚点：降频不适用，回退走正常选型。
    #[tokio::test]
    async fn converse_selection_false_no_anchor_falls_back_to_select() {
        let h = harness();
        ensure_selector(&h);
        for i in 0..7 {
            insert_plain(&h, &format!("cand-{i}"));
        }
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Global),
            None,
            vec![],
            "advance",
            None,
            Vec::new(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 1);
        let selected = outcome.selected_neuron_id.expect("无锚点时复用轮也回退选型");
        assert_eq!(
            neuron.as_ref().map(|n| n.id.clone()).as_deref(),
            Some(selected.as_str())
        );
    }

    /// `reselect` 只影响真正调 LLM 的分支：Fixed / None 策略不感知（按原规则执行）。
    #[tokio::test]
    async fn converse_selection_fixed_ignores_reselect() {
        let h = harness();
        let sys = insert_system(
            &h,
            "session.test_fixed_reuse",
            SessionBehavior {
                selection: SelectionPolicy::Fixed,
                tools: ToolPolicy::None,
                insert_id: None,
            },
            "FIXED-CONTENT",
        );
        let (_, neuron, outcome) = run_pipeline(
            &h,
            Some(SessionSeed::Neuron(sys.id.clone())),
            Some("pre".into()),
            vec![],
            "go",
            None,
            Vec::new(),
            false,
        )
        .await
        .unwrap();
        // Fixed：复用轮也读自己 content，不调 selector（runner 发送前写回其 id）。
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(sys.id.as_str()));
        assert_eq!(neuron.as_ref().map(|n| n.id.as_str()), Some(sys.id.as_str()));
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0);
        assert_eq!(outcome.response, "echo-0");
    }

    /// 标签消费：Core 无条件并入所有对话 wire；System 仅系统模式会话并入；Normal 由神经元管理。
    #[tokio::test]
    async fn tag_consumption_into_wire() {
        let h = harness();
        {
            let mut reg = h.tool_registry.write().unwrap();
            reg.register_tagged(ToolTag::Core, EchoTool::new("core_echo"), ToolSource::Config);
            reg.register_tagged(ToolTag::System, EchoTool::new("sys_echo"), ToolSource::Config);
            reg.register_tagged(ToolTag::Normal, EchoTool::new("plain_echo"), ToolSource::Config);
        }
        let wire_names = || {
            h.last_tools
                .lock()
                .unwrap()
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>()
        };

        // 1) Chat 对话（无 override、无神经元）：禁用工具，不注入 Core。
        run_pipeline(
            &h,
            None,
            None,
            vec![],
            "hello",
            None,
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert!(wire_names().is_empty(), "Chat 对话应禁用工具（不注入 Core）");

        // 2) 系统模式对话：Core + System 进 wire，Normal（神经元管理）不进。
        run_pipeline(
            &h,
            None,
            None,
            vec![],
            "hello",
            None,
            vec![ToolTag::Core, ToolTag::System],
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            wire_names(),
            vec!["core_echo", "sys_echo"],
            "系统模式应注入 Core + System"
        );

        // 3) 非对话调用（mode=None，如禁工具的内部裁决）：不注入任何标签工具。
        run_pipeline(
            &h,
            None,
            None,
            vec![],
            "go",
            Some(Vec::new()),
            Vec::new(),
            true,
        )
        .await
        .unwrap();
        assert!(wire_names().is_empty(), "非对话调用不应注入任何工具");
    }
}
