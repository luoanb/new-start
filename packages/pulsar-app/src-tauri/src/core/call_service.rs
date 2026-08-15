use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{
    conversation_store::ConversationStore,
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{
        ChatModelSelection, Conversation, ModelCallRequest, ModelCallResponse,
        ModelMessage, Message, MessageBody, MessageRole, NeighborhoodPoolPolicy, Neuron,
        SelectionPolicy, SessionBehavior, ToolCall, ToolPolicy, ToolTag,
        DEFAULT_ASSISTANT_GLOBAL_LIMIT,
    },
    neuron_manager::NeuronManager,
    providers::ProviderRegistry,
    tool_registry::ToolRegistry,
};

/// 直连（无发起神经元）系统类型标记：仅用于日志/审计，不落库。
pub const SYSTEM_TYPE_DIRECT: &str = "direct";

/// 会话级运行态（`conversation.extra.session.state`）：选型锚点自 `topic.extra.assistant`
/// 迁出，旧 topic 数据读取时回退兼容。
///
/// 已废弃（由消息盖章推导替代）：`last_intervention_at` / `intervention_neuron_ids`
/// 曾在会话态滚动累积"干预窗口"；现改为每条 assistant 产物落库盖章选中神经元
/// （`Message.neuron_id`），评分区间由 `interval_neuron_ids` 按消息介入边界推导。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub last_selected_neuron_id: Option<String>,
    /// 首轮选中 neuron.content 冻结后的稳定系统提示词（B2 方案）。
    /// 有值后 `resolve_role` 返回此值而非新选中 neuron.content。
    #[serde(default)]
    pub stable_system_prompt: Option<String>,
    /// 标记是否已完成首轮冻结（`stable_system_prompt` 已写入）。
    #[serde(default)]
    pub stable_system_frozen: bool,
}

/// 会话元数据（`conversation.extra.session`）：发起神经元绑定 + 种子 + 运行态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub spec_neuron_id: String,
    #[serde(default)]
    pub state: SessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<SessionSeed>,
}

/// 会话种子：决定首轮选型起点与推进规则。
///
/// - `Global`：全域首轮选 1 → 写 `state.last_selected`；后续按领域推进。
/// - `Neuron(id)`：系统神经元用 behavior（禁 Global，宽容回退 Neighborhood）；
///   普通神经元推导默认领域行为。
/// - `None`（缺省）：直连，不选型、role_system 为空。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum SessionSeed {
    Global,
    Neuron(String),
}

const EXTRA_SESSION_KEY: &str = "session";
const EXTRA_STATE_KEY: &str = "state";
const EXTRA_SPEC_NEURON_ID_KEY: &str = "spec_neuron_id";
const EXTRA_SEED_KEY: &str = "seed";

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

/// 读取会话绑定的发起神经元 id（未绑定发起神经元时返回 None）。
pub fn session_spec_neuron_id(conversation: &Conversation) -> Option<String> {
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
pub fn session_seed(conversation: &Conversation) -> Option<SessionSeed> {
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
pub fn write_session_state(
    store: &ConversationStore,
    session_id: &str,
    state: &SessionState,
) -> AppResult<()> {
    let mut conversation = store.require_conversation(session_id)?;
    set_session_state(&mut conversation, state);
    store.save_conversation(&conversation)
}

/// 单轮对话输入：全部显式传入，service 不读库、不写库、不感知会话。
#[derive(Debug, Clone)]
pub struct RoundInput {
    /// 种子分派起点；`None` = 直连（不选型）。
    pub seed: Option<SessionSeed>,
    /// 会话运行态（last_selected 等），上层传入。
    pub state: SessionState,
    /// 历史消息（模型侧，sanitize 后）。
    pub messages: Vec<ModelMessage>,
    /// 工具授权覆盖（`None` → 按 seed/behavior 推导；Agent 传全部工具）。
    pub tool_override: Option<Vec<String>>,
    /// 本轮是否进行选型：`true` 按 seed/behavior 原规则走 LLM 选型；`false` 不选型，
    /// 优先沿用 `last_selected_neuron_id` 锚点（锚点缺失仍回退选型）。
    /// 仅影响真正调 LLM 的分支（Global / 邻选），Fixed / None 分支不感知。
    /// 频率策略由调用方算好传入（业务层按推进轮次取模），引擎不持有任何轮次/频率概念。
    pub reselect: bool,
    /// 本调用自动并入的标签工具（如 `ConversationMode::tool_tags()` 算好的 Core / System）。
    /// 空 = 不注入任何标签工具（Chat 对话、内部裁决等），完全沿用 tool_override / behavior。
    pub tool_tags: Vec<ToolTag>,
}

/// 单轮对话产物：仅模型侧结果；落库由上层（ConversationRunner）负责。
#[derive(Debug, Clone)]
pub struct RoundOutcome {
    /// 最终文本（含工具结果拼接），返回给用户。
    pub response: String,
    /// 模型原始输出（tool_call 消息落库用）。
    pub model_output: Option<String>,
    /// 模型本轮声明的工具调用（含参数，上层落 tool_call 消息用）。
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 首个工具执行结果。
    pub tool_result: Option<String>,
    pub selected_neuron_id: Option<String>,
    pub state: SessionState,
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

/// 执行面：无状态单轮对话引擎（种子分派 → 选型/授权 → 模型调用 → 单次工具执行）。
///
/// 不持有 ConversationStore / TopicStore：读会话、落库、课题副作用均由上层
/// （`ConversationRunner` / 各业务 session 文件）负责。
pub struct NeuronCallService {
    model_caller: Arc<dyn ModelCaller>,
    neuron_manager: Arc<NeuronManager>,
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
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self {
        Self {
            model_caller,
            neuron_manager,
            tool_registry,
        }
    }

    /// 单轮对话：`resolve_role`（种子分派/选型/工具授权）→ 组装模型输入 → 调用模型 →
    /// 单次工具执行 → `RoundOutcome`。全程无状态：`state` 进、`state` 出。
    pub async fn converse(
        &self,
        input: RoundInput,
        model_input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<RoundOutcome> {
        let mut state = input.state;
        tracing::info!(
            phase = "service_converse",
            seed = ?input.seed,
            tool_tags = ?input.tool_tags,
            reselect = input.reselect,
            history = input.messages.len(),
            input_len = model_input.len(),
            "converse start"
        );
        let (selected_neuron, role_system, behavior) = self
            .resolve_role(
                input.seed.as_ref(),
                &mut state,
                &input.messages,
                input.reselect,
            )
            .await?;
        tracing::info!(
            phase = "service_converse",
            selected_neuron_id = ?selected_neuron.as_ref().map(|n| n.id.as_str()),
            role_len = role_system.len(),
            stable_frozen = state.stable_system_frozen,
            "resolve_role done"
        );

        // 工具授权：override 优先；否则按 behavior.tools 三策略（∩ 注册表）。
        let tool_ids = match input.tool_override {
            Some(ids) => ids,
            None => match behavior.as_ref().map(|b| &b.tools) {
                Some(ToolPolicy::None) | None => Vec::new(),
                Some(ToolPolicy::FromNeuron) => selected_neuron
                    .as_ref()
                    .map(|n| n.tool_ids.clone())
                    .unwrap_or_default(),
                Some(ToolPolicy::Allowlist(list)) => list.clone(),
            },
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
            for tag in &input.tool_tags {
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
            phase = "service_converse",
            authorized_tool_count = authorized_tool_ids.len(),
            wire_tool_ids = ?tools.as_ref().map(|t| t.iter().map(|d| d.name.clone()).collect::<Vec<_>>()),
            "tools authorized"
        );

        // 模板由 insert_id 有无推导：有 → 操作说明书契约段；无 → 神经元角色段。
        let (template, insert_or_empty) = match behavior.as_ref().and_then(|b| b.insert_id.clone()) {
            Some(insert_id) => (
                ModelAppendTemplate::Manual,
                InsertCatalog::require(&insert_id),
            ),
            None => (ModelAppendTemplate::Neuron, ""),
        };
        // B2 方案：已冻结时，选中 neuron.content 作为独立 Context 消息插入（历史之后、用户输入之前）。
        let context = if state.stable_system_frozen {
            selected_neuron.as_ref().map(|n| n.content.as_str())
        } else {
            None
        };
        let messages = ModelCallInput::assemble_with_context(
            &input.messages,
            &role_system,
            insert_or_empty,
            context,
            model_input,
            template,
        );

        let model_response = self
            .model_caller
            .call_model(ModelCallRequest {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
                messages,
                tools,
            })
            .await?;
        tracing::info!(
            phase = "service_converse",
            provider = %model.provider_id,
            model = %model.model_id,
            output_len = model_response.output.len(),
            tool_calls = model_response.tool_calls.as_ref().map_or(0, |c| c.len()),
            "model call done"
        );

        let mut output = model_response.output.clone();
        let mut tool_result = None;
        // 单次工具执行语义：模型可能一次声明多个 tool_calls（并行调用），引擎只执行首个。
        // 产物仅携带被执行的这条，保证落库后 assistant(tool_calls=[该条]) 与 tool(结果) 配对一致；
        // 否则未应答的 tool_calls 会在历史 sanitize 时被降级，导致 tool 结果失去前置
        // tool_calls 声明，OpenAI 兼容接口报「tool 必须是前置 tool_calls 的响应」。
        let tool_calls = model_response
            .tool_calls
            .as_ref()
            .map(|calls| calls.iter().take(1).cloned().collect::<Vec<_>>());
        if let Some(tool_calls) = tool_calls.as_ref() {
            if let Some(first) = tool_calls.first() {
                if !authorized_tool_ids.iter().any(|id| id == &first.name) {
                    return Err(AppError::InvalidInput(format!(
                        "Tool '{}' is not authorized for this round",
                        first.name
                    )));
                }
                let tool = self
                    .tool_registry
                    .read()
                    .expect("tool registry lock should not be poisoned")
                    .get_tool(&first.name)
                    .ok_or_else(|| AppError::SkillNotFound(first.name.clone()))?;
                tracing::info!(
                    phase = "service_converse",
                    tool = %first.name,
                    args_len = first.arguments.to_string().len(),
                    "executing tool"
                );
                let result = tool.execute(first.arguments.clone()).await?;
                tracing::info!(
                    phase = "service_converse",
                    tool = %first.name,
                    result_len = result.len(),
                    "tool executed"
                );
                tool_result = Some(result.clone());
                output = if output.trim().is_empty() {
                    result
                } else {
                    format!("{output}\n\n[tool:{}] {result}", first.name)
                };
            }
        }

        Ok(RoundOutcome {
            response: output,
            model_output: Some(model_response.output.clone()),
            tool_calls,
            tool_result,
            selected_neuron_id: selected_neuron.map(|n| n.id),
            state,
        })
    }

    /// 种子分派：解析 role（role_system / selected_neuron / behavior）并推进 `state`。
    ///
    /// - `None`（直连）：不选型、role_system 空、Neuron 模板；清空历史锚点。
    /// - `Global`：无历史全域池选 1 → 写 last_selected；有历史退化为邻域选（锚点 = last_selected）。
    /// - `Neuron(普通)`：默认邻域行为（锚点 = 自身）+ FromNeuron 工具 + 无契约段。
    /// - `Neuron(系统)`：用 behavior（`None` 不选型清锚点 / `Fixed` 读自己 content 不写锚点 /
    ///   `Neighborhood` 锚点规则；`Global` 禁用于系统神经元 → 宽容回退 Neighborhood）。
    async fn resolve_role(
        &self,
        seed: Option<&SessionSeed>,
        state: &mut SessionState,
        messages: &[ModelMessage],
        reselect: bool,
    ) -> AppResult<(Option<Neuron>, String, Option<SessionBehavior>)> {
        let Some(seed) = seed else {
            state.last_selected_neuron_id = None;
            state.stable_system_prompt = None;
            state.stable_system_frozen = false;
            return Ok((None, String::new(), None));
        };
        let (neuron, mut role_system, behavior) = match seed {
            SessionSeed::Global => {
                let behavior = SessionBehavior {
                    selection: SelectionPolicy::Global {
                        limit: DEFAULT_ASSISTANT_GLOBAL_LIMIT,
                    },
                    tools: ToolPolicy::None,
                    insert_id: None,
                };
                let scope = Self::scope_for_selection(
                    &behavior.selection,
                    "",
                    state.last_selected_neuron_id.as_deref(),
                )
                .expect("Global always produce a scope");
                if let Some(role) = self.reuse_selected_neuron(state, reselect) {
                    tracing::info!(
                        phase = "resolve_role",
                        seed = ?SessionSeed::Global,
                        reused_anchor = true,
                        neuron_id = %role.id,
                        "reusing last-selected neuron (no LLM selection)"
                    );
                    let rs = role.content.clone();
                    (Some(role), rs, Some(behavior))
                } else {
                    let role = self.neuron_manager.select_role(messages, scope).await?;
                    state.last_selected_neuron_id = Some(role.id.clone());
                    tracing::info!(
                        phase = "resolve_role",
                        seed = ?SessionSeed::Global,
                        reused_anchor = false,
                        neuron_id = %role.id,
                        "LLM-selected neuron (global pool)"
                    );
                    let rs = role.content.clone();
                    (Some(role), rs, Some(behavior))
                }
            }
            SessionSeed::Neuron(id) => {
                let neuron = self
                    .neuron_manager
                    .get(id)?
                    .ok_or_else(|| AppError::NeuronNotFound(id.clone()))?;
                if neuron.system_type.is_none() {
                    // 普通神经元：推导默认领域行为（邻域锚点 = 自身）。
                    let behavior = SessionBehavior {
                        selection: SelectionPolicy::Neighborhood {
                            policy: NeighborhoodPoolPolicy::default(),
                        },
                        tools: ToolPolicy::FromNeuron,
                        insert_id: None,
                    };
                    let scope = Self::scope_for_selection(
                        &behavior.selection,
                        id,
                        state.last_selected_neuron_id.as_deref(),
                    )
                    .expect("Neighborhood always produce a scope");
                    if let Some(role) = self.reuse_selected_neuron(state, reselect) {
                        tracing::info!(
                            phase = "resolve_role",
                            seed = ?SessionSeed::Neuron(id.clone()),
                            reused_anchor = true,
                            neuron_id = %role.id,
                            "reusing last-selected neuron (no LLM selection)"
                        );
                        let rs = role.content.clone();
                        (Some(role), rs, Some(behavior))
                    } else {
                        let role = self.neuron_manager.select_role(messages, scope).await?;
                        state.last_selected_neuron_id = Some(role.id.clone());
                        tracing::info!(
                            phase = "resolve_role",
                            seed = ?SessionSeed::Neuron(id.clone()),
                            reused_anchor = false,
                            neuron_id = %role.id,
                            "LLM-selected neuron (neighborhood)"
                        );
                        let rs = role.content.clone();
                        (Some(role), rs, Some(behavior))
                    }
                } else {
                    // 系统神经元：用 behavior（禁 Global，旧数据宽容回退 Neighborhood）。
                    let behavior = neuron.behavior.clone().ok_or_else(|| {
                        AppError::InvalidInput(format!(
                            "neuron {id} is a system neuron but has no behavior"
                        ))
                    })?;
                    let selection = match &behavior.selection {
                        SelectionPolicy::Global { .. } => SelectionPolicy::Neighborhood {
                            policy: NeighborhoodPoolPolicy::default(),
                        },
                        other => other.clone(),
                    };
                    match &selection {
                        SelectionPolicy::None => {
                            tracing::info!(
                                phase = "resolve_role",
                                seed = ?SessionSeed::Neuron(id.clone()),
                                selection = "none",
                                "system neuron: no selection, clearing anchor"
                            );
                            state.last_selected_neuron_id = None;
                            (None, String::new(), Some(behavior))
                        }
                        SelectionPolicy::Fixed => {
                            // 读系统神经元自己的 content；不写 last_selected。
                            tracing::info!(
                                phase = "resolve_role",
                                seed = ?SessionSeed::Neuron(id.clone()),
                                selection = "fixed",
                                neuron_id = %neuron.id,
                                "system neuron: fixed role, no LLM selection"
                            );
                            let rs = neuron.content.clone();
                            (Some(neuron), rs, Some(behavior))
                        }
                        SelectionPolicy::Neighborhood { .. } => {
                            let scope = Self::scope_for_selection(
                                &selection,
                                id,
                                state.last_selected_neuron_id.as_deref(),
                            )
                            .expect("Neighborhood always produce a scope");
                            if let Some(role) = self.reuse_selected_neuron(state, reselect) {
                                tracing::info!(
                                    phase = "resolve_role",
                                    seed = ?SessionSeed::Neuron(id.clone()),
                                    selection = "neighborhood",
                                    reused_anchor = true,
                                    neuron_id = %role.id,
                                    "reusing last-selected neuron (no LLM selection)"
                                );
                                let rs = role.content.clone();
                                (Some(role), rs, Some(behavior))
                            } else {
                                let role = self.neuron_manager.select_role(messages, scope).await?;
                                state.last_selected_neuron_id = Some(role.id.clone());
                                tracing::info!(
                                    phase = "resolve_role",
                                    seed = ?SessionSeed::Neuron(id.clone()),
                                    selection = "neighborhood",
                                    reused_anchor = false,
                                    neuron_id = %role.id,
                                    "LLM-selected neuron (neighborhood)"
                                );
                                let rs = role.content.clone();
                                (Some(role), rs, Some(behavior))
                            }
                        }
                        SelectionPolicy::Global { .. } => {
                            unreachable!("converted to Neighborhood above")
                        }
                    }
                }
            }
        };
        Self::freeze_or_replace(state, &mut role_system);
        Ok((neuron, role_system, behavior))
    }

    /// 首轮冻结 / 后续轮替换为稳定系统提示词。
    /// 首轮（`stable_system_frozen == false`）将 `role_system` 冻结为 `stable_system_prompt`；
    /// 后续轮用 `stable_system_prompt` 替换 `role_system`，保持 System 消息不变。
    fn freeze_or_replace(state: &mut SessionState, role_system: &mut String) {
        if role_system.is_empty() {
            return;
        }
        if !state.stable_system_frozen {
            state.stable_system_prompt = Some(role_system.clone());
            state.stable_system_frozen = true;
        } else if let Some(stable) = &state.stable_system_prompt {
            *role_system = stable.clone();
        }
    }

    /// 选型降频（复用轮）：`reselect == false` 且有历史锚点时，直接沿用
    /// `last_selected_neuron_id` 作为本轮角色（跳过 LLM 选型）。
    /// `true`（选型轮）/ 锚点缺失返回 `None`，走正常选型。
    fn reuse_selected_neuron(&self, state: &SessionState, reselect: bool) -> Option<Neuron> {
        if reselect {
            return None;
        }
        let id = state.last_selected_neuron_id.as_ref()?;
        self.neuron_manager.get(id).ok().flatten()
    }

    /// selection → 候选池装配 scope（委托 NeuronManager，`resolve_role` 共用语义）。
    fn scope_for_selection(
        selection: &SelectionPolicy,
        spec_neuron_id: &str,
        last_selected: Option<&str>,
    ) -> Option<super::models::AssistantCandidateScope> {
        NeuronManager::scope_for_selection(selection, spec_neuron_id, last_selected)
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
            tracing::warn!(
                phase = "tool_authorization",
                tool_id = %id,
                "ignoring unknown tool id"
            );
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
        error::AppResult,
        models::{ChatModelSelection, ModelMessageRole, NeuronCreate, ToolDefinition, ToolSource},
        neuron::{
            config::NeuronConfigReader,
            manager::ASSISTANT_SELECT_NEURON,
            model::NeuronModelCaller,
            store::NeuronStore,
        },
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
            mode: crate::core::models::ConversationMode::Chat,
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
        tool_call: Arc<Mutex<Option<ToolCall>>>,
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
                tool_calls: self.tool_call.lock().unwrap().clone().map(|tc| vec![tc]),
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
    impl crate::core::tool_registry::Tool for EchoTool {
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
        service: Arc<NeuronCallService>,
        selector_calls: Arc<AtomicUsize>,
        echo_calls: Arc<AtomicUsize>,
        echo_tool_call: Arc<Mutex<Option<ToolCall>>>,
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
        let echo_calls = Arc::new(AtomicUsize::new(0));
        let echo_tool_call: Arc<Mutex<Option<ToolCall>>> = Arc::new(Mutex::new(None));
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
        let service = Arc::new(NeuronCallService::new(
            Arc::clone(&echo),
            Arc::clone(&manager),
            Arc::clone(&tool_registry),
        ));
        Harness {
            store,
            manager,
            service,
            selector_calls,
            echo_calls,
            echo_tool_call,
            last_messages,
            last_tools,
            tool_registry,
            root,
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
    async fn converse_keeps_history_and_role_in_model_input() {
        let h = harness();
        let history = vec![ModelMessage {
            role: ModelMessageRole::User,
            content: "past".into(),
            tool_calls: None,
            tool_call_id: None,
        }];
        let _outcome = h
            .service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: history,
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "current",
                &model(),
            )
            .await
            .unwrap();
        // 直连：role_system 为空 → Neuron 模板系统段 + 历史 + 本轮输入。
        let blob = h
            .last_messages
            .lock()
            .unwrap()
            .iter()
            .map(|m| format!("{:?}:{}", m.role, m.content))
            .collect::<String>();
        assert!(blob.contains("past"), "history should reach model: {blob}");
        assert!(blob.contains("current"), "current input should reach model: {blob}");
        assert!(blob.contains("【神经元】"), "neuron template system section expected: {blob}");
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
    fn session_seed_serde_roundtrip() {
        for seed in [SessionSeed::Global, SessionSeed::Neuron("n-1".into())] {
            let value = serde_json::to_value(&seed).unwrap();
            let back: SessionSeed = serde_json::from_value(value).unwrap();
            assert_eq!(back, seed);
        }
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

    #[test]
    fn filter_drops_unknown_tool_ids() {
        let mut registry = ToolRegistry::new();
        registry.register_source(EchoTool::new("echo"), ToolSource::Config);
        let out = filter_authorized_tool_ids(&registry, &["echo".into(), "nope".into()]);
        assert_eq!(out, vec!["echo".to_string()]);
    }

    #[tokio::test]
    async fn converse_direct_seed_no_selection() {
        let h = harness();
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "hello",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.selected_neuron_id, None);
        assert_eq!(outcome.state.last_selected_neuron_id, None);
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Global),
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 1);
        let selected = outcome.selected_neuron_id.expect("global round selects");
        assert!(matches!(outcome.state.last_selected_neuron_id.as_deref(), Some(id) if id == selected));
        assert_ne!(selected, "");
    }

    #[tokio::test]
    async fn converse_global_with_history_uses_neighborhood() {
        let h = harness();
        ensure_selector(&h);
        let root = insert_plain(&h, "root");
        let child_a = insert_downstream(&h, &root.id, "child-a");
        let _child_b = insert_downstream(&h, &root.id, "child-b");
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Global),
                    state: SessionState {
                        last_selected_neuron_id: Some(child_a.id.clone()),
                        ..Default::default()
                    },
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "advance",
                &model(),
            )
            .await
            .unwrap();
        // 有历史 → 邻域选（锚点 = last_selected）。邻域候选会按策略扩展创建 → 至少一次模型调用。
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.unwrap();
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
            Some(selected.as_str())
        );
    }

    #[tokio::test]
    async fn converse_plain_neuron_defaults_to_neighborhood() {
        let h = harness();
        // 普通神经元 seed → 推导默认邻域行为（锚点 = 自身）；邻域候选按策略扩展创建后选型。
        let iso = insert_plain(&h, "iso");
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(iso.id.clone())),
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "hi",
                &model(),
            )
            .await
            .unwrap();
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.expect("neighborhood selects");
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(sys.id.clone())),
                    state: SessionState {
                        last_selected_neuron_id: Some("pre".into()),
                        ..Default::default()
                    },
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        // Fixed：选中系统神经元自身，且不改写历史锚点。
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(sys.id.as_str()));
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
            Some("pre")
        );
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(sys.id.clone())),
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        assert!(h.selector_calls.load(Ordering::Relaxed) >= 1);
        let selected = outcome.selected_neuron_id.expect("neighborhood selects");
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(sys.id.clone())),
                    state: SessionState {
                        last_selected_neuron_id: Some("pre".into()),
                        ..Default::default()
                    },
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.selected_neuron_id, None);
        assert_eq!(outcome.state.last_selected_neuron_id, None);
    }

    #[tokio::test]
    async fn converse_tool_override_grants_tools() {
        let h = harness();
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo"), ToolSource::Config);
        *h.echo_tool_call.lock().unwrap() = Some(ToolCall {
            id: "call-1".into(),
            name: "echo".into(),
            arguments: json!({"text": "hi"}),
        });
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: Some(vec!["echo".into()]),
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.tool_result.as_deref(), Some("echo:hi"));
        assert!(outcome.response.contains("[tool:echo] echo:hi"));
        assert!(outcome.tool_calls.is_some());
        assert_eq!(outcome.model_output.as_deref(), Some("echo-0"));
    }

    #[tokio::test]
    async fn converse_unauthorized_tool_rejected() {
        let h = harness();
        h.tool_registry
            .write()
            .unwrap()
            .register_source(EchoTool::new("echo"), ToolSource::Config);
        *h.echo_tool_call.lock().unwrap() = Some(ToolCall {
            id: "call-1".into(),
            name: "echo".into(),
            arguments: json!({"text": "hi"}),
        });
        let err = h
            .service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Global),
                    state: SessionState {
                        last_selected_neuron_id: Some(anchor.id.clone()),
                        ..Default::default()
                    },
                    messages: vec![],
                    tool_override: None,
                    reselect: false,
                    tool_tags: Vec::new(),
                },
                "advance",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 0, "复用轮不得调 selector");
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(anchor.id.as_str()));
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
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
            let outcome = h
                .service
                .converse(
                    RoundInput {
                        seed: Some(SessionSeed::Global),
                        state: SessionState {
                            last_selected_neuron_id: Some(anchor.id.clone()),
                            ..Default::default()
                        },
                        messages: vec![],
                        tool_override: None,
                        reselect: true,
                        tool_tags: Vec::new(),
                    },
                    "advance",
                    &model(),
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Global),
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: false,
                    tool_tags: Vec::new(),
                },
                "advance",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(h.selector_calls.load(Ordering::Relaxed), 1);
        let selected = outcome.selected_neuron_id.expect("无锚点时复用轮也回退选型");
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
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
        let outcome = h
            .service
            .converse(
                RoundInput {
                    seed: Some(SessionSeed::Neuron(sys.id.clone())),
                    state: SessionState {
                        last_selected_neuron_id: Some("pre".into()),
                        ..Default::default()
                    },
                    messages: vec![],
                    tool_override: None,
                    reselect: false,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        // Fixed：复用轮也读自己 content，不写锚点、不调 selector。
        assert_eq!(outcome.selected_neuron_id.as_deref(), Some(sys.id.as_str()));
        assert_eq!(
            outcome.state.last_selected_neuron_id.as_deref(),
            Some("pre")
        );
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
        h.service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "hello",
                &model(),
            )
            .await
            .unwrap();
        assert!(wire_names().is_empty(), "Chat 对话应禁用工具（不注入 Core）");

        // 2) 系统模式对话：Core + System 进 wire，Normal（神经元管理）不进。
        h.service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: None,
                    reselect: true,
                    tool_tags: vec![ToolTag::Core, ToolTag::System],
                },
                "hello",
                &model(),
            )
            .await
            .unwrap();
        assert_eq!(
            wire_names(),
            vec!["core_echo", "sys_echo"],
            "系统模式应注入 Core + System"
        );

        // 3) 非对话调用（mode=None，如禁工具的内部裁决）：不注入任何标签工具。
        h.service
            .converse(
                RoundInput {
                    seed: None,
                    state: SessionState::default(),
                    messages: vec![],
                    tool_override: Some(Vec::new()),
                    reselect: true,
                    tool_tags: Vec::new(),
                },
                "go",
                &model(),
            )
            .await
            .unwrap();
        assert!(wire_names().is_empty(), "非对话调用不应注入任何工具");
    }
}
