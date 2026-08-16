//! 统一 RPC 端点：把 Tauri command 集合以 `POST /rpc` 暴露给远程前端。
//!
//! - 载荷：`{ cmd, params }`，`params` 字段名与前端 Tauri `invoke` 一致（camelCase），
//!   httpClient 可直接复用现有参数构造，零迁移。
//! - 每个分支实现与 `lib.rs` 对应 command 相同业务语义（调用 `Gateway` / 分域 State →
//!   成功后广播 `StateChange`）。
//! - 锁纪律与 Tauri 路径一致：不持 Gateway / 域锁跨网络 I/O，仅短临界区 `lock`。

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use crate::core::{
    app_log, insert_catalog::InsertCatalog, providers::ProviderConfigView,
    tool_config::ToolConfigView, AppError, AppResult, ChatOptions, ConversationMode,
    ModelCallRequest, NeuronCreate, NeuronKindFilter, NeuronUpdate, SessionBehavior, SessionSeed,
    StateChange, TopicStatus, TopicUpdate,
};

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
struct TopicListParams {
    status: Option<String>,
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
            let response = state
                .gateway
                .send_model_message(
                    p.message,
                    ChatOptions {
                        provider_id: p.provider_id,
                        model_id: p.model_id,
                        conversation_id: p.conversation_id,
                    },
                )
                .await
                .map_err(RpcErrorBody::from)?;
            (state.state_emit)(StateChange::Conversations {
                affected: vec![response.conversation_id.clone()],
            });
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
            (state.state_emit)(StateChange::Topics);
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
            (state.state_emit)(StateChange::Topics);
            value(topic)
        }
        "delete_topic" => {
            let p: IdParams = from_params(params)?;
            let deleted = with_topic(state, |store| store.delete(&p.id))?;
            (state.state_emit)(StateChange::Topics);
            value(deleted)
        }
        "add_topic_scope_item" => {
            let p: AddScopeItemParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.add_scope_item(&p.topic_id, &p.goal, &p.done_contract)
            })?;
            (state.state_emit)(StateChange::Topics);
            value(topic)
        }
        "delete_topic_scope_item" => {
            let p: ScopeItemParams = from_params(params)?;
            let topic =
                with_topic(state, |store| store.delete_scope_item(&p.topic_id, &p.item_id))?;
            (state.state_emit)(StateChange::Topics);
            value(topic)
        }
        "complete_topic_scope_item" => {
            let p: ScopeItemParams = from_params(params)?;
            let topic = with_topic(state, |store| {
                store.complete_scope_item(&p.topic_id, &p.item_id)
            })?;
            (state.state_emit)(StateChange::Topics);
            value(topic)
        }
        "pause_topic" => {
            let p: IdParams = from_params(params)?;
            let topic = with_topic(state, |store| store.pause(&p.id))?;
            (state.state_emit)(StateChange::Topics);
            value(topic)
        }
        "resume_topic" => {
            let p: IdParams = from_params(params)?;
            let topic = with_topic(state, |store| store.resume(&p.id))?;
            (state.state_emit)(StateChange::Topics);
            value(topic)
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

fn conv_mode(mode: &str) -> ConversationMode {
    match mode.to_lowercase().as_str() {
        "agent" => ConversationMode::Agent,
        "assistant" => ConversationMode::Assistant,
        "system" => ConversationMode::System,
        _ => ConversationMode::Chat,
    }
}
