pub mod core;
pub mod fileops;
pub mod net;
pub mod runtime;
pub mod terminal;
pub mod tui;

use crate::core::{
    app_log::{self, LogEntry},
    assistant_session::AssistantSession,
    config::ConfigStore,
    conversation_store::ConversationStore,
    error::AppErrorPayload,
    insert_catalog::{InsertCatalog, InsertInfo},
    neuron_manager::NeuronManager,
    poller::Poller,
    providers::{ProviderConfigView, ProviderRegistry},
    session_tracker::{RunningSession, SessionTracker},
    storage,
    tool_config::ToolConfigView,
    topic_store::TopicStore,
    ChatModelSelection, ChatOptions, ChatResponse, Connection, Conversation, ConversationMode,
    Gateway, McpServerStatus, Message, ModelCallRequest, ModelCallResponse, ModelInfo, Neuron,
    NeuronCreate, NeuronKindFilter, NeuronPage, NeuronSubgraph, NeuronUpdate, PollerStatus,
    ProviderInfo, RuntimeStatus, SamplingParams, SessionBehavior, SessionSeed, SkillInfo, StateChange,
    StateEmitter, ThinkingConfig, ToolInfo, Topic, TopicStatus, TopicUpdate, STATE_CHANGED_EVENT,
};
use crate::fileops::fs::{
    FsEntry, FsInfo, FsMatch, FsReadResult, FsSuggestEntry, FsWriteResult, GrepMatch,
};
use crate::fileops::workspace::{WorkspaceEntry, WorkspaceView};
use crate::net::{NetState, ServerConfig};
use crate::terminal::commands::{
    terminal_kill, terminal_list, terminal_resize, terminal_spawn, terminal_write,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};
use tauri::{Emitter, Manager, State};
use tokio::sync::broadcast;

type TauriResult<T> = Result<T, AppErrorPayload>;

// ── Debug ──

#[tauri::command]
fn debug_storage_path() -> String {
    format!(
        "cwd={:?} storage={:?}",
        std::env::current_dir(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(storage::STORAGE_DIR_NAME)
    )
}

// ── Chat ──

#[tauri::command]
async fn send_chat_message(
    gateway: State<'_, Gateway>,
    _state_emit: State<'_, StateEmitter>,
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
    params: Option<SamplingParams>,
    thinking: Option<ThinkingConfig>,
) -> TauriResult<ChatResponse> {
    // Gateway is shared via Tauri State (Arc); send_model_message_stream is &self and
    // clone-outs before network await — no outer Mutex held across I/O.
    // 流式增量（MessageDelta）与完成后的收敛（Conversations）均由 Gateway 内部广播。
    gateway
        .inner()
        .send_model_message_stream(
            message,
            ChatOptions {
                provider_id,
                model_id,
                conversation_id,
                params,
                thinking,
            },
        )
        .await
        .map_err(|error| error.payload())
}

/// 持久化会话级模型选择（后端持有）：前端改选时调用，写 `extra.session.state.model`。
#[tauri::command]
async fn set_session_model(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    conversation_id: String,
    selection: ChatModelSelection,
) -> TauriResult<()> {
    gateway
        .inner()
        .set_session_model(&conversation_id, &selection)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations {
        affected: vec![conversation_id.clone()],
    });
    Ok(())
}

#[tauri::command]
async fn create_conversation(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    mode: String,
) -> TauriResult<String> {
    let conv_mode = match mode.to_lowercase().as_str() {
        "agent" => ConversationMode::Agent,
        "assistant" => ConversationMode::Assistant,
        "system" => ConversationMode::System,
        _ => ConversationMode::Chat,
    };
    let conversation_id = gateway
        .inner()
        .create_new_conversation(conv_mode)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations {
        affected: vec![conversation_id.clone()],
    });
    Ok(conversation_id)
}

#[tauri::command]
async fn close_session(
    sessions: State<'_, SessionTracker>,
    state_emit: State<'_, StateEmitter>,
    session_id: String,
) -> TauriResult<String> {
    let session_id = sessions
        .inner()
        .close(&session_id)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Sessions);
    Ok(session_id)
}

#[tauri::command]
async fn list_running_sessions(
    sessions: State<'_, SessionTracker>,
) -> TauriResult<Vec<RunningSession>> {
    sessions.inner().list().map_err(|error| error.payload())
}

// ── Info ──

#[tauri::command]
async fn list_skills(gateway: State<'_, Gateway>) -> TauriResult<Vec<SkillInfo>> {
    Ok(gateway.inner().list_skills())
}

/// 工具治理视图：全量工具（native / config / mcp）。
#[tauri::command]
async fn list_tools(gateway: State<'_, Gateway>) -> TauriResult<Vec<ToolInfo>> {
    Ok(gateway.inner().list_tool_info())
}

/// MCP server 连接状态（装配期与运行期重装配后均可读取）。
#[tauri::command]
async fn list_mcp_servers(gateway: State<'_, Gateway>) -> TauriResult<Vec<McpServerStatus>> {
    Ok(gateway.inner().mcp_server_statuses())
}

/// 读取当前工具配置（供前端弹窗编辑）。
#[tauri::command]
async fn get_tool_config(gateway: State<'_, Gateway>) -> TauriResult<ToolConfigView> {
    gateway.inner().get_tool_config().map_err(|e| e.payload())
}

/// 保存工具配置：校验 → 原子写回 JSON → 全量重装配（保存即生效，无需重启）。
/// 校验失败时拒绝保存，前端展示可读错误。
#[tauri::command]
async fn save_tool_config(
    gateway: State<'_, Gateway>,
    view: ToolConfigView,
) -> TauriResult<ToolConfigView> {
    gateway
        .inner()
        .save_tool_config(view)
        .await
        .map_err(|e| e.payload())
}

/// 重新装配：读取磁盘配置并全量重建工具集（不写文件）。
/// 供前端「刷新」按钮使用，配置非法时返回可读错误。
#[tauri::command]
async fn reassemble_tools(gateway: State<'_, Gateway>) -> TauriResult<()> {
    gateway
        .inner()
        .reassemble_tools()
        .await
        .map_err(|e| e.payload())
}

#[tauri::command]
async fn list_providers(providers: State<'_, ProviderRegistry>) -> TauriResult<Vec<ProviderInfo>> {
    Ok(providers.inner().list_providers())
}

#[tauri::command]
async fn list_models(
    providers: State<'_, ProviderRegistry>,
    provider_id: Option<String>,
) -> TauriResult<Vec<ModelInfo>> {
    providers
        .inner()
        .list_models(provider_id.as_deref())
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn call_model(
    providers: State<'_, ProviderRegistry>,
    request: ModelCallRequest,
) -> TauriResult<ModelCallResponse> {
    providers
        .inner()
        .call_model(request)
        .await
        .map_err(|error| error.payload())
}

/// 读取服务商/模型的完整可编辑配置（供 main 区编辑器初始化；api_key 掩码回显）。
#[tauri::command]
async fn get_provider_config(
    providers: State<'_, ProviderRegistry>,
) -> TauriResult<ProviderConfigView> {
    providers
        .inner()
        .get_config_view()
        .map_err(|error| error.payload())
}

/// 保存服务商/模型配置：校验 → 原子写回 config.json → 热重载（保存即生效）。
/// 校验失败拒绝保存并返回可读错误；保存成功后广播 Providers 事件。
#[tauri::command]
async fn save_provider_config(
    providers: State<'_, ProviderRegistry>,
    state_emit: State<'_, StateEmitter>,
    view: ProviderConfigView,
) -> TauriResult<ProviderConfigView> {
    let saved = providers
        .inner()
        .save_config(view)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Providers);
    Ok(saved)
}

#[tauri::command]
async fn list_conversations(store: State<'_, ConversationStore>) -> TauriResult<Vec<Conversation>> {
    store
        .inner()
        .list_conversations()
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn history(
    gateway: State<'_, Gateway>,
    conversation_id: Option<String>,
) -> TauriResult<Vec<Message>> {
    gateway
        .inner()
        .history(conversation_id)
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn clear_conversation(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    conversation_id: Option<String>,
) -> TauriResult<String> {
    let conversation_id = gateway
        .inner()
        .clear_conversation(conversation_id)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations {
        affected: vec![conversation_id.clone()],
    });
    Ok(conversation_id)
}

#[tauri::command]
async fn status(gateway: State<'_, Gateway>) -> TauriResult<RuntimeStatus> {
    gateway.inner().status().map_err(|error| error.payload())
}

// ── Topic ──

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

#[tauri::command]
async fn list_topics(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    status: Option<String>,
) -> TauriResult<Vec<Topic>> {
    let filter = topic_status_filter(status.as_deref());
    with_topic_store(&topic_store, |store| store.list(filter))
}

#[tauri::command]
async fn get_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    id: String,
) -> TauriResult<Topic> {
    with_topic_store(&topic_store, |store| {
        store.get(&id)?.ok_or_else(|| {
            crate::core::AppError::ConversationNotFound(format!("Topic not found: {id}"))
        })
    })
}

#[tauri::command]
async fn create_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    name: String,
    description: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| {
        store.create(&name, &description, TopicStatus::Todo, vec![], None)
    })
}

#[tauri::command]
async fn update_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| {
        store.update(
            &id,
            TopicUpdate {
                name,
                description,
                extra: None,
            },
        )
    })
}

#[tauri::command]
async fn delete_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    id: String,
) -> TauriResult<bool> {
    // 课题变更事件由 TopicStore 写操作统一广播（删除成功时）。
    with_topic_store(&topic_store, |store| store.delete(&id))
}

#[tauri::command]
async fn add_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    topic_id: String,
    goal: String,
    done_contract: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| {
        store.add_scope_item(&topic_id, &goal, &done_contract)
    })
}

#[tauri::command]
async fn delete_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| {
        store.delete_scope_item(&topic_id, &item_id)
    })
}

#[tauri::command]
async fn complete_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| {
        store.complete_scope_item(&topic_id, &item_id)
    })
}

#[tauri::command]
async fn pause_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    id: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播（set_status 内部）。
    with_topic_store(&topic_store, |store| store.pause(&id))
}

#[tauri::command]
async fn resume_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    id: String,
) -> TauriResult<Topic> {
    // 课题变更事件由 TopicStore 写操作统一广播。
    with_topic_store(&topic_store, |store| store.resume(&id))
}

// ── Poller ──

#[tauri::command]
async fn poll_status(poller: State<'_, Arc<StdMutex<Poller>>>) -> TauriResult<PollerStatus> {
    with_poller(&poller, |p| Ok(p.status()))
}

#[tauri::command]
async fn poll_pause(
    poller: State<'_, Arc<StdMutex<Poller>>>,
    state_emit: State<'_, StateEmitter>,
) -> TauriResult<()> {
    let status = with_poller(&poller, |p| {
        p.pause();
        Ok(p.status())
    })?;
    state_emit.inner()(StateChange::Poller { status });
    Ok(())
}

#[tauri::command]
async fn poll_resume(
    poller: State<'_, Arc<StdMutex<Poller>>>,
    state_emit: State<'_, StateEmitter>,
) -> TauriResult<()> {
    let status = with_poller(&poller, |p| {
        p.resume();
        Ok(p.status())
    })?;
    state_emit.inner()(StateChange::Poller { status });
    Ok(())
}

#[tauri::command]
async fn poll_trigger(
    poller: State<'_, Arc<StdMutex<Poller>>>,
    state_emit: State<'_, StateEmitter>,
) -> TauriResult<()> {
    let status = with_poller(&poller, |p| {
        p.trigger();
        Ok(p.status())
    })?;
    state_emit.inner()(StateChange::Poller { status });
    Ok(())
}

/// 设置轮询并发推进数量（clamp 到 1..=8），持久化并运行时生效。
#[tauri::command]
async fn poll_set_parallelism(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    n: u64,
) -> TauriResult<u64> {
    let clamped = gateway
        .inner()
        .set_poll_parallelism(n)
        .map_err(|error| error.payload())?;
    let status = gateway
        .inner()
        .poll_status()
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Poller { status });
    Ok(clamped)
}

// ── Neuron ──

#[tauri::command]
async fn list_neurons(mgr: State<'_, Arc<NeuronManager>>) -> TauriResult<Vec<Neuron>> {
    mgr.inner().list_neurons().map_err(|error| error.payload())
}

#[tauri::command]
async fn get_neuron(mgr: State<'_, Arc<NeuronManager>>, id: String) -> TauriResult<Neuron> {
    mgr.inner()
        .get_neuron(&id)
        .map_err(|error| error.payload())?
        .ok_or_else(|| crate::core::AppError::NeuronNotFound(id).payload())
}

#[tauri::command]
async fn update_neuron(
    mgr: State<'_, Arc<NeuronManager>>,
    id: String,
    desc: Option<String>,
    content: Option<String>,
    tool_ids: Option<Vec<String>>,
) -> TauriResult<Neuron> {
    mgr.inner()
        .update_content_for_admin(
            &id,
            NeuronUpdate {
                desc,
                content,
                tool_ids,
            },
        )
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn get_connections(
    mgr: State<'_, Arc<NeuronManager>>,
    id: String,
) -> TauriResult<Vec<Connection>> {
    mgr.inner()
        .get_connections(&id)
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn get_network(
    mgr: State<'_, Arc<NeuronManager>>,
    id: String,
    max_depth: Option<usize>,
) -> TauriResult<NeuronSubgraph> {
    mgr.inner()
        .get_network(&id, max_depth.unwrap_or(2))
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn create_neuron_plain(
    mgr: State<'_, Arc<NeuronManager>>,
    desc: String,
    content: Option<String>,
    link_to: Option<String>,
    tool_ids: Vec<String>,
) -> TauriResult<Neuron> {
    let create = NeuronCreate {
        desc,
        content: content.unwrap_or_default(),
        weight: 0.0,
        system_type: None,
        tool_ids,
        lineage_parent_id: None,
        variant_state: None,
    };
    mgr.inner()
        .create_plain(create, link_to.as_deref())
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn adjust_neuron_weight(
    mgr: State<'_, Arc<NeuronManager>>,
    id: String,
    delta: f64,
) -> TauriResult<Neuron> {
    mgr.inner()
        .adjust_weight(&id, delta)
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn adjust_edge_weight(
    mgr: State<'_, Arc<NeuronManager>>,
    source: String,
    target: String,
    delta: f64,
) -> TauriResult<Connection> {
    mgr.inner()
        .adjust_edge_weight(&source, &target, delta)
        .map_err(|error| error.payload())
}

/// 人工评价：定位被评消息所在介入区间的盖章神经元并应用评分 delta，
/// 与模型打分 hook 共享 `apply_score_feedback`，仅分数来源不同（用户点击）。
#[tauri::command]
async fn score_feedback(
    assistant: State<'_, Arc<AssistantSession>>,
    state_emit: State<'_, StateEmitter>,
    conversation_id: String,
    message_index: usize,
    score: i64,
) -> TauriResult<()> {
    assistant
        .inner()
        .score_feedback(&conversation_id, message_index, score)
        .await
        .map_err(|error| {
            tracing::warn!(
                phase = "score_feedback_command",
                conversation_id,
                message_index,
                score,
                error = %error,
                "manual rating failed"
            );
            error.payload()
        })?;
    state_emit.inner()(StateChange::Neurons);
    state_emit.inner()(StateChange::Conversations {
        affected: vec![conversation_id],
    });
    Ok(())
}

// ── Session Specs ──

/// 开启会话：按 spec_neuron_id 推导种子并调用 `Gateway::start_session`。
/// - 传神经元 → `SessionSeed::Neuron(id)`（系统神经元用 behavior，普通神经元推导领域行为）；
/// - 不传 → Assistant 模式用 `Global`（全域首轮选 1），其余模式直连（`None`）。
#[tauri::command]
async fn open_session(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    spec_neuron_id: String,
    mode: String,
) -> TauriResult<Conversation> {
    let conv_mode = match mode.to_lowercase().as_str() {
        "assistant" => ConversationMode::Assistant,
        "agent" => ConversationMode::Agent,
        "system" => ConversationMode::System,
        _ => ConversationMode::Chat,
    };
    let seed = match spec_neuron_id.trim() {
        // 系统模式沿用助手模式的全域选型（Global），仅附加 System 标签工具。
        "" if conv_mode == ConversationMode::Assistant || conv_mode == ConversationMode::System => {
            Some(SessionSeed::Global)
        }
        "" => None,
        id => Some(SessionSeed::Neuron(id.to_string())),
    };
    let conversation = gateway
        .inner()
        .start_session(seed, conv_mode)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations {
        affected: vec![conversation.id.clone()],
    });
    Ok(conversation)
}

/// 管理面分页列表（分页 + 搜索 + 类型筛选 all/system/normal），供列表视图增量加载。
#[tauri::command]
async fn list_neurons_page(
    mgr: State<'_, Arc<NeuronManager>>,
    page: usize,
    page_size: usize,
    search: Option<String>,
    kind: String,
) -> TauriResult<NeuronPage> {
    mgr.inner()
        .list_neurons_page(page, page_size, search.as_deref(), NeuronKindFilter::parse(&kind))
        .map_err(|error| error.payload())
}

/// 管理面设置 / 换绑 / 取消系统类型（空串或 None 视为取消绑定）。
#[tauri::command]
async fn set_neuron_system_type(
    mgr: State<'_, Arc<NeuronManager>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
    system_type: Option<String>,
) -> TauriResult<Neuron> {
    let neuron = mgr
        .inner()
        .set_system_type_for_admin(&id, system_type.as_deref())
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Neurons);
    Ok(neuron)
}

/// 管理面更新系统神经元行为（所有 system_type 非空的神经元可写，含裁决类）。
#[tauri::command]
async fn update_neuron_behavior(
    mgr: State<'_, Arc<NeuronManager>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
    behavior: SessionBehavior,
) -> TauriResult<Neuron> {
    let neuron = mgr
        .inner()
        .update_behavior_for_admin(&id, behavior)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Neurons);
    Ok(neuron)
}

/// 契约段目录：全部可用 insert id 与一句话说明（供前端下拉选择，替代自由输入）。
#[tauri::command]
fn list_insert_catalog() -> Vec<InsertInfo> {
    InsertCatalog::catalog()
}

// ── Workspace / Files ──

/// 当前 active 工作区（fs_* 命令统一以 active workspace 为根）。
fn require_active_workspace(
    store: &crate::fileops::workspace::WorkspaceStore,
) -> TauriResult<WorkspaceEntry> {
    store
        .active()
        .map_err(|error| error.payload())?
        .ok_or_else(|| {
            crate::core::AppError::InvalidInput(
                "no active workspace; add one via the Files view before using file commands".into(),
            )
            .payload()
        })
}

#[tauri::command]
async fn list_workspaces(gateway: State<'_, Gateway>) -> TauriResult<WorkspaceView> {
    gateway
        .inner()
        .workspace_store()
        .view()
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn add_workspace(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    root: String,
) -> TauriResult<WorkspaceView> {
    let view = gateway
        .inner()
        .workspace_store()
        .add(&root)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(view)
}

#[tauri::command]
async fn remove_workspace(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    id: String,
) -> TauriResult<WorkspaceView> {
    let view = gateway
        .inner()
        .workspace_store()
        .remove(&id)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(view)
}

#[tauri::command]
async fn set_active_workspace(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    id: String,
) -> TauriResult<WorkspaceView> {
    let view = gateway
        .inner()
        .workspace_store()
        .set_active(&id)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(view)
}

#[tauri::command]
async fn update_workspace_ignore(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    id: String,
    ignore: Vec<String>,
) -> TauriResult<WorkspaceView> {
    let view = gateway
        .inner()
        .workspace_store()
        .update_ignore(&id, ignore)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(view)
}

#[tauri::command]
async fn fs_list(
    gateway: State<'_, Gateway>,
    path: Option<String>,
    ignore: Option<Vec<String>>,
) -> TauriResult<Vec<FsEntry>> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .list(&ws, path.as_deref(), ignore.as_deref())
        .map_err(|error| error.payload())
}

// ── 路径补全（添加工作区输入框：绝对路径建议）──

fn home_dir_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[tauri::command]
fn get_home_dir() -> TauriResult<String> {
    home_dir_path()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| AppErrorPayload {
            code: "no_home_dir",
            message: "无法获取用户主目录".into(),
        })
}

/// 列出绝对路径的直接子项（不递归）。目录不存在/无权限时返回错误，
/// 前端逐级向父目录回退以继续给出候选。
#[tauri::command]
async fn fs_suggest_abs(path: String) -> TauriResult<Vec<FsSuggestEntry>> {
    crate::fileops::fs::list_suggest(&path).map_err(|message| AppErrorPayload {
        code: "fs_suggest_failed",
        message,
    })
}

#[tauri::command]
async fn fs_read(
    gateway: State<'_, Gateway>,
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> TauriResult<FsReadResult> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .read(&ws, &path, offset, limit)
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn fs_write(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    path: String,
    content: String,
    base_mtime: Option<i64>,
) -> TauriResult<FsWriteResult> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    let result = gateway
        .inner()
        .file_system()
        .write(&ws, &path, &content, base_mtime)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(result)
}

#[tauri::command]
async fn fs_create_dir(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    path: String,
) -> TauriResult<()> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .create_dir(&ws, &path)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(())
}

#[tauri::command]
async fn fs_delete(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    paths: Vec<String>,
) -> TauriResult<()> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .delete(&ws, &paths)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(())
}

#[tauri::command]
async fn fs_rename(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    from: String,
    to: String,
) -> TauriResult<()> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .rename(&ws, &from, &to)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(())
}

#[tauri::command]
async fn fs_move(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    from: String,
    to: String,
) -> TauriResult<()> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .rename(&ws, &from, &to)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Workspaces);
    Ok(())
}

#[tauri::command]
async fn fs_glob(
    gateway: State<'_, Gateway>,
    pattern: String,
    cwd: Option<String>,
) -> TauriResult<Vec<FsMatch>> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .glob(&ws, &pattern, cwd.as_deref())
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn fs_grep(
    gateway: State<'_, Gateway>,
    pattern: String,
    path: Option<String>,
    case_sensitive: Option<bool>,
    multiline: Option<bool>,
    glob: Option<String>,
    context: Option<usize>,
) -> TauriResult<Vec<GrepMatch>> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .grep(
            &ws,
            &pattern,
            path.as_deref(),
            case_sensitive.unwrap_or(false),
            multiline.unwrap_or(false),
            glob.as_deref(),
            context.unwrap_or(0),
        )
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn fs_info(
    gateway: State<'_, Gateway>,
    path: String,
) -> TauriResult<FsInfo> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    gateway
        .inner()
        .file_system()
        .info(&ws, &path)
        .map_err(|error| error.payload())
}

// ── Logs ──

#[tauri::command]
fn logs_snapshot() -> Vec<LogEntry> {
    app_log::snapshot()
}

#[tauri::command]
fn logs_get_level() -> String {
    app_log::get_level()
}

#[tauri::command]
fn logs_set_level(level: String) -> TauriResult<String> {
    app_log::set_level(&level)
        .map_err(|message| crate::core::AppError::InvalidInput(message).payload())
}

#[tauri::command]
fn logs_clear_buffer() {
    app_log::clear_buffer();
}

#[tauri::command]
fn logs_dir() -> Option<String> {
    app_log::log_dir().map(|path| path.display().to_string())
}

// ── Helpers ──

fn with_topic_store<T>(
    topic_store: &Arc<StdMutex<TopicStore>>,
    action: impl FnOnce(&TopicStore) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let store = topic_store.lock().map_err(|_| {
        crate::core::AppError::RuntimeError("TopicStore lock failed".into()).payload()
    })?;
    action(&store).map_err(|error| error.payload())
}

fn with_poller<T>(
    poller: &Arc<StdMutex<Poller>>,
    action: impl FnOnce(&mut Poller) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let mut guard = poller.lock().map_err(|e| {
        crate::core::AppError::StorageError(format!("Poller lock error: {e}")).payload()
    })?;
    action(&mut guard).map_err(|error| error.payload())
}

// ── App Entry ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let storage_root = storage::resolve(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri has a parent"),
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let emit_handle = handle.clone();
            let emit = Arc::new(move |entry: LogEntry| {
                let _ = emit_handle.emit(app_log::LOG_EVENT, entry);
            });
            if let Err(error) = app_log::init(&storage_root, Some(emit), true) {
                eprintln!("warning: failed to init logging: {error}");
            }
            tracing::info!(
                path = %storage_root.display(),
                "pulsar logging initialized"
            );

            // WebKitGTK 磁盘缓存不完全遵守 no-store（实测重启后仍按 URL 复用旧 CSS/JS，
            // 导致整个 App 样式混搭）；每次启动在建窗前清掉 WebKitCache，保证加载最新 bundle。
            if let Ok(data_dir) = app.path().app_data_dir() {
                let webkit_cache = data_dir.join("WebKitCache");
                if webkit_cache.exists() {
                    match std::fs::remove_dir_all(&webkit_cache) {
                        Ok(()) => tracing::info!("cleared stale webkit disk cache at startup"),
                        Err(error) => tracing::warn!(
                            error = %error,
                            "failed to clear webkit disk cache at startup"
                        ),
                    }
                }
            }

            // 远程模式：内嵌 server 配置（config.json `server` 节）。缺省 / enabled=false 不启动，
            // 等价现状（本机 Tauri IPC 路径零改动）。
            let server_cfg = ConfigStore::new(storage_root.clone())
                .read()
                .ok()
                .and_then(|config| config.server)
                .filter(|section| section.enabled.unwrap_or(false))
                .map(|section| ServerConfig {
                    host: section.host.unwrap_or_else(|| "127.0.0.1".into()),
                    port: section.port.unwrap_or(8787),
                    tokens: section.tokens.unwrap_or_default(),
                });
            let server_enabled = server_cfg.is_some();

            // 统一状态事件发射器：command 层写操作与后台推进完成后广播，
            // 前端 dataStore 监听 STATE_CHANGED_EVENT 并按 kind 重新拉取。
            // 远程模式启用时同时注入 broadcast 通道，供 SSE 转发。
            let state_emit_handle = handle.clone();
            let (events_tx, _events_rx) = broadcast::channel::<StateChange>(256);
            let events_tx_for_emit = events_tx.clone();
            let state_emit: StateEmitter = Arc::new(move |change: StateChange| {
                if server_enabled {
                    let _ = events_tx_for_emit.send(change.clone());
                }
                let _ = state_emit_handle.emit(STATE_CHANGED_EVENT, change);
            });

            let store = ConversationStore::new(&storage_root).map_err(|error| error.to_string())?;
            // 终端桥接（方案 A）：先于 Gateway 创建，注入 execute_command 可见执行能力；
            // 同一 manager 由 tauri command 层（app.manage）与 Agent 工具桥接共用。
            let terminal_manager = Arc::new(terminal::TerminalManager::new());
            let terminal_hub = terminal::TerminalEventHub::new(handle.clone());
            let terminal_bridge = Arc::new(terminal::AgentTerminalBridge::new(
                Arc::clone(&terminal_manager),
                terminal_hub.clone(),
            ));
            let gateway = Gateway::with_state_emitter_and_terminal(
                store,
                Some(state_emit.clone()),
                Some(terminal_bridge),
            )
            .map_err(|error| error.to_string())?;

            // Domain states (no outer Mutex across network).
            let neuron_manager = gateway.neuron_manager();
            let topic_store = gateway.topic_store().map_err(|e| e.to_string())?;
            let assistant = gateway.assistant();
            let poller = gateway.poller();
            let sessions = gateway.session_tracker();
            let providers = gateway.providers();
            let conversation_store = gateway.conversation_store();

            app.manage(neuron_manager.clone());
            app.manage(topic_store);
            app.manage(assistant);
            app.manage(poller);
            app.manage(sessions);
            app.manage(providers);
            app.manage(conversation_store);
            // 终端事件 hub：IPC 命令与 WS 公共通道（net/ws.rs）共享的会话事件广播器。
            app.manage(terminal_hub.clone());
            // 终端浏览器支持：随内嵌 server 的 `/ws` 端点启动（net::NetState 注入
            // terminal manager 与 hub，见下方远程模式分支）；不再独立监听端口。
            let ws_manager = Arc::clone(&terminal_manager);
            app.manage(terminal_manager);
            let gateway_for_server = gateway.clone();
            app.manage(gateway);
            let state_emit_for_server = state_emit.clone();
            app.manage(state_emit);

            // 远程模式：条件启动内嵌 server（持有 Gateway / StateEmitter 克隆、SSE 广播
            // 通道与终端会话；`/ws` 终端业务随 server 一并启用）。
            if let Some(cfg) = server_cfg {
                let net_state = NetState {
                    gateway: gateway_for_server,
                    state_emit: state_emit_for_server,
                    events_tx: events_tx.clone(),
                    tokens: cfg.tokens.clone(),
                    terminal: ws_manager,
                    terminal_hub: terminal_hub.clone(),
                };
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = net::run_server(cfg, net_state).await {
                        tracing::error!(
                            error = %error,
                            "network server exited; remote mode unavailable"
                        );
                    }
                });
            }

            // Bootstrap without holding any Gateway lock across model calls.
            tauri::async_runtime::spawn(async move {
                tracing::info!(phase = "bootstrap_neurons", "starting neuron bootstrap");
                match neuron_manager.bootstrap().await {
                    Ok(report) => {
                        tracing::info!(
                            phase = "bootstrap_neurons",
                            create_neuron_id = %report.create_neuron_id,
                            select_neuron_id = %report.select_neuron_id,
                            "neuron bootstrap complete"
                        );
                    }
                    Err(error) => {
                        tracing::warn!(
                            phase = "bootstrap_neurons",
                            error_code = error.code(),
                            error = %error,
                            "neuron bootstrap incomplete"
                        );
                    }
                }
            });

            // 手动创建主窗口（tauri.conf.json 中 `"create": false`）并给所有资源响应加
            // `Cache-Control: no-store`：Tauri 的 tauri:// 协议默认不设缓存头，WebKitGTK
            // 会把旧版 index.html/CSS/JS 写入跨重启持久化的磁盘缓存（~/.local/share/<id>/WebKitCache），
            // 重启后按 URL 复用旧资源导致样式错乱。no-store 使 webview 每次都取当前 bundle。
            let window_config = app.config().app.windows[0].clone();
            tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)
                .map_err(|error| error.to_string())?
                .on_web_resource_request(|_request, response| {
                    response.headers_mut().insert(
                        http::header::CACHE_CONTROL,
                        http::header::HeaderValue::from_static("no-store"),
                    );
                })
                .build()
                .map_err(|error| error.to_string())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            debug_storage_path,
            send_chat_message,
            set_session_model,
            create_conversation,
            close_session,
            list_running_sessions,
            list_skills,
            list_tools,
            list_mcp_servers,
            get_tool_config,
            save_tool_config,
            reassemble_tools,
            list_providers,
            list_models,
            call_model,
            get_provider_config,
            save_provider_config,
            list_conversations,
            history,
            clear_conversation,
            status,
            // Topic
            list_topics,
            get_topic,
            create_topic,
            update_topic,
            delete_topic,
            add_topic_scope_item,
            delete_topic_scope_item,
            complete_topic_scope_item,
            pause_topic,
            resume_topic,
            // Poller
            poll_status,
            poll_pause,
            poll_resume,
            poll_trigger,
            poll_set_parallelism,
            // Neuron
            list_neurons,
            get_neuron,
            update_neuron,
            get_connections,
            get_network,
            create_neuron_plain,
            adjust_neuron_weight,
            adjust_edge_weight,
            score_feedback,
            // Neurons / 统一管理
            open_session,
            list_neurons_page,
            set_neuron_system_type,
            update_neuron_behavior,
            list_insert_catalog,
            // Logs
            logs_snapshot,
            logs_get_level,
            logs_set_level,
            logs_clear_buffer,
            logs_dir,
            // Workspace / Files
            list_workspaces,
            add_workspace,
            remove_workspace,
            set_active_workspace,
            update_workspace_ignore,
            fs_list,
            fs_read,
            fs_write,
            fs_create_dir,
            fs_delete,
            fs_rename,
            fs_move,
            fs_glob,
            fs_grep,
            fs_info,
            get_home_dir,
            fs_suggest_abs,
            // Terminal
            terminal_spawn,
            terminal_write,
            terminal_resize,
            terminal_kill,
            terminal_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
