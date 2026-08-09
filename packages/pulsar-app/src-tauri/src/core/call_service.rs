use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    assistant_mode::extract_json_object,
    conversation_store::{now_ms, ConversationStore},
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    log_redact::preview_json_for_log,
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        AssistantCandidateScope, ChatModelSelection, ChatResponse, Conversation, ConversationMode,
        EnsureSystemOpts, Message, MessageBody, MessageRole, ModelCallRequest, ModelCallResponse,
        ModelMessage, NeighborhoodPoolPolicy, Neuron, SelectionPolicy, SessionBehavior, ToolPolicy,
    },
    neuron_manager::{default_behavior_for_system_type, NeuronManager},
    providers::ProviderRegistry,
    tool_registry::ToolRegistry,
};

/// 会话级运行态（`conversation.extra.session.state`）：选型/干预信号自 `topic.extra.assistant`
/// 迁出，旧 topic 数据读取时回退兼容。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub last_selected_neuron_id: Option<String>,
    #[serde(default)]
    pub last_intervention_at: Option<u128>,
    #[serde(default)]
    pub intervention_neuron_ids: Vec<String>,
}

/// 会话元数据（`conversation.extra.session`）：规格绑定 + 运行态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub spec_neuron_id: String,
    #[serde(default)]
    pub state: SessionState,
}

const EXTRA_SESSION_KEY: &str = "session";
const EXTRA_STATE_KEY: &str = "state";
const EXTRA_SPEC_NEURON_ID_KEY: &str = "spec_neuron_id";

/// 读取会话运行态（缺失 / 非法回落默认）。
pub fn read_session_state(conversation: &Conversation) -> SessionState {
    conversation
        .extra
        .as_ref()
        .and_then(|extra| extra.get(EXTRA_SESSION_KEY))
        .and_then(|session| session.get(EXTRA_STATE_KEY))
        .and_then(|state| serde_json::from_value(state.clone()).ok())
        .unwrap_or_default()
}

/// 读取会话绑定的规格神经元 id（非规格会话为 None）。
pub fn session_spec_neuron_id(conversation: &Conversation) -> Option<String> {
    conversation
        .extra
        .as_ref()
        .and_then(|extra| extra.get(EXTRA_SESSION_KEY))
        .and_then(|session| session.get(EXTRA_SPEC_NEURON_ID_KEY))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// 将运行态写回 `extra.session.state`（保留其它 extra 键与规格绑定）。
fn set_session_state(conversation: &mut Conversation, state: &SessionState) {
    let mut extra = conversation.extra.take().unwrap_or_else(|| json!({}));
    if !extra.is_object() {
        extra = json!({});
    }
    let session = extra
        .get(EXTRA_SESSION_KEY)
        .cloned()
        .unwrap_or_else(|| json!({ EXTRA_SPEC_NEURON_ID_KEY: "" }));
    let mut session_obj = if session.is_object() {
        session
    } else {
        json!({ EXTRA_SPEC_NEURON_ID_KEY: "" })
    };
    session_obj[EXTRA_STATE_KEY] = serde_json::to_value(state).unwrap_or_default();
    extra[EXTRA_SESSION_KEY] = session_obj;
    conversation.extra = Some(extra);
}

/// 写回会话运行态。
pub fn write_session_state(
    store: &ConversationStore,
    session_id: &str,
    state: &SessionState,
) -> AppResult<()> {
    let mut conversation = store.require_conversation(session_id)?;
    set_session_state(&mut conversation, state);
    store.save_conversation(&conversation)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTrigger {
    UserInput,
    ManualStep,
    Poller,
}

#[derive(Debug, Clone)]
pub struct AssistantRoundContext {
    pub session_id: String,
    pub topic_id: Option<String>,
    /// 轮询 / 手动推进时注入的课题简报（目标、进度、待办清单），避免模型盲目推进。
    pub topic_brief: Option<String>,
    pub trigger: RoundTrigger,
    pub user_input: Option<String>,
    pub system_prompt: Option<String>,
    pub selected_neuron: Option<Neuron>,
    pub authorized_tool_ids: Vec<String>,
    pub messages: Vec<ModelMessage>,
    pub model_output: Option<String>,
    pub tool_result: Option<String>,
    pub poll_count_for_topic: u64,
    pub last_selected_neuron_id: Option<String>,
    /// 会话绑定的规格神经元 id（resolve_round 填充）。
    pub spec_neuron_id: Option<String>,
    /// 会话规格行为（resolve_round 填充；execute_round 读取 insert_id）。
    pub behavior: Option<SessionBehavior>,
}

/// 模型调用抽象：生产用 [`ProviderRegistry`]，测试可注入替身。
#[async_trait]
pub trait ModelCaller: Send + Sync {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse>;
}

#[async_trait]
impl ModelCaller for ProviderRegistry {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse> {
        ProviderRegistry::call_model(self, request).await
    }
}

/// 执行面：会话级「规格 → 选型/授权 → 模型调用 → 单次工具执行 → 会话态落库」闭环。
///
/// 不持有 topic_store：课题相关副作用（match_topic / score_feedback / complete_scope 等）
/// 由调用方（AssistantMode 的 hooks）负责。
pub struct NeuronCallService {
    model_caller: Arc<dyn ModelCaller>,
    neuron_manager: Arc<NeuronManager>,
    store: ConversationStore,
    tool_registry: Arc<RwLock<ToolRegistry>>,
}

impl std::fmt::Debug for NeuronCallService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuronCallService").finish_non_exhaustive()
    }
}

impl NeuronCallService {
    pub fn new(
        model_caller: Arc<dyn ModelCaller>,
        neuron_manager: Arc<NeuronManager>,
        store: ConversationStore,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        Self {
            model_caller,
            neuron_manager,
            store,
            tool_registry,
        }
    }

    /// 开启规格会话：校验规格 + 创建 conversation 并写 `extra.session` 元数据。
    pub fn open_session(
        &self,
        spec_neuron_id: &str,
        mode: ConversationMode,
    ) -> AppResult<Conversation> {
        // 校验目标确实是有效会话规格（system_type + behavior 非空）。
        let spec = self.neuron_manager.get_session_behavior(spec_neuron_id)?;
        let mut conversation = self.store.create_conversation(None, mode)?;
        let mut extra = json!({});
        extra[EXTRA_SESSION_KEY] = json!({
            EXTRA_SPEC_NEURON_ID_KEY: spec_neuron_id,
            EXTRA_STATE_KEY: SessionState::default(),
        });
        conversation.extra = Some(extra);
        self.store.save_conversation(&conversation)?;
        tracing::info!(
            phase = "open_session",
            session_id = %conversation.id,
            spec_neuron_id,
            behavior = ?spec,
            "session opened"
        );
        Ok(conversation)
    }

    /// 一轮端到端（resolve_round → execute_round）。
    pub async fn converse(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let mut ctx = self
            .build_context(session_id, RoundTrigger::UserInput)
            .await?;
        ctx.user_input = Some(input.to_string());
        self.resolve_round(&mut ctx).await?;
        self.execute_round(&mut ctx, model).await
    }

    /// Phase A：规格解析 + role 解析/选型 + 工具授权 + 会话态写回（不调用最终模型）。
    ///
    /// selection 统一语义：
    /// - `None`：role_system 为空，不读任何 content；
    /// - `Fixed`：读规格神经元自己的 content，永不变化（不写 last_selected）；
    /// - `Neighborhood`：锚点 = last_selected（首轮 = 规格神经元自身）邻域选 1；
    /// - `Global`：无历史全域选 1，有历史退化为邻域选。
    pub async fn resolve_round(&self, ctx: &mut AssistantRoundContext) -> AppResult<()> {
        let conversation = self.store.require_conversation(&ctx.session_id)?;
        let spec_neuron_id = session_spec_neuron_id(&conversation).ok_or_else(|| {
            AppError::InvalidInput(format!(
                "session {} is not a spec session (missing extra.session.spec_neuron_id)",
                ctx.session_id
            ))
        })?;
        let spec_neuron = self
            .neuron_manager
            .get(&spec_neuron_id)?
            .ok_or_else(|| AppError::NeuronNotFound(spec_neuron_id.clone()))?;
        let behavior = self.neuron_manager.get_session_behavior(&spec_neuron_id)?;
        ctx.spec_neuron_id = Some(spec_neuron_id.clone());
        ctx.behavior = Some(behavior.clone());

        let mut state = read_session_state(&conversation);

        match &behavior.selection {
            SelectionPolicy::None => {
                // 不选型：role_system 为空；清空历史锚点。
                ctx.system_prompt = None;
                state.last_selected_neuron_id = None;
            }
            SelectionPolicy::Fixed => {
                // 读规格神经元自己的 content，永不变化（不写 last_selected）。
                ctx.system_prompt = Some(spec_neuron.content.clone());
                ctx.selected_neuron = Some(spec_neuron.clone());
            }
            SelectionPolicy::Neighborhood { .. } | SelectionPolicy::Global { .. } => {
                let scope = Self::scope_for_selection(
                    &behavior.selection,
                    &spec_neuron_id,
                    state.last_selected_neuron_id.as_deref(),
                )
                .expect("Neighborhood/Global always produce a scope");
                let role = self.select_role(&ctx.messages, scope).await?;
                state.last_selected_neuron_id = Some(role.id.clone());
                ctx.selected_neuron = Some(role.clone());
                ctx.system_prompt = Some(role.content.clone());
            }
        }

        // 工具授权：按 behavior.tools 三策略（∩ 注册表）。
        let tool_ids = match &behavior.tools {
            ToolPolicy::None => Vec::new(),
            ToolPolicy::FromNeuron => ctx
                .selected_neuron
                .as_ref()
                .map(|n| n.tool_ids.clone())
                .unwrap_or_default(),
            ToolPolicy::Allowlist(list) => list.clone(),
        };
        let guard = self
            .tool_registry
            .read()
            .expect("tool registry lock should not be poisoned");
        ctx.authorized_tool_ids = filter_authorized_tool_ids(&guard, &tool_ids);
        drop(guard);

        write_session_state(&self.store, &ctx.session_id, &state)?;
        Ok(())
    }

    /// selection → 候选池装配 scope（resolve_round 与统一入口 `call_system_prompt` 共用）。
    /// `None` / `Fixed` 不涉及候选池，返回 `None`。
    fn scope_for_selection(
        selection: &SelectionPolicy,
        spec_neuron_id: &str,
        last_selected: Option<&str>,
    ) -> Option<AssistantCandidateScope> {
        match selection {
            SelectionPolicy::None | SelectionPolicy::Fixed => None,
            SelectionPolicy::Neighborhood { policy } => Some(match last_selected {
                // 有历史：锚点 = last_selected。
                Some(last) => AssistantCandidateScope::Neighborhood {
                    self_id: last.to_string(),
                    policy: *policy,
                },
                // 首轮：锚点 = 规格神经元自身（读它自己的邻域）。
                None => AssistantCandidateScope::Neighborhood {
                    self_id: spec_neuron_id.to_string(),
                    policy: *policy,
                },
            }),
            SelectionPolicy::Global { limit } => Some(match last_selected {
                // 有历史：退化为邻域选（锚点 = last_selected）。
                Some(last) => AssistantCandidateScope::Neighborhood {
                    self_id: last.to_string(),
                    policy: NeighborhoodPoolPolicy::default(),
                },
                // 无历史：全域池选 1。
                None => AssistantCandidateScope::Global { limit: *limit },
            }),
        }
    }

    /// role 解析/选型：按 scope 装配候选池；n=1 硬规则短路（跳过选型模型）。
    async fn select_role(
        &self,
        messages: &[ModelMessage],
        scope: AssistantCandidateScope,
    ) -> AppResult<Neuron> {
        let candidates = self
            .neuron_manager
            .select_assistant_candidates(scope)
            .await?;
        if candidates.len() == 1 {
            // n=1 硬规则：候选池仅 1 个 → 跳过选型模型，直接选中并记录使用信号。
            let single = candidates[0].clone();
            self.neuron_manager.mark_used_for_assistant(&single.id);
            return Ok(single);
        }
        self.neuron_manager
            .select_one_from_with_history(&candidates, messages)
            .await
    }

    /// 统一系统提示词入口：懒创建（裁决类神经元）→ 读 behavior（无 behavior 回落默认）→
    /// selection 取 role_system（`None` 空 / `Fixed` 自己 content / `Neighborhood`/`Global`
    /// 按无历史锚点走选型，选中者 content 即 role_system）→ insert_id 有则拼契约段
    /// （`Manual` 模板，`tools: None`）→ `call_model` → `require_json` 时 `extract_json_object`。
    ///
    /// 取代旧 `assistant_mode::call_system_prompt_json` 与 `insert_id_for_system_type`：
    /// 管理面与裁决类系统神经元统一走此入口，语义与 `resolve_round` 同款。
    pub async fn call_system_prompt(
        &self,
        system_type: &str,
        user_payload: serde_json::Value,
        model: &ChatModelSelection,
        history: &[ModelMessage],
        require_json: bool,
    ) -> AppResult<serde_json::Value> {
        let user_preview = preview_json_for_log(&user_payload, 240);
        tracing::info!(
            phase = "call_system_prompt",
            system_type,
            user_payload = %user_preview,
            "call_system_prompt start"
        );

        let prompt_neuron = self
            .neuron_manager
            .ensure_system_neuron(system_type, EnsureSystemOpts { reset: false })
            .await?;
        // 无 behavior（旧库已存在）回落默认（裁决类 = Fixed + 各自 insert_id）。
        let behavior = prompt_neuron
            .behavior
            .clone()
            .or_else(|| default_behavior_for_system_type(system_type))
            .ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "system_type={system_type} has no behavior and no default behavior mapping"
                ))
            })?;

        // selection → role_system（此入口无会话态：Neighborhood/Global 按无历史锚点选型）。
        let role_system = match &behavior.selection {
            SelectionPolicy::None => String::new(),
            SelectionPolicy::Fixed => prompt_neuron.content.clone(),
            SelectionPolicy::Neighborhood { .. } | SelectionPolicy::Global { .. } => {
                let scope = Self::scope_for_selection(&behavior.selection, &prompt_neuron.id, None)
                    .expect("Neighborhood/Global always produce a scope");
                let role = self.select_role(history, scope).await?;
                role.content.clone()
            }
        };

        let (template, insert_or_empty) = match behavior.insert_id.as_ref() {
            Some(insert_id) => (
                ModelAppendTemplate::Manual,
                InsertCatalog::require(insert_id),
            ),
            None => (ModelAppendTemplate::Neuron, ""),
        };
        let messages = ModelCallInput::assemble(
            history,
            &role_system,
            insert_or_empty,
            &user_payload.to_string(),
            template,
        );
        let response = self
            .model_caller
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages,
                tools: None,
            })
            .await?;

        if require_json {
            extract_json_object(&response.output)
        } else {
            Ok(serde_json::Value::String(response.output))
        }
    }

    /// Phase B：user_input/nudge 落库 + 拼接（insert_id 有无推导模板）+ 模型调用 +
    /// 单次工具执行 + 消息落库。
    pub async fn execute_round(
        &self,
        ctx: &mut AssistantRoundContext,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse> {
        let role_system = ctx.system_prompt.clone().unwrap_or_default();
        let user_input = if let Some(user_input) = ctx.user_input.clone() {
            let user_message = Message {
                role: MessageRole::User,
                body: MessageBody::Text {
                    content: user_input.clone(),
                },
                timestamp: now_ms(),
            };
            self.store.add_message(&ctx.session_id, user_message)?;
            user_input
        } else if matches!(ctx.trigger, RoundTrigger::ManualStep | RoundTrigger::Poller) {
            // 轮询 / 手动推进：注入课题简报，让模型明确目标、进度与待办，避免盲目推进。
            let brief = ctx.topic_brief.clone().unwrap_or_else(|| {
                "Continue advancing the bound topic using available tools if needed.".to_string()
            });
            // 轮询简报落库为 nudge（role=User, kind=nudge）：记录本轮发给模型的输入，
            // 保证历史因果链完整；不参与后续模型输入组装（message_to_model 过滤）。
            if matches!(ctx.trigger, RoundTrigger::Poller) {
                let nudge = Message {
                    role: MessageRole::User,
                    body: MessageBody::Nudge {
                        content: brief.clone(),
                    },
                    timestamp: now_ms(),
                };
                self.store.add_message(&ctx.session_id, nudge)?;
            }
            brief
        } else {
            String::new()
        };

        // 拼接规则由 insert_id 有无推导（不进配置）：有 → Manual 契约段；无 → Neuron 角色拼接。
        let (template, insert_or_empty) =
            match ctx.behavior.as_ref().and_then(|b| b.insert_id.clone()) {
                Some(insert_id) => (
                    ModelAppendTemplate::Manual,
                    InsertCatalog::require(&insert_id),
                ),
                None => (ModelAppendTemplate::Neuron, ""),
            };
        let messages = ModelCallInput::assemble(
            &ctx.messages,
            &role_system,
            insert_or_empty,
            &user_input,
            template,
        );

        let tools = if ctx.authorized_tool_ids.is_empty() {
            None
        } else {
            Some(
                self.tool_registry
                    .read()
                    .map(|reg| reg.definitions_for(&ctx.authorized_tool_ids))
                    .unwrap_or_default(),
            )
        };

        tracing::info!(
            phase = "neuron_call_execute",
            session_id = %ctx.session_id,
            provider = %model.provider_id,
            model = %model.model_id,
            message_count = messages.len(),
            tool_defs = tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "model call start"
        );
        let model_response = match self
            .model_caller
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages: messages.clone(),
                tools,
            })
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(
                    phase = "neuron_call_execute",
                    error_code = error.code(),
                    error = %error,
                    "model call failed"
                );
                return Err(error);
            }
        };
        tracing::info!(
            phase = "neuron_call_execute",
            output_len = model_response.output.len(),
            tool_calls = model_response
                .tool_calls
                .as_ref()
                .map(|c| c.len())
                .unwrap_or(0),
            "model call ok"
        );

        let mut output = model_response.output.clone();
        let mut tool_result = None;

        if let Some(tool_calls) = model_response.tool_calls.clone() {
            if let Some(first) = tool_calls.first() {
                if !ctx.authorized_tool_ids.iter().any(|id| id == &first.name) {
                    return Err(AppError::InvalidInput(format!(
                        "Tool '{}' is not authorized for selected neuron",
                        first.name
                    )));
                }
                tracing::info!(
                    phase = "neuron_call_execute",
                    tool = %first.name,
                    "tool execute start"
                );
                // 读锁内仅 clone 工具引用（释放锁后再 await execute，锁不跨 await）。
                let tool = self
                    .tool_registry
                    .read()
                    .ok()
                    .and_then(|reg| reg.get_tool(&first.name));
                let result = match tool {
                    Some(tool) => tool.execute(first.arguments.clone()).await?,
                    None => return Err(AppError::SkillNotFound(first.name.clone())),
                };
                tracing::info!(
                    phase = "neuron_call_execute",
                    tool = %first.name,
                    result_len = result.len(),
                    "tool execute ok"
                );
                tool_result = Some(result.clone());
                let tool_msg = Message {
                    role: MessageRole::Assistant,
                    body: MessageBody::ToolCall {
                        content: output.clone(),
                        tool_calls: vec![first.clone()],
                    },
                    timestamp: now_ms(),
                };
                self.store.add_message(&ctx.session_id, tool_msg)?;
                let result_msg = Message {
                    role: MessageRole::Tool,
                    body: MessageBody::ToolResult {
                        tool_call_id: first.id.clone(),
                        tool_name: first.name.clone(),
                        content: result.clone(),
                    },
                    timestamp: now_ms(),
                };
                self.store.add_message(&ctx.session_id, result_msg)?;
                output = if output.trim().is_empty() {
                    result
                } else {
                    format!("{output}\n\n[tool:{name}] {result}", name = first.name)
                };
            }
        } else {
            let assistant_msg = Message {
                role: MessageRole::Assistant,
                body: MessageBody::Text {
                    content: output.clone(),
                },
                timestamp: now_ms(),
            };
            self.store.add_message(&ctx.session_id, assistant_msg)?;
        }

        ctx.model_output = Some(output.clone());
        ctx.tool_result = tool_result;

        Ok(ChatResponse {
            conversation_id: ctx.session_id.clone(),
            response: output,
        })
    }

    /// 纯会话层面的上下文构建（无 topic 信息；课题字段由 AssistantMode.build_context 补充）。
    ///
    /// pub：Gateway 层组装 Chat（execute_round 退化形态）与 Agent（多轮 execute_round）
    /// 时复用同一套历史读取。
    pub async fn build_context(
        &self,
        session_id: &str,
        trigger: RoundTrigger,
    ) -> AppResult<AssistantRoundContext> {
        let conversation = self.store.require_conversation(session_id)?;
        let state = read_session_state(&conversation);
        let messages = ModelCallInput::sanitize_tool_pairs(
            &conversation
                .messages
                .iter()
                .filter_map(message_to_model)
                .collect::<Vec<_>>(),
        );
        Ok(AssistantRoundContext {
            session_id: session_id.to_string(),
            topic_id: None,
            topic_brief: None,
            trigger,
            user_input: None,
            system_prompt: None,
            selected_neuron: None,
            authorized_tool_ids: Vec::new(),
            messages,
            model_output: None,
            tool_result: None,
            poll_count_for_topic: 0,
            last_selected_neuron_id: state.last_selected_neuron_id.clone(),
            spec_neuron_id: None,
            behavior: None,
        })
    }
}

/// 工具白名单 ∩ 注册表：仅授权真实存在的工具。
pub fn filter_authorized_tool_ids(registry: &ToolRegistry, tool_ids: &[String]) -> Vec<String> {
    let known: std::collections::HashSet<String> = registry
        .list_definitions()
        .into_iter()
        .map(|d| d.name)
        .collect();
    let mut out = Vec::new();
    for id in tool_ids {
        if known.contains(id) {
            out.push(id.clone());
        } else {
            eprintln!("assistant ignoring unknown tool id: {id}");
        }
    }
    out
}

pub(crate) fn message_to_model(message: &Message) -> Option<ModelMessage> {
    match &message.body {
        // Compaction 摘要按 System 角色携带（与 engine 对齐），避免长会话压缩后丢失上下文。
        MessageBody::Compaction { content, .. } => Some(ModelMessage {
            role: crate::core::models::ModelMessageRole::System,
            content: format!("[Previous conversation summary]: {content}"),
            tool_calls: None,
            tool_call_id: None,
        }),
        // 工具结果必须按 tool 角色发送，否则 OpenAI 兼容接口（如 DeepSeek）会以
        // 「tool_calls 后缺少 tool 消息」拒绝请求。
        MessageBody::ToolResult {
            tool_call_id,
            content,
            ..
        } => Some(ModelMessage {
            role: crate::core::models::ModelMessageRole::Tool,
            content: content.clone(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.clone()),
        }),
        MessageBody::ToolCall {
            content,
            tool_calls,
        } => Some(ModelMessage {
            role: crate::core::models::ModelMessageRole::Assistant,
            content: content.clone(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
        }),
        MessageBody::Text { content } => {
            let role = match message.role {
                MessageRole::User => crate::core::models::ModelMessageRole::User,
                // Tool 角色不会携带 Text 正文（Tool 只对应 ToolResult），兜底按 Assistant 发送。
                MessageRole::Assistant | MessageRole::Tool => {
                    crate::core::models::ModelMessageRole::Assistant
                }
                MessageRole::System => crate::core::models::ModelMessageRole::System,
                MessageRole::Compaction => unreachable!("handled above"),
            };
            Some(ModelMessage {
                role,
                content: content.clone(),
                tool_calls: None,
                tool_call_id: None,
            })
        }
        // 轮询简报（nudge）仅作审计/展示/压缩记录，不拼回后续模型输入，
        // 避免历史简报反复进 context 造成膨胀。
        MessageBody::Nudge { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex as StdMutex,
        },
    };

    use rusqlite::Connection as SqliteConnection;

    use crate::core::{
        models::{
            EnsureSystemOpts, ModelMessage, ModelMessageRole, NeighborhoodPoolPolicy, NeuronCreate,
            ToolSource,
        },
        neuron_config::NeuronConfigReader,
        neuron_model::NeuronModelCaller,
        neuron_store::NeuronStore,
        tool_registry::Tool,
    };

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo tool"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({})
        }
        async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
            Ok(format!("echo:{args}"))
        }
    }

    struct CalculateTool;
    #[async_trait]
    impl Tool for CalculateTool {
        fn name(&self) -> &str {
            "calculate"
        }
        fn description(&self) -> &str {
            "calculate tool"
        }
        fn parameters(&self) -> serde_json::Value {
            json!({})
        }
        async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
            Ok("42".into())
        }
    }

    /// 选型/生成模型替身：选型时返回候选第一个 id；创建时按 prompt 中 "exactly N" 返回 N 个草稿。
    struct MockSelector {
        calls: Arc<AtomicUsize>,
    }

    /// 拼接 System|User 消息文本（assemble 在 history 为空时会把内容折叠成单条 System）。
    fn mock_prompt_blob(messages: &[ModelMessage]) -> String {
        messages
            .iter()
            .filter(|m| matches!(m.role, ModelMessageRole::System | ModelMessageRole::User))
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[async_trait]
    impl NeuronModelCaller for MockSelector {
        async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            let blob = mock_prompt_blob(&messages);
            // 选型路径：prompt 内嵌 candidates 数组 → 返回第一个候选。
            if let Some(idx) = blob.find(r#""candidates"#) {
                let rest = &blob[idx..];
                if let Some(start) = rest.find(r#""id":""#) {
                    let tail = &rest[start + 6..];
                    if let Some(end) = tail.find('"') {
                        return Ok(format!(r#"{{"neuron_id":"{}"}}"#, &tail[..end]));
                    }
                }
            }
            // 创建路径：按 "exactly N" 返回 N 个草稿（供 bootstrap fill / 池扩充）。
            let count = blob
                .split("exactly ")
                .nth(1)
                .and_then(|rest| {
                    rest.split_whitespace().next().and_then(|token| {
                        token
                            .trim_matches(|c: char| !c.is_ascii_digit())
                            .parse()
                            .ok()
                    })
                })
                .unwrap_or(1usize);
            if count <= 1 {
                return Ok(format!(
                    r#"{{"desc":"generated-{call}","content":"content-{call}","weight":1.0,"tool_ids":[]}}"#
                ));
            }
            let items: Vec<String> = (0..count)
                .map(|i| {
                    format!(
                        r#"{{"desc":"generated-{call}-{i}","content":"content-{call}-{i}","weight":1.0,"tool_ids":[]}}"#
                    )
                })
                .collect();
            Ok(format!("[{}]", items.join(",")))
        }
    }

    /// 最终模型调用替身：固定输出，无工具调用。
    struct FakeModelCaller {
        output: String,
    }

    #[async_trait]
    impl ModelCaller for FakeModelCaller {
        async fn call_model(&self, _request: ModelCallRequest) -> AppResult<ModelCallResponse> {
            Ok(ModelCallResponse {
                provider_id: "fake".into(),
                model_id: "fake".into(),
                output: self.output.clone(),
                tool_calls: None,
                finish_reason: "stop".into(),
            })
        }
    }

    /// 统一入口替身：输出可提取的 JSON。
    struct JsonModelCaller;

    #[async_trait]
    impl ModelCaller for JsonModelCaller {
        async fn call_model(&self, _request: ModelCallRequest) -> AppResult<ModelCallResponse> {
            Ok(ModelCallResponse {
                provider_id: "fake".into(),
                model_id: "fake".into(),
                output: r#"{"action":"ok"}"#.into(),
                tool_calls: None,
                finish_reason: "stop".into(),
            })
        }
    }

    struct Harness {
        manager: Arc<NeuronManager>,
        store: ConversationStore,
        service: Arc<NeuronCallService>,
        selector_calls: Arc<AtomicUsize>,
    }

    fn harness() -> Harness {
        let conn = Arc::new(StdMutex::new(SqliteConnection::open_in_memory().unwrap()));
        let store = Arc::new(StdMutex::new(NeuronStore::new(conn)));
        store.lock().unwrap().init_table().unwrap();
        let root = std::env::temp_dir().join(format!(
            "pulsar-call-service-{}",
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
        let manager = Arc::new(NeuronManager::new(
            store,
            Arc::new(MockSelector {
                calls: Arc::clone(&selector_calls),
            }),
            NeuronConfigReader::new(root.clone()),
            Arc::new(RwLock::new(ToolRegistry::new())),
        ));
        let conv_store = ConversationStore::new(root.clone()).unwrap();
        let registry = {
            let mut reg = ToolRegistry::new();
            reg.register_source(EchoTool, ToolSource::Config);
            reg.register_source(CalculateTool, ToolSource::Config);
            Arc::new(RwLock::new(reg))
        };
        let service = Arc::new(NeuronCallService::new(
            Arc::new(FakeModelCaller {
                output: "fake-output".into(),
            }),
            Arc::clone(&manager),
            conv_store.clone(),
            Arc::clone(&registry),
        ));
        Harness {
            manager,
            store: conv_store,
            service,
            selector_calls,
        }
    }

    fn insert_plain(manager: &NeuronManager, desc: &str, tool_ids: Vec<String>) -> String {
        manager
            .create_plain(
                NeuronCreate {
                    desc: desc.into(),
                    content: format!("{desc} content"),
                    tool_ids,
                    ..Default::default()
                },
                None,
            )
            .unwrap()
            .id
    }

    async fn ensure_spec(
        manager: &NeuronManager,
        system_type: &str,
        behavior: SessionBehavior,
    ) -> String {
        manager
            .ensure_session_neuron(
                system_type,
                behavior,
                None,
                EnsureSystemOpts { reset: false },
            )
            .await
            .unwrap()
            .id
    }

    async fn open_ctx(h: &Harness, spec_id: &str) -> (Conversation, AssistantRoundContext) {
        let conversation = h
            .service
            .open_session(spec_id, ConversationMode::Chat)
            .unwrap();
        let ctx = h
            .service
            .build_context(&conversation.id, RoundTrigger::UserInput)
            .await
            .unwrap();
        (conversation, ctx)
    }

    #[test]
    fn behavior_json_roundtrip() {
        let behavior = SessionBehavior {
            selection: SelectionPolicy::Neighborhood {
                policy: NeighborhoodPoolPolicy::default(),
            },
            tools: ToolPolicy::Allowlist(vec!["echo".into()]),
            insert_id: Some("insert.ref".into()),
        };
        let json = serde_json::to_string(&behavior).unwrap();
        let back: SessionBehavior = serde_json::from_str(&json).unwrap();
        assert_eq!(behavior, back);
        // 缺失字段回落默认（None selection + None tools + None insert_id）。
        let minimal: SessionBehavior = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(minimal, SessionBehavior::default());
        assert!(matches!(minimal.selection, SelectionPolicy::None));
    }

    #[test]
    fn selection_policy_legacy_json_parses_tolerantly() {
        // 旧 `Fixed{neuron_id}`：回落新 Fixed（读自己 content，忽略旧目标 id）。
        let fixed: SelectionPolicy =
            serde_json::from_str(r#"{"Fixed": {"neuron_id": "n_legacy"}}"#).unwrap();
        assert_eq!(fixed, SelectionPolicy::Fixed);
        // 旧 `Global{limit, switching}`：忽略 switching。
        let global: SelectionPolicy =
            serde_json::from_str(r#"{"Global": {"limit": 5, "switching": "Reelect"}}"#).unwrap();
        assert_eq!(global, SelectionPolicy::Global { limit: 5 });
        // 旧 `Neighborhood{policy, switching}`：忽略 switching。
        let neighborhood: SelectionPolicy = serde_json::from_str(
            r#"{"Neighborhood": {"policy": {"existing_downstream": 2, "new_downstream": 1, "fill_downstream_shortage": true, "siblings": 1, "upstream_depth": 1, "global_top_weight": 0}, "switching": "Conditional"}}"#,
        )
        .unwrap();
        assert!(matches!(
            neighborhood,
            SelectionPolicy::Neighborhood { policy } if policy.existing_downstream == 2
        ));
        // 单元字符串形态仍可解析。
        assert_eq!(
            serde_json::from_str::<SelectionPolicy>(r#""Fixed""#).unwrap(),
            SelectionPolicy::Fixed
        );
    }

    #[test]
    fn filter_drops_unknown_tool_ids() {
        let registry = ToolRegistry::new();
        let filtered = filter_authorized_tool_ids(
            &registry,
            &["echo".into(), "missing_tool".into(), "calculate".into()],
        );
        assert!(filtered.is_empty());
    }

    #[test]
    fn session_state_roundtrip_via_extra() {
        let mut conversation = Conversation {
            id: "cv_t".into(),
            mode: ConversationMode::Chat,
            messages: vec![],
            created_at: 1,
            updated_at: 1,
            extra: Some(json!({"session": {"spec_neuron_id": "n_spec", "state": {}}})),
        };
        let mut state = read_session_state(&conversation);
        assert!(state.last_selected_neuron_id.is_none());
        state.last_selected_neuron_id = Some("n_selected".into());
        set_session_state(&mut conversation, &state);
        let roundtrip = read_session_state(&conversation);
        assert_eq!(
            roundtrip.last_selected_neuron_id.as_deref(),
            Some("n_selected")
        );
        // 其它 extra 键保留
        assert_eq!(
            session_spec_neuron_id(&conversation).as_deref(),
            Some("n_spec")
        );
    }

    #[tokio::test]
    async fn resolve_round_n1_shortcircuits_selection() {
        let h = harness();
        // 不 bootstrap：候选池仅 1 个普通神经元 → 命中 n=1 硬规则（短路跳过选型模型）。
        let behavior = SessionBehavior {
            selection: SelectionPolicy::Global { limit: 1 },
            tools: ToolPolicy::None,
            insert_id: None,
        };
        let spec_id = ensure_spec(&h.manager, "session.n1", behavior).await;
        let solo_id = insert_plain(&h.manager, "solo", vec![]);
        let (conversation, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        let selected = ctx
            .selected_neuron
            .as_ref()
            .expect("n=1 must select the solo neuron");
        assert_eq!(selected.desc, "solo");
        // 未调用选型模型。
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0);
        // 会话态写回 last_selected。
        let cv = h.store.require_conversation(&conversation.id).unwrap();
        let state = read_session_state(&cv);
        assert_eq!(
            state.last_selected_neuron_id.as_deref(),
            Some(selected.id.as_str())
        );
        assert_eq!(
            state.last_selected_neuron_id.as_deref(),
            Some(solo_id.as_str())
        );
    }

    #[tokio::test]
    async fn fixed_policy_reads_spec_neuron_own_content_and_never_selects() {
        let h = harness();
        let behavior = SessionBehavior {
            selection: SelectionPolicy::Fixed,
            tools: ToolPolicy::None,
            insert_id: None,
        };
        let spec_id = h
            .manager
            .ensure_session_neuron(
                "session.fixed",
                behavior.clone(),
                Some("固定提示词".into()),
                EnsureSystemOpts { reset: false },
            )
            .await
            .unwrap()
            .id;
        let (conversation, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        // role_system = 规格神经元自己的 content；selected_neuron = 规格神经元自身。
        assert_eq!(ctx.system_prompt.as_deref(), Some("固定提示词"));
        assert_eq!(
            ctx.selected_neuron.as_ref().map(|n| n.id.as_str()),
            Some(spec_id.as_str())
        );
        // Fixed 不写 last_selected；不调用选型模型。
        let cv = h.store.require_conversation(&conversation.id).unwrap();
        let state = read_session_state(&cv);
        assert!(state.last_selected_neuron_id.is_none());
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn global_policy_first_round_global_then_neighborhood() {
        let h = harness();
        let behavior = SessionBehavior {
            selection: SelectionPolicy::Global { limit: 5 },
            tools: ToolPolicy::None,
            insert_id: None,
        };
        let spec_id = ensure_spec(&h.manager, "session.global", behavior).await;
        insert_plain(&h.manager, "alpha", vec![]);
        insert_plain(&h.manager, "beta", vec![]);
        let (conversation, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        // 无历史：全域池选 1 → 触发选型；last_selected 写回会话态 = 首轮选中者。
        let round1 = h.selector_calls.load(Ordering::Relaxed);
        assert!(round1 >= 1);
        let cv = h.store.require_conversation(&conversation.id).unwrap();
        let state = read_session_state(&cv);
        assert_eq!(
            state.last_selected_neuron_id.as_deref(),
            ctx.selected_neuron.as_ref().map(|n| n.id.as_str())
        );
        // 次轮：有历史 → 退化为邻域选（锚点 = last_selected），再次触发选型（非 Fixed 复用）。
        let mut ctx2 = h
            .service
            .build_context(&conversation.id, RoundTrigger::UserInput)
            .await
            .unwrap();
        h.service.resolve_round(&mut ctx2).await.unwrap();
        assert!(h.selector_calls.load(Ordering::Relaxed) > round1);
    }

    #[tokio::test]
    async fn tool_policy_three_modes() {
        let h = harness();
        // 规格神经元自身挂工具白名单：echo（注册）+ missing（未注册）。
        // Fixed 选型下 selected = 规格神经元自身 → 直接验证三策略授权。
        let tool_ids = vec!["echo".into(), "missing".into()];

        // 1) None → 不授权。
        let spec_id = ensure_spec(
            &h.manager,
            "session.tools_none",
            SessionBehavior {
                selection: SelectionPolicy::Fixed,
                tools: ToolPolicy::None,
                insert_id: None,
            },
        )
        .await;
        h.manager
            .set_tool_ids_for_admin(&spec_id, tool_ids.clone())
            .unwrap();
        let (_, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        assert!(ctx.authorized_tool_ids.is_empty());

        // 2) FromNeuron → spec.tool_ids ∩ 注册表。
        let spec_id = ensure_spec(
            &h.manager,
            "session.tools_from_neuron",
            SessionBehavior {
                selection: SelectionPolicy::Fixed,
                tools: ToolPolicy::FromNeuron,
                insert_id: None,
            },
        )
        .await;
        h.manager
            .set_tool_ids_for_admin(&spec_id, tool_ids.clone())
            .unwrap();
        let (_, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        assert_eq!(ctx.authorized_tool_ids, vec!["echo".to_string()]);

        // 3) Allowlist → 白名单 ∩ 注册表。
        let spec_id = ensure_spec(
            &h.manager,
            "session.tools_allowlist",
            SessionBehavior {
                selection: SelectionPolicy::Fixed,
                tools: ToolPolicy::Allowlist(vec!["echo".into(), "calculate".into()]),
                insert_id: None,
            },
        )
        .await;
        h.manager
            .set_tool_ids_for_admin(&spec_id, tool_ids)
            .unwrap();
        let (_, mut ctx) = open_ctx(&h, &spec_id).await;
        h.service.resolve_round(&mut ctx).await.unwrap();
        assert_eq!(
            ctx.authorized_tool_ids,
            vec!["echo".to_string(), "calculate".to_string()]
        );
    }

    #[tokio::test]
    async fn converse_roundtrip_persists_messages_and_state() {
        let h = harness();
        h.manager.bootstrap().await.unwrap();
        let spec_id = ensure_spec(
            &h.manager,
            "session.e2e",
            SessionBehavior {
                selection: SelectionPolicy::None,
                tools: ToolPolicy::None,
                insert_id: None,
            },
        )
        .await;
        let (conversation, _) = open_ctx(&h, &spec_id).await;
        let model = ChatModelSelection {
            provider_id: "p".into(),
            model_id: "m".into(),
        };
        let response = h
            .service
            .converse(&conversation.id, "hello", &model)
            .await
            .unwrap();
        assert_eq!(response.response, "fake-output");
        assert_eq!(response.conversation_id, conversation.id);
        // 消息落库：user + assistant。
        let cv = h.store.require_conversation(&conversation.id).unwrap();
        assert_eq!(cv.messages.len(), 2);
        // 会话态（extra.session.state）已初始化。
        assert!(session_spec_neuron_id(&cv).is_some());
    }

    #[tokio::test]
    async fn call_system_prompt_unified_entry() {
        let h = harness();
        let registry = {
            let mut reg = ToolRegistry::new();
            reg.register_source(EchoTool, ToolSource::Config);
            Arc::new(RwLock::new(reg))
        };
        let service = Arc::new(NeuronCallService::new(
            Arc::new(JsonModelCaller),
            Arc::clone(&h.manager),
            h.store.clone(),
            Arc::clone(&registry),
        ));
        let model = ChatModelSelection {
            provider_id: "p".into(),
            model_id: "m".into(),
        };
        let value = service
            .call_system_prompt(
                "assistant_match_topic",
                json!({ "user_input": "hi" }),
                &model,
                &[],
                true,
            )
            .await
            .unwrap();
        assert_eq!(value, json!({ "action": "ok" }));
        // 裁决类神经元被懒创建，且 behavior = Fixed + insert_id（创建即注册默认）。
        let neuron = h
            .manager
            .get_by_system_type("assistant_match_topic")
            .unwrap()
            .unwrap();
        let behavior = neuron
            .behavior
            .as_ref()
            .expect("default behavior registered on creation");
        assert_eq!(behavior.selection, SelectionPolicy::Fixed);
        assert_eq!(behavior.insert_id.as_deref(), Some("assistant.match_topic"));
        // 无 behavior（旧库）回落默认映射，同样可调用。
        h.manager.delete_for_admin(&neuron.id).unwrap();
        let value2 = service
            .call_system_prompt(
                "assistant_match_topic",
                json!({ "user_input": "hi2" }),
                &model,
                &[],
                true,
            )
            .await
            .unwrap();
        assert_eq!(value2, json!({ "action": "ok" }));
        // require_json = false 时原样返回字符串。
        let value3 = service
            .call_system_prompt(
                "assistant_match_topic",
                json!({ "user_input": "hi3" }),
                &model,
                &[],
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            value3,
            serde_json::Value::String(r#"{"action":"ok"}"#.into())
        );
    }
}
