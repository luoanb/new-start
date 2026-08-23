//! 统一 RPC 端点：把 Tauri command 集合以 `POST /rpc` 暴露给远程前端。
//!
//! - 载荷：`{ cmd, params }`，`params` 字段名与前端 Tauri `invoke` 一致（camelCase），
//!   httpClient 可直接复用现有参数构造，零迁移。
//! - 每个分支实现与 `lib.rs` 对应 command 相同业务语义（调用 `Gateway` / 分域 State →
//!   成功后广播 `StateChange`）。
//! - 锁纪律与 Tauri 路径一致：不持 Gateway / 域锁跨网络 I/O，仅短临界区 `lock`。

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::core::{
    app_log, hook::hook_defs_meta, hook_judgement_store::HookJudgementFilter,
    insert_catalog::InsertCatalog, providers::ProviderConfigView,
    tool_config::ToolConfigView,
    AppError, AppResult, ChatOptions, ConversationMode,
    ModelCallRequest, NeuronCreate, NeuronKindFilter, NeuronUpdate, SessionBehavior, SessionSeed,
    StateChange, TopicStatus, TopicUpdate,
};
use crate::fileops::search::chunk::SemanticSearchResult;
use crate::fileops::search::retriever::Retriever;
use crate::fileops::workspace::{WorkspaceEntry, WorkspaceStore};
use crate::fileops::gitops::confirm::{ConfirmOutcome, GitOpKind};
use crate::fileops::gitops::{ConflictTake, GitResetMode, GitStashAction};

use super::NetState;

// ── 请求 / 响应 ──

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub cmd: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcErrorBody {
    pub code: String,
    pub message: String,
}

impl From<AppError> for RpcErrorBody {
    fn from(e: AppError) -> Self {
        Self {
            code: e.code().to_string(),
            message: e.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorBody>,
}

fn bad_request(message: impl Into<String>) -> RpcErrorBody {
    RpcErrorBody {
        code: "bad_request".into(),
        message: message.into(),
    }
}

fn from_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcErrorBody> {
    serde_json::from_value(params).map_err(|e| bad_request(e.to_string()))
}

fn value<T: Serialize>(data: T) -> Result<Value, RpcErrorBody> {
    serde_json::to_value(data).map_err(|e| bad_request(e.to_string()))
}

// ── 锁内短临界区 helpers（对齐 lib.rs with_topic_store / with_poller）──

fn with_topic<T>(
    state: &NetState,
    f: impl FnOnce(&crate::core::TopicStore) -> AppResult<T>,
) -> Result<T, RpcErrorBody> {
    let store = state.gateway.topic_store().map_err(RpcErrorBody::from)?;
    let guard = store.lock().map_err(|_| {
        RpcErrorBody {
            code: "lock_failed".into(),
            message: "TopicStore lock failed".into(),
        }
    })?;
    f(&guard).map_err(RpcErrorBody::from)
}

fn with_hook_judgement<T>(
    state: &NetState,
    f: impl FnOnce(&crate::core::hook_judgement_store::HookJudgementStore) -> AppResult<T>,
) -> Result<T, RpcErrorBody> {
    let store = state
        .gateway
        .hook_judgement_store()
        .map_err(RpcErrorBody::from)?;
    let guard = store.lock().map_err(|_| {
        RpcErrorBody {
            code: "lock_failed".into(),
            message: "HookJudgementStore lock failed".into(),
        }
    })?;
    f(&guard).map_err(RpcErrorBody::from)
}

fn with_poller<T>(
    state: &NetState,
    f: impl FnOnce(&mut crate::core::Poller) -> AppResult<T>,
) -> Result<T, RpcErrorBody> {
    let poller = state.gateway.poller();
    let mut guard = poller.lock().map_err(|_| {
        RpcErrorBody {
            code: "lock_failed".into(),
            message: "Poller lock failed".into(),
        }
    })?;
    f(&mut guard).map_err(RpcErrorBody::from)
}

fn topic_status_filter(status: Option<&str>) -> Option<TopicStatus> {
    status.and_then(|s| match s {
        "todo" => Some(TopicStatus::Todo),
        "in_progress" => Some(TopicStatus::InProgress),
        "paused" => Some(TopicStatus::Paused),
        "done" => Some(TopicStatus::Done),
        "cancelled" => Some(TopicStatus::Cancelled),
        "waiting_user" => Some(TopicStatus::WaitingUser),
        "wrapping_up" => Some(TopicStatus::WrappingUp),
        _ => None,
    })
}

// ── 参数结构（字段名与前端 Tauri invoke 参数一致，camelCase）──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendChatParams {
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
    #[serde(default)]
    params: Option<crate::core::SamplingParams>,
    #[serde(default)]
    thinking: Option<crate::core::ThinkingConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationIdParams {
    session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderModelParams {
    provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearConversationParams {
    conversation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationSummariesParams {
    #[serde(default)]
    page: usize,
    #[serde(default = "default_summaries_page_size")]
    page_size: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPageParams {
    conversation_id: Option<String>,
    #[serde(default = "default_message_page_size")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_summaries_page_size() -> usize {
    50
}

fn default_message_page_size() -> usize {
    100
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TopicListParams {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookJudgementsListParams {
    filters: Option<HookJudgementFilter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdParams {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTopicParams {
    name: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTopicParams {
    id: String,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeItemParams {
    topic_id: String,
    item_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddScopeItemParams {
    topic_id: String,
    goal: String,
    done_contract: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParallelismParams {
    n: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateNeuronParams {
    id: String,
    desc: Option<String>,
    content: Option<String>,
    tool_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkParams {
    id: String,
    max_depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateNeuronParams {
    desc: String,
    content: Option<String>,
    link_to: Option<String>,
    tool_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeightParams {
    id: String,
    delta: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeWeightParams {
    source: String,
    target: String,
    delta: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreFeedbackParams {
    conversation_id: String,
    message_index: usize,
    score: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenSessionParams {
    spec_neuron_id: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSessionModelParams {
    conversation_id: String,
    selection: crate::core::ChatModelSelection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListNeuronsPageParams {
    page: usize,
    page_size: usize,
    search: Option<String>,
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemTypeParams {
    id: String,
    system_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BehaviorParams {
    id: String,
    behavior: SessionBehavior,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LevelParams {
    level: String,
}

// ── RPC 端点 ──

pub async fn handle_rpc(
    State(state): State<NetState>,
    Json(req): Json<RpcRequest>,
) -> Json<RpcResponse> {
    let result = dispatch(&state, &req.cmd, req.params.unwrap_or(Value::Null)).await;
    match result {
        Ok(data) => Json(RpcResponse {
            ok: true,
            data: Some(data),
            error: None,
        }),
        Err(error) => Json(RpcResponse {
            ok: false,
            data: None,
            error: Some(error),
        }),
    }
}

async fn dispatch(state: &NetState, cmd: &str, params: Value) -> Result<Value, RpcErrorBody> {
    match cmd {
        // ── Debug ──
        "debug_storage_path" => Ok(Value::String(format!(
            "cwd={:?} storage={:?}",
            std::env::current_dir(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join(crate::core::storage::STORAGE_DIR_NAME)
        ))),

        // ── Chat ──
        "send_chat_message" => {
            let p: SendChatParams = from_params(params)?;
            // 流式入口：MessageDelta 增量 + 完成后的 Conversations 收敛均由 Gateway 内部广播
            //（SSE 复用同一 StateChange 通道自动推送）。
            let response = state
                .gateway
                .send_model_message_stream(
                    p.message,
                    ChatOptions {
                        provider_id: p.provider_id,
                        model_id: p.model_id,
                        conversation_id: p.conversation_id,
                        params: p.params,
                        thinking: p.thinking,
                    },
                )
                .await
                .map_err(RpcErrorBody::from)?;
            value(response)
        }
        "create_conversation" => {
            let p: ModeParams = from_params(params)?;
            let conversation_id = state
                .gateway
                .create_new_conversation(conv_mode(&p.mode))
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Conversations {
                affected: vec![conversation_id.clone()],
            });
            value(conversation_id)
        }
        "close_session" => {
            let p: ConversationIdParams = from_params(params)?;
            let session_id = state
                .gateway
                .session_tracker()
                .close(&p.session_id)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Sessions);
            value(session_id)
        }
        "list_running_sessions" => {
            let sessions = state
                .gateway
                .session_tracker()
                .list()
                .map_err(RpcErrorBody::from)?;
            value(sessions)
        }

        // ── Info ──
        "list_skills" => value(state.gateway.list_skills()),
        "list_tools" => value(state.gateway.list_tool_info()),
        "list_mcp_servers" => value(state.gateway.mcp_server_statuses()),
        "get_tool_config" => {
            let view = state
                .gateway
                .get_tool_config()
                .map_err(RpcErrorBody::from)?;
            value(view)
        }
        "save_tool_config" => {
            let p: ViewParams<ToolConfigView> = from_params(params)?;
            let view = state
                .gateway
                .save_tool_config(p.view)
                .await
                .map_err(RpcErrorBody::from)?;
            value(view)
        }
        "reassemble_tools" => {
            state
                .gateway
                .reassemble_tools()
                .await
                .map_err(RpcErrorBody::from)?;
            value(())
        }
        "list_providers" => value(state.gateway.providers().list_providers()),
        "list_models" => {
            let p: ProviderModelParams = from_params(params)?;
            let models = state
                .gateway
                .providers()
                .list_models(p.provider_id.as_deref())
                .map_err(RpcErrorBody::from)?;
            value(models)
        }
        "call_model" => {
            let p: ViewParams<ModelCallRequest> = from_params(params)?;
            let res = state
                .gateway
                .providers()
                .call_model(p.view)
                .await
                .map_err(RpcErrorBody::from)?;
            value(res)
        }
        "get_provider_config" => {
            let view = state
                .gateway
                .providers()
                .get_config_view()
                .map_err(RpcErrorBody::from)?;
            value(view)
        }
        "save_provider_config" => {
            let p: ViewParams<ProviderConfigView> = from_params(params)?;
            let saved = state
                .gateway
                .providers()
                .save_config(p.view)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Providers);
            value(saved)
        }
        "list_conversations" => {
            let list = state
                .gateway
                .conversation_store()
                .list_conversations()
                .map_err(RpcErrorBody::from)?;
            value(list)
        }
        "history" => {
            let p: ClearConversationParams = from_params(params)?;
            let messages = state
                .gateway
                .history(p.conversation_id)
                .map_err(RpcErrorBody::from)?;
            value(messages)
        }
        "list_conversation_summaries" => {
            let p: ConversationSummariesParams = from_params(params)?;
            let page = state
                .gateway
                .list_conversation_summaries(p.page, p.page_size)
                .map_err(RpcErrorBody::from)?;
            value(page)
        }
        "history_page" => {
            let p: HistoryPageParams = from_params(params)?;
            let page = state
                .gateway
                .history_page(p.conversation_id, p.limit, p.offset)
                .map_err(RpcErrorBody::from)?;
            value(page)
        }
        "clear_conversation" => {
            let p: ClearConversationParams = from_params(params)?;
            let conversation_id = state
                .gateway
                .clear_conversation(p.conversation_id)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Conversations {
                affected: vec![conversation_id.clone()],
            });
            value(conversation_id)
        }
        "status" => {
            let status = state.gateway.status().map_err(RpcErrorBody::from)?;
            value(status)
        }

        // ── Topic ──
        "list_topics" => {
            let p: TopicListParams = from_params(params)?;
            let filter = topic_status_filter(p.status.as_deref());
            let topics = with_topic(state, |store| store.list(filter))?;
            value(topics)
        }
        "get_topic" => {
            let p: IdParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.get(&p.id)?.ok_or_else(|| {
                    AppError::ConversationNotFound(format!("Topic not found: {}", p.id))
                })
            })?;
            value(topic)
        }
        "create_topic" => {
            let p: CreateTopicParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.create(&p.name, &p.description, TopicStatus::Todo, vec![], None)
            })?;
            value(topic)
        }
        "update_topic" => {
            let p: UpdateTopicParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.update(
                    &p.id,
                    TopicUpdate {
                        name: p.name,
                        description: p.description,
                        extra: None,
                    },
                )
            })?;
            value(topic)
        }
        "delete_topic" => {
            let p: IdParams = from_params(params)?;
            let deleted = with_topic(state, |store| store.delete(&p.id))?;
            value(deleted)
        }
        "add_topic_scope_item" => {
            let p: AddScopeItemParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.add_scope_item(&p.topic_id, &p.goal, &p.done_contract)
            })?;
            value(topic)
        }
        "delete_topic_scope_item" => {
            let p: ScopeItemParams = from_params(params)?;
            let topic =
                with_topic(state, |store| store.delete_scope_item(&p.topic_id, &p.item_id))?;
            value(topic)
        }
        "complete_topic_scope_item" => {
            let p: ScopeItemParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.complete_scope_item(&p.topic_id, &p.item_id)
            })?;
            value(topic)
        }
        "pause_topic" => {
            let p: IdParams = from_params(params)?;
            let topic = with_topic(state, |store| store.pause(&p.id))?;
            value(topic)
        }
        "resume_topic" => {
            let p: IdParams = from_params(params)?;
            let topic = with_topic(state, |store| store.resume(&p.id))?;
            value(topic)
        }

        // ── Hook Judgements ──
        "hook_judgements_list" => {
            let p: HookJudgementsListParams = from_params(params)?;
            let filter = p.filters.unwrap_or_default();
            let records = with_hook_judgement(state, |store| store.list(&filter))?;
            value(records)
        }
        "hook_defs_list" => {
            value(hook_defs_meta())
        }

        // ── Poller ──
        "poll_status" => {
            let status = with_poller(state, |p| Ok(p.status()))?;
            value(status)
        }
        "poll_pause" => {
            let status = with_poller(state, |p| {
                p.pause();
                Ok(p.status())
            })?;
            (state.state_emit)(StateChange::Poller { status });
            value(())
        }
        "poll_resume" => {
            let status = with_poller(state, |p| {
                p.resume();
                Ok(p.status())
            })?;
            (state.state_emit)(StateChange::Poller { status });
            value(())
        }
        "poll_trigger" => {
            let status = with_poller(state, |p| {
                p.trigger();
                Ok(p.status())
            })?;
            (state.state_emit)(StateChange::Poller { status });
            value(())
        }
        "poll_set_parallelism" => {
            let p: ParallelismParams = from_params(params)?;
            let clamped = state
                .gateway
                .set_poll_parallelism(p.n)
                .map_err(RpcErrorBody::from)?;
            let status = state
                .gateway
                .poll_status()
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Poller { status });
            value(clamped)
        }

        // ── Neuron ──
        "list_neurons" => {
            let list = state
                .gateway
                .neuron_manager()
                .list_neurons()
                .map_err(RpcErrorBody::from)?;
            value(list)
        }
        "get_neuron" => {
            let p: IdParams = from_params(params)?;
            let neuron = state
                .gateway
                .neuron_manager()
                .get_neuron(&p.id)
                .map_err(RpcErrorBody::from)?
                .ok_or_else(|| AppError::NeuronNotFound(p.id))
                .map_err(RpcErrorBody::from)?;
            value(neuron)
        }
        "update_neuron" => {
            let p: UpdateNeuronParams = from_params(params)?;
            let neuron = state
                .gateway
                .neuron_manager()
                .update_content_for_admin(
                    &p.id,
                    NeuronUpdate {
                        desc: p.desc,
                        content: p.content,
                        tool_ids: p.tool_ids,
                    },
                )
                .map_err(RpcErrorBody::from)?;
            value(neuron)
        }
        "get_connections" => {
            let p: IdParams = from_params(params)?;
            let connections = state
                .gateway
                .neuron_manager()
                .get_connections(&p.id)
                .map_err(RpcErrorBody::from)?;
            value(connections)
        }
        "get_network" => {
            let p: NetworkParams = from_params(params)?;
            let subgraph = state
                .gateway
                .neuron_manager()
                .get_network(&p.id, p.max_depth.unwrap_or(2))
                .map_err(RpcErrorBody::from)?;
            value(subgraph)
        }
        "create_neuron_plain" => {
            let p: CreateNeuronParams = from_params(params)?;
            let create = NeuronCreate {
                desc: p.desc,
                content: p.content.unwrap_or_default(),
                weight: 0.0,
                system_type: None,
                tool_ids: p.tool_ids,
                lineage_parent_id: None,
                variant_state: None,
            };
            let neuron = state
                .gateway
                .neuron_manager()
                .create_plain(create, p.link_to.as_deref())
                .map_err(RpcErrorBody::from)?;
            value(neuron)
        }
        "adjust_neuron_weight" => {
            let p: WeightParams = from_params(params)?;
            let neuron = state
                .gateway
                .neuron_manager()
                .adjust_weight(&p.id, p.delta)
                .map_err(RpcErrorBody::from)?;
            value(neuron)
        }
        "adjust_edge_weight" => {
            let p: EdgeWeightParams = from_params(params)?;
            let connection = state
                .gateway
                .neuron_manager()
                .adjust_edge_weight(&p.source, &p.target, p.delta)
                .map_err(RpcErrorBody::from)?;
            value(connection)
        }
        "score_feedback" => {
            let p: ScoreFeedbackParams = from_params(params)?;
            state
                .gateway
                .assistant()
                .score_feedback(&p.conversation_id, p.message_index, p.score)
                .await
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Neurons);
            (state.state_emit)(StateChange::Conversations {
                affected: vec![p.conversation_id],
            });
            value(())
        }

        // ── Session Specs ──
        "set_session_model" => {
            let p: SetSessionModelParams = from_params(params)?;
            state
                .gateway
                .set_session_model(&p.conversation_id, &p.selection)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Conversations {
                affected: vec![p.conversation_id],
            });
            value(())
        }
        "open_session" => {
            let p: OpenSessionParams = from_params(params)?;
            let conv_mode = conv_mode(&p.mode);
            let seed = match p.spec_neuron_id.trim() {
                "" if conv_mode == ConversationMode::Assistant
                    || conv_mode == ConversationMode::System =>
                {
                    Some(SessionSeed::Global)
                }
                "" => None,
                id => Some(SessionSeed::Neuron(id.to_string())),
            };
            let conversation = state
                .gateway
                .start_session(seed, conv_mode)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Conversations {
                affected: vec![conversation.id.clone()],
            });
            value(conversation)
        }
        "list_neurons_page" => {
            let p: ListNeuronsPageParams = from_params(params)?;
            let page = state
                .gateway
                .neuron_manager()
                .list_neurons_page(
                    p.page,
                    p.page_size,
                    p.search.as_deref(),
                    NeuronKindFilter::parse(&p.kind),
                )
                .map_err(RpcErrorBody::from)?;
            value(page)
        }
        "set_neuron_system_type" => {
            let p: SystemTypeParams = from_params(params)?;
            let neuron = state
                .gateway
                .neuron_manager()
                .set_system_type_for_admin(&p.id, p.system_type.as_deref())
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Neurons);
            value(neuron)
        }
        "update_neuron_behavior" => {
            let p: BehaviorParams = from_params(params)?;
            let neuron = state
                .gateway
                .neuron_manager()
                .update_behavior_for_admin(&p.id, p.behavior)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Neurons);
            value(neuron)
        }
        "list_insert_catalog" => value(InsertCatalog::catalog()),

        // ── Logs ──
        "logs_snapshot" => value(app_log::snapshot()),
        "logs_get_level" => value(app_log::get_level()),
        "logs_set_level" => {
            let p: LevelParams = from_params(params)?;
            let level = app_log::set_level(&p.level)
                .map_err(|message| bad_request(message))?;
            value(level)
        }
        "logs_clear_buffer" => {
            app_log::clear_buffer();
            value(())
        }
        "logs_dir" => value(app_log::log_dir().map(|path| path.display().to_string())),

        // ── Workspace / Files ──
        "list_workspaces" => {
            let view = state
                .gateway
                .workspace_store()
                .view()
                .map_err(RpcErrorBody::from)?;
            value(view)
        }
        "add_workspace" => {
            let p: RootParams = from_params(params)?;
            let view = state
                .gateway
                .workspace_store()
                .add(&p.root)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(view)
        }
        "remove_workspace" => {
            let p: IdParams = from_params(params)?;
            let view = state
                .gateway
                .workspace_store()
                .remove(&p.id)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(view)
        }
        "set_active_workspace" => {
            let p: IdParams = from_params(params)?;
            let view = state
                .gateway
                .workspace_store()
                .set_active(&p.id)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(view)
        }
        "update_workspace_ignore" => {
            let p: IgnoreParams = from_params(params)?;
            let view = state
                .gateway
                .workspace_store()
                .update_ignore(&p.id, p.ignore)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(view)
        }
        "get_home_dir" => {
            crate::home_dir_path()
                .map(|p| Value::String(p.to_string_lossy().into_owned()))
                .ok_or_else(|| bad_request("无法获取用户主目录"))
        }
        "fs_suggest_abs" => {
            let p: FsPathParams = from_params(params)?;
            value(
                crate::fileops::fs::list_suggest(&p.path).map_err(|m| RpcErrorBody {
                    code: "fs_suggest_failed".into(),
                    message: m,
                })?,
            )
        }
        "fs_list" => {
            let p: FsListParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let entries = state
                .gateway
                .file_system()
                .list(&ws, p.path.as_deref(), p.ignore.as_deref())
                .map_err(RpcErrorBody::from)?;
            value(entries)
        }
        "fs_read" => {
            let p: FsReadParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let result = state
                .gateway
                .file_system()
                .read(&ws, &p.path, p.offset, p.limit)
                .map_err(RpcErrorBody::from)?;
            value(result)
        }
        "fs_write" => {
            let p: FsWriteParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let result = state
                .gateway
                .file_system()
                .write(&ws, &p.path, &p.content, p.base_mtime)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(result)
        }
        "fs_create_dir" => {
            let p: FsPathParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            state
                .gateway
                .file_system()
                .create_dir(&ws, &p.path)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(())
        }
        "fs_delete" => {
            let p: FsDeleteParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            state
                .gateway
                .file_system()
                .delete(&ws, &p.paths)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(())
        }
        "fs_rename" => {
            let p: FsMoveParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            state
                .gateway
                .file_system()
                .rename(&ws, &p.from, &p.to)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(())
        }
        "fs_move" => {
            let p: FsMoveParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            state
                .gateway
                .file_system()
                .rename(&ws, &p.from, &p.to)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Workspaces);
            value(())
        }
        "fs_glob" => {
            let p: FsGlobParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let results = state
                .gateway
                .file_system()
                .glob(&ws, &p.pattern, p.cwd.as_deref())
                .map_err(RpcErrorBody::from)?;
            value(results)
        }
        "fs_grep" => {
            let p: FsGrepParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let matches = state
                .gateway
                .file_system()
                .grep(
                    &ws,
                    &p.pattern,
                    p.path.as_deref(),
                    p.case_sensitive.unwrap_or(false),
                    p.multiline.unwrap_or(false),
                    p.glob.as_deref(),
                    p.context.unwrap_or(0),
                )
                .map_err(RpcErrorBody::from)?;
            value(matches)
        }
        "fs_semantic_search" => {
            let p: FsSemanticSearchParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let index_root = state.gateway.search_index_root();
            let result: SemanticSearchResult = Retriever::search(
                &index_root,
                &ws,
                &p.query,
                p.top_k,
                p.path.as_deref(),
            )
            .map_err(RpcErrorBody::from)?;
            value(result)
        }
        "fs_info" => {
            let p: FsPathParams = from_params(params)?;
            let store = state.gateway.workspace_store();
            let ws = require_active(&store)?;
            let info = state
                .gateway
                .file_system()
                .info(&ws, &p.path)
                .map_err(RpcErrorBody::from)?;
            value(info)
        }

        // ── Git ──
        "git_repos" => {
            let repos = state
                .gateway
                .git_service()
                .discover_repos()
                .await
                .map_err(RpcErrorBody::from)?;
            value(repos)
        }
        "git_status" => {
            let p: GitStatusParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            let view = svc.backend().status(&repo).await.map_err(RpcErrorBody::from)?;
            value(view)
        }
        "git_diff" => {
            let p: GitDiffParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            let diff = svc
                .backend()
                .diff(&repo, p.cached.unwrap_or(false), p.path.as_deref())
                .await
                .map_err(RpcErrorBody::from)?;
            value(diff)
        }
        "git_log" => {
            let p: GitLogParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let commits = svc
                .backend()
                .log(&repo, p.limit.unwrap_or(30), p.offset.unwrap_or(0))
                .await
                .map_err(RpcErrorBody::from)?;
            value(commits)
        }
        "git_show_files" => {
            let p: GitShowFilesParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            let files = svc
                .backend()
                .show_files(&repo, &p.hash)
                .await
                .map_err(RpcErrorBody::from)?;
            value(files)
        }
        "git_show_diff" => {
            let p: GitShowDiffParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            let diff = svc
                .backend()
                .show_diff(&repo, &p.hash, &p.path)
                .await
                .map_err(RpcErrorBody::from)?;
            value(diff)
        }
        "git_branches" => {
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let branches = svc
                .backend()
                .branches(&repo)
                .await
                .map_err(RpcErrorBody::from)?;
            value(branches)
        }
        "git_blame" => {
            let p: GitBlameParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            let lines = svc
                .backend()
                .blame(&repo, &p.path)
                .await
                .map_err(RpcErrorBody::from)?;
            value(lines)
        }
        "git_stash_list" => {
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let entries = svc
                .backend()
                .stash_list(&repo)
                .await
                .map_err(RpcErrorBody::from)?;
            value(entries)
        }
        "git_set_active_repo" => {
            let p: GitSetActiveRepoParams = from_params(params)?;
            state
                .gateway
                .git_service()
                .set_active_repo(Some(p.repo_id))
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Git);
            value(())
        }
        "git_add" => {
            let p: GitAddParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            svc.backend()
                .stage(&repo, &p.paths.unwrap_or_default(), p.all.unwrap_or(false))
                .await
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Git);
            value(())
        }
        "git_unstage" => {
            let p: GitUnstageParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            svc.backend()
                .unstage(&repo, &p.paths.unwrap_or_default())
                .await
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Git);
            value(())
        }
        "git_restore" => {
            let p: GitRestoreParams = from_params(params)?;
            if p.paths.is_empty() {
                return Err(bad_request("git_restore requires at least one path"));
            }
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let outcome = svc
                .confirm()
                .request_and_wait(
                    GitOpKind::Checkout,
                    "撤销工作区改动".into(),
                    json!({ "paths": p.paths.clone() }),
                )
                .await
                .map_err(RpcErrorBody::from)?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .restore(&repo, &p.paths)
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
            }
            value(())
        }
        "git_commit" => {
            let p: GitCommitParams = from_params(params)?;
            if p.message.trim().is_empty() {
                return Err(bad_request("commit message must not be empty"));
            }
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let detail = match svc.backend().diff(&repo, true, None).await {
                Ok(d) => json!({
                    "staged_files": d.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                    "truncated": d.truncated,
                }),
                Err(_) => json!({ "staged_files": [] }),
            };
            let outcome = svc
                .confirm()
                .request_and_wait(GitOpKind::Commit, "提交暂存区改动".into(), detail)
                .await
                .map_err(RpcErrorBody::from)?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .commit(&repo, &p.message)
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
            }
            value(())
        }
        "git_reset" => {
            let p: GitResetParams = from_params(params)?;
            let reset_mode = GitResetMode::parse(&p.mode).map_err(RpcErrorBody::from)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            if (reset_mode == GitResetMode::Hard || reset_mode == GitResetMode::Keep)
                && !svc.dangerous_writes()
            {
                return Err(bad_request(
                    "git reset --hard/--keep 会丢弃工作区改动，属危险写操作且默认关闭；请先开启「危险写操作」开关或改用 --soft/--mixed",
                ));
            }
            let detail = if reset_mode == GitResetMode::Hard || reset_mode == GitResetMode::Keep {
                match svc.backend().status(&repo).await {
                    Ok(s) => json!({
                        "lost": s.staged.into_iter().chain(s.unstaged).map(|e| e.path).collect::<Vec<_>>(),
                    }),
                    Err(_) => json!({ "lost": [] }),
                }
            } else {
                json!({ "lost": [] })
            };
            let outcome = svc
                .confirm()
                .request_and_wait(
                    GitOpKind::Reset,
                    format!(
                        "重置到 {}（--{}）",
                        p.target.as_deref().unwrap_or("HEAD"),
                        reset_mode.as_str()
                    ),
                    detail,
                )
                .await
                .map_err(RpcErrorBody::from)?;
            if outcome == ConfirmOutcome::Approved {
                let preview = svc
                    .backend()
                    .reset(&repo, reset_mode, p.target.as_deref())
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
                value(preview)
            } else {
                value(crate::fileops::gitops::GitResetPreview::default())
            }
        }
        "git_checkout" => {
            let p: GitCheckoutParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let dirty = match svc.backend().status(&repo).await {
                Ok(s) => !s.unstaged.is_empty() || !s.untracked.is_empty(),
                Err(_) => false,
            };
            if dirty {
                if !svc.dangerous_writes() {
                    return Err(bad_request(
                        "checkout 将覆盖未提交改动，属危险写操作且默认关闭；请先提交/暂存改动或开启「危险写操作」开关",
                    ));
                }
                let outcome = svc
                    .confirm()
                    .request_and_wait(
                        GitOpKind::Checkout,
                        format!("切换到 {}（将覆盖未提交改动）", p.target),
                        json!({ "target": p.target.clone() }),
                    )
                    .await
                    .map_err(RpcErrorBody::from)?;
                if outcome == ConfirmOutcome::Approved {
                    svc.backend()
                        .checkout(&repo, &p.target)
                        .await
                        .map_err(RpcErrorBody::from)?;
                    (state.state_emit)(StateChange::Git);
                }
            } else {
                svc.backend()
                    .checkout(&repo, &p.target)
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
            }
            value(())
        }
        "git_stash" => {
            let p: GitStashParams = from_params(params)?;
            let stash_action = GitStashAction::parse(&p.action).map_err(RpcErrorBody::from)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            match stash_action {
                GitStashAction::Push | GitStashAction::Apply => {
                    svc.backend()
                        .stash(&repo, stash_action, p.message.as_deref())
                        .await
                        .map_err(RpcErrorBody::from)?;
                    (state.state_emit)(StateChange::Git);
                }
                GitStashAction::Pop => {
                    let outcome = svc
                        .confirm()
                        .request_and_wait(
                            GitOpKind::StashApply,
                            "应用并移除最新 stash".into(),
                            json!({}),
                        )
                        .await
                        .map_err(RpcErrorBody::from)?;
                    if outcome == ConfirmOutcome::Approved {
                        svc.backend()
                            .stash(&repo, stash_action, None)
                            .await
                            .map_err(RpcErrorBody::from)?;
                        (state.state_emit)(StateChange::Git);
                    }
                }
                GitStashAction::Drop => {
                    let outcome = svc
                        .confirm()
                        .request_and_wait(GitOpKind::StashDrop, "丢弃最新 stash".into(), json!({}))
                        .await
                        .map_err(RpcErrorBody::from)?;
                    if outcome == ConfirmOutcome::Approved {
                        svc.backend()
                            .stash(&repo, stash_action, None)
                            .await
                            .map_err(RpcErrorBody::from)?;
                        (state.state_emit)(StateChange::Git);
                    }
                }
            }
            value(())
        }
        "git_push" => {
            let p: GitPushParams = from_params(params)?;
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let detail = match svc.backend().status(&repo).await {
                Ok(s) => json!({ "branch": s.branch, "ahead": s.ahead }),
                Err(_) => json!({}),
            };
            let outcome = svc
                .confirm()
                .request_and_wait(GitOpKind::Push, "推送到远程分支".into(), detail)
                .await
                .map_err(RpcErrorBody::from)?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .push(&repo, p.remote.as_deref(), p.branch.as_deref())
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
            }
            value(())
        }
        "git_pull" => {
            let svc = state.gateway.git_service();
            let repo = svc.active_repo().await.map_err(RpcErrorBody::from)?;
            let outcome = svc
                .confirm()
                .request_and_wait(GitOpKind::Pull, "拉取并合并远程改动".into(), json!({}))
                .await
                .map_err(RpcErrorBody::from)?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .pull(&repo)
                    .await
                    .map_err(RpcErrorBody::from)?;
                (state.state_emit)(StateChange::Git);
            }
            value(())
        }
        "git_resolve_conflict" => {
            let p: GitResolveConflictParams = from_params(params)?;
            let take = ConflictTake::parse(&p.take).map_err(RpcErrorBody::from)?;
            let svc = state.gateway.git_service();
            let repo = match p.repo_id {
                Some(id) => svc.repo_by_id(&id).map_err(RpcErrorBody::from)?,
                None => svc.active_repo().await.map_err(RpcErrorBody::from)?,
            };
            svc.backend()
                .resolve_conflict(&repo, &p.path, take)
                .await
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Git);
            value(())
        }
        "git_confirm" => {
            let p: GitConfirmParams = from_params(params)?;
            state
                .gateway
                .git_service()
                .confirm()
                .resolve(&p.op_id, p.approved)
                .map_err(RpcErrorBody::from)?;
            value(())
        }
        "git_get_confirm_config" => {
            value(json!({
                "dangerous_writes": state.gateway.git_service().dangerous_writes(),
            }))
        }
        "git_set_dangerous_writes" => {
            let p: GitSetDangerousWritesParams = from_params(params)?;
            state
                .gateway
                .set_git_dangerous_writes(p.enabled)
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Git);
            value(json!({ "dangerous_writes": p.enabled }))
        }

        _ => Err(RpcErrorBody {
            code: "unknown_command".into(),
            message: format!("unknown command: {cmd}"),
        }),
    }
}

// ── helpers ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModeParams {
    mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewParams<T> {
    view: T,
}

// ── Workspace / Files 参数（字段名与前端 invoke 一致）──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootParams {
    root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IgnoreParams {
    id: String,
    ignore: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsListParams {
    path: Option<String>,
    ignore: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsReadParams {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsWriteParams {
    path: String,
    content: String,
    base_mtime: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsPathParams {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsDeleteParams {
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsMoveParams {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsGlobParams {
    pattern: String,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsGrepParams {
    pattern: String,
    path: Option<String>,
    case_sensitive: Option<bool>,
    multiline: Option<bool>,
    glob: Option<String>,
    context: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FsSemanticSearchParams {
    query: String,
    top_k: Option<usize>,
    path: Option<String>,
}

// ── Git 参数（字段名与前端 invoke 一致）──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitStatusParams {
    repo_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitDiffParams {
    repo_id: Option<String>,
    path: Option<String>,
    cached: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitLogParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitShowFilesParams {
    repo_id: Option<String>,
    hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitShowDiffParams {
    repo_id: Option<String>,
    hash: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitBlameParams {
    repo_id: Option<String>,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitSetActiveRepoParams {
    repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitAddParams {
    paths: Option<Vec<String>>,
    all: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitUnstageParams {
    paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitRestoreParams {
    paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitCommitParams {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitResetParams {
    mode: String,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitCheckoutParams {
    target: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitStashParams {
    action: String,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitPushParams {
    remote: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitResolveConflictParams {
    repo_id: Option<String>,
    path: String,
    take: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitConfirmParams {
    op_id: String,
    approved: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitSetDangerousWritesParams {
    enabled: bool,
}

/// fs_* 命令统一以 active workspace 为根。
fn require_active(store: &WorkspaceStore) -> Result<WorkspaceEntry, RpcErrorBody> {
    store
        .active()
        .map_err(RpcErrorBody::from)?
        .ok_or_else(|| bad_request("no active workspace; add one before using file commands"))
}

fn conv_mode(mode: &str) -> ConversationMode {
    match mode.to_lowercase().as_str() {
        "agent" => ConversationMode::Agent,
        "assistant" => ConversationMode::Assistant,
        "system" => ConversationMode::System,
        _ => ConversationMode::Chat,
    }
}
