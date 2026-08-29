pub mod core;
pub mod fileops;
pub mod net;
pub mod runtime;
pub mod server_runtime;
pub mod terminal;
pub mod tui;

use crate::core::{
    app_log::{self, LogEntry},
    assistant_session::AssistantSession,
    config::{server_env_overrides, ConfigStore, DEFAULT_SERVER_HOST, DEFAULT_SERVER_PORT},
    conversation_store::ConversationStore,
    error::AppErrorPayload,
    hook::hook_defs_meta,
    hook_judgement_store::{
        HookJudgementFilter, HookJudgementListResult, HookJudgementStore,
    },
    insert_catalog::{InsertCatalog, InsertInfo},
    neuron_manager::NeuronManager,
    poller::Poller,
    providers::{ProviderConfigView, ProviderRegistry},
    session_tracker::{RunningSession, SessionTracker},
    storage,
    tool_config::ToolConfigView,
    topic_store::TopicStore,
    ChatModelSelection, ChatOptions, ChatResponse, Connection, Conversation, ConversationMode,
    ConversationSummaryPage, Gateway, McpServerStatus, Message, MessagePage, ModelCallRequest,
    ModelCallResponse, ModelInfo, Neuron,
    NeuronCreate, NeuronKindFilter, NeuronPage, NeuronSubgraph, NeuronUpdate, PollerStatus,
    ProviderInfo, RuntimeStatus, SamplingParams, SessionBehavior, SessionSeed, SkillInfo, StateChange,
    StateEmitter, ThinkingConfig, ToolInfo, Topic, TopicStatus, TopicUpdate, STATE_CHANGED_EVENT,
};
use crate::fileops::fs::{
    FsEntry, FsInfo, FsMatch, FsReadResult, FsSuggestEntry, FsWriteResult, GrepMatch,
};
use crate::fileops::gitops::confirm::{ConfirmOutcome, GitOpKind};
use crate::fileops::gitops::{
    ConflictTake, GitBlameLine, GitBranchItem, GitCommitInfo, GitDiff, GitFileDiff, GitRepo,
    GitResetMode, GitResetPreview, GitShowFile, GitStashAction, GitStashEntry, GitStatusView,
};
use crate::fileops::search::chunk::SemanticSearchResult;
use crate::fileops::search::retriever::Retriever;
use crate::fileops::workspace::{WorkspaceEntry, WorkspaceView};
use crate::net::{NetState, ServerConfig, ServerInfo};
use crate::terminal::commands::{
    terminal_kill, terminal_list, terminal_resize, terminal_spawn, terminal_write,
};
use serde_json::json;
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

// ── Server ──

/// 服务器运行信息（桌面 IPC 版 `GET /config`）：读 config.json `server` 节 + env 覆盖，
/// 与远程 `/config` 端点同构，供前端统一展示 / 预判认证（GUI 无 CLI 层，优先级 env > config > 默认）。
#[tauri::command]
fn server_info() -> ServerInfo {
    let (env_host, env_port, env_token) = server_env_overrides();
    let section = ConfigStore::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(storage::STORAGE_DIR_NAME),
    )
    .read()
    .ok()
    .and_then(|config| config.server);
    let enabled = section.as_ref().and_then(|s| s.enabled).unwrap_or(false);
    let host = env_host
        .or_else(|| section.as_ref().and_then(|s| s.host.clone()))
        .unwrap_or_else(|| DEFAULT_SERVER_HOST.into());
    let port = env_port
        .or_else(|| section.as_ref().and_then(|s| s.port))
        .unwrap_or(DEFAULT_SERVER_PORT);
    let tokens = env_token
        .map(|t| vec![t])
        .or_else(|| section.as_ref().and_then(|s| s.tokens.clone()))
        .unwrap_or_default();
    ServerInfo {
        version: env!("CARGO_PKG_VERSION"),
        enabled,
        host,
        port,
        static_enabled: cfg!(feature = "embed-static"),
        auth_required: !tokens.is_empty(),
    }
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
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    session_id: String,
) -> TauriResult<String> {
    // 统一停止语义：取消活动轮次 + 暂停绑定课题 + 摘除运行条目（Gateway::stop_session）。
    let session_id = gateway
        .inner()
        .stop_session(&session_id)
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

/// 会话列表摘要分页（前端会话侧栏）：只读元信息（含消息条数与首条文本摘要），不携带消息正文。
#[tauri::command]
async fn list_conversation_summaries(
    store: State<'_, ConversationStore>,
    page: Option<usize>,
    page_size: Option<usize>,
) -> TauriResult<ConversationSummaryPage> {
    store
        .inner()
        .list_conversation_summaries(page.unwrap_or(0), page_size.unwrap_or(50))
        .map_err(|error| error.payload())
}

/// 消息历史分页（前端消息区）：从最新倒推切片，`offset` = 已加载条数。
#[tauri::command]
async fn history_page(
    gateway: State<'_, Gateway>,
    conversation_id: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> TauriResult<MessagePage> {
    gateway
        .inner()
        .history_page(conversation_id, limit.unwrap_or(100), offset.unwrap_or(0))
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

// ── Hook Judgements ──

/// 裁决记录分页列表（时间线倒序）。空过滤 = 全量；面板与消息卡锚点查询共用。
/// 出参为 `{ records, total }`，total 为过滤后总数，供面板分页（滚动加载）与计数消费。
#[tauri::command]
async fn hook_judgements_list(
    hook_judgement_store: State<'_, Arc<StdMutex<HookJudgementStore>>>,
    filters: Option<HookJudgementFilter>,
) -> TauriResult<HookJudgementListResult> {
    let filter = filters.unwrap_or_default();
    with_hook_judgement_store(&hook_judgement_store, |store| store.list_with_total(&filter))
}

/// Hook 元信息表（`HOOK_DEFS` 静态表出参：面板过滤下拉的数据源）。
#[tauri::command]
fn hook_defs_list() -> Vec<crate::core::hook::HookDefMeta> {
    hook_defs_meta()
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

/// 重置全部系统提示词为代码内置预设（rebootstrap）：删除重建 5 个 assistant_* 系统神经元，
/// 不重置 create_neuron 种子，普通神经元与权重不受影响。
#[tauri::command]
async fn reset_system_prompts(
    mgr: State<'_, Arc<NeuronManager>>,
    state_emit: State<'_, StateEmitter>,
) -> TauriResult<()> {
    mgr.inner()
        .rebootstrap()
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Neurons);
    Ok(())
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

// ── Git ──

/// 发现 active workspace 内全部 git 仓库（仅向内扫描，结果缓存于 GitService）。
#[tauri::command]
async fn git_repos(gateway: State<'_, Gateway>) -> TauriResult<Vec<GitRepo>> {
    gateway
        .inner()
        .git_service()
        .discover_repos()
        .await
        .map_err(|error| error.payload())
}

/// 指定仓库（repo_id）或当前操作仓库状态（缺省回落 active/第一个 repo）。
#[tauri::command]
async fn git_status(
    gateway: State<'_, Gateway>,
    repo_id: Option<String>,
) -> TauriResult<GitStatusView> {
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .status(&repo)
        .await
        .map_err(|error| error.payload())
}

/// unified diff；`cached=true` 查看暂存区，默认工作区（未暂存）。
/// `repo_id` 指定仓库（git-diff 面板按 key 内 repo 取数），缺省回落 active。
#[tauri::command]
async fn git_diff(
    gateway: State<'_, Gateway>,
    repo_id: Option<String>,
    path: Option<String>,
    cached: Option<bool>,
) -> TauriResult<GitDiff> {
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .diff(&repo, cached.unwrap_or(false), path.as_deref())
        .await
        .map_err(|error| error.payload())
}

/// 最近提交历史，默认 30 条；`offset` 支持分页（`git log --skip`）。
#[tauri::command]
async fn git_log(
    gateway: State<'_, Gateway>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> TauriResult<Vec<GitCommitInfo>> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .log(&repo, limit.unwrap_or(30), offset.unwrap_or(0))
        .await
        .map_err(|error| error.payload())
}

/// 某提交的变更文件统计列表（`git show --numstat`）。`repo_id` 缺省回落 active。
#[tauri::command]
async fn git_show_files(
    gateway: State<'_, Gateway>,
    repo_id: Option<String>,
    hash: String,
) -> TauriResult<Vec<GitShowFile>> {
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .show_files(&repo, &hash)
        .await
        .map_err(|error| error.payload())
}

/// 某提交中单个文件的 unified diff（复用 `GitFileDiff` 结构）。
#[tauri::command]
async fn git_show_diff(
    gateway: State<'_, Gateway>,
    repo_id: Option<String>,
    hash: String,
    path: String,
) -> TauriResult<GitFileDiff> {
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .show_diff(&repo, &hash, &path)
        .await
        .map_err(|error| error.payload())
}

/// 本地 + 远端分支（标记当前分支与 upstream）。
#[tauri::command]
async fn git_branches(gateway: State<'_, Gateway>) -> TauriResult<Vec<GitBranchItem>> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .branches(&repo)
        .await
        .map_err(|error| error.payload())
}

/// 单文件行级 blame（路径相对 repo 根）。
#[tauri::command]
async fn git_blame(
    gateway: State<'_, Gateway>,
    repo_id: Option<String>,
    path: String,
) -> TauriResult<Vec<GitBlameLine>> {
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .blame(&repo, &path)
        .await
        .map_err(|error| error.payload())
}

/// stash 列表。
#[tauri::command]
async fn git_stash_list(gateway: State<'_, Gateway>) -> TauriResult<Vec<GitStashEntry>> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .stash_list(&repo)
        .await
        .map_err(|error| error.payload())
}

/// 切换当前操作仓库（会话内存态）。
#[tauri::command]
async fn git_set_active_repo(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    repo_id: String,
) -> TauriResult<()> {
    gateway
        .inner()
        .git_service()
        .set_active_repo(Some(repo_id))
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(())
}

/// 暂存；`all=true` 暂存全部，或按 `paths`（相对 repo 根）暂存指定路径。
#[tauri::command]
async fn git_add(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    paths: Option<Vec<String>>,
    all: Option<bool>,
) -> TauriResult<()> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .stage(&repo, &paths.unwrap_or_default(), all.unwrap_or(false))
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(())
}

/// 从暂存区取消暂存（`git restore --staged`；paths 为空 = 取消全部）。
#[tauri::command]
async fn git_unstage(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    paths: Option<Vec<String>>,
) -> TauriResult<()> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .unstage(&repo, &paths.unwrap_or_default())
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(())
}

/// 撤销工作区改动（丢弃未提交修改；需用户确认）。
#[tauri::command]
async fn git_restore(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    paths: Vec<String>,
) -> TauriResult<()> {
    if paths.is_empty() {
        return Err(
            crate::core::AppError::InvalidInput("git_restore requires at least one path".into())
                .payload(),
        );
    }
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    let outcome = svc
        .confirm()
        .request_and_wait(
            GitOpKind::Checkout,
            "撤销工作区改动".into(),
            json!({ "paths": paths.clone() }),
        )
        .await
        .map_err(|error| error.payload())?;
    if outcome == ConfirmOutcome::Approved {
        svc.backend()
            .restore(&repo, &paths)
            .await
            .map_err(|error| error.payload())?;
        state_emit.inner()(StateChange::Git);
    }
    Ok(())
}

/// 从暂存区提交（直接提交，不弹确认窗——Commit 可回滚、无破坏性）。
#[tauri::command]
async fn git_commit(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    message: String,
) -> TauriResult<()> {
    if message.trim().is_empty() {
        return Err(
            crate::core::AppError::InvalidInput("commit message must not be empty".into()).payload(),
        );
    }
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    svc.backend()
        .commit(&repo, &message)
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(())
}

/// 重置到目标（默认 HEAD）；`--hard/--keep` 属高危写：默认关闭，需先开启
/// `git.dangerous_writes` 开关，且仍走确认服务（展示将丢失改动清单）。
#[tauri::command]
async fn git_reset(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    mode: String,
    target: Option<String>,
) -> TauriResult<GitResetPreview> {
    let reset_mode = GitResetMode::parse(&mode).map_err(|error| error.payload())?;
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    if (reset_mode == GitResetMode::Hard || reset_mode == GitResetMode::Keep)
        && !svc.dangerous_writes()
    {
        return Err(crate::core::AppError::InvalidInput(
            "git reset --hard/--keep 会丢弃工作区改动，属危险写操作且默认关闭；请先开启「危险写操作」开关或改用 --soft/--mixed".into(),
        )
        .payload());
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
                target.as_deref().unwrap_or("HEAD"),
                reset_mode.as_str()
            ),
            detail,
        )
        .await
        .map_err(|error| error.payload())?;
    if outcome == ConfirmOutcome::Approved {
        let preview = svc
            .backend()
            .reset(&repo, reset_mode, target.as_deref())
            .await
            .map_err(|error| error.payload())?;
        state_emit.inner()(StateChange::Git);
        Ok(preview)
    } else {
        Ok(GitResetPreview::default())
    }
}

/// 切换分支/提交；若工作区有未提交改动将被覆盖 → 高危写场景，默认关闭开关 + 确认。
#[tauri::command]
async fn git_checkout(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    target: String,
) -> TauriResult<()> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    let dirty = match svc.backend().status(&repo).await {
        Ok(s) => !s.unstaged.is_empty() || !s.untracked.is_empty(),
        Err(_) => false,
    };
    if dirty {
        if !svc.dangerous_writes() {
            return Err(crate::core::AppError::InvalidInput(
                "checkout 将覆盖未提交改动，属危险写操作且默认关闭；请先提交/暂存改动或开启「危险写操作」开关".into(),
            )
            .payload());
        }
        let outcome = svc
            .confirm()
            .request_and_wait(
                GitOpKind::Checkout,
                format!("切换到 {target}（将覆盖未提交改动）"),
                json!({ "target": target.clone() }),
            )
            .await
            .map_err(|error| error.payload())?;
        if outcome == ConfirmOutcome::Approved {
            svc.backend()
                .checkout(&repo, &target)
                .await
                .map_err(|error| error.payload())?;
            state_emit.inner()(StateChange::Git);
        }
    } else {
        svc.backend()
            .checkout(&repo, &target)
            .await
            .map_err(|error| error.payload())?;
        state_emit.inner()(StateChange::Git);
    }
    Ok(())
}

/// stash 操作：push/apply 直接执行；pop/drop 需确认。
#[tauri::command]
async fn git_stash(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    action: String,
    message: Option<String>,
) -> TauriResult<()> {
    let stash_action = GitStashAction::parse(&action).map_err(|error| error.payload())?;
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    match stash_action {
        GitStashAction::Push | GitStashAction::Apply => {
            svc.backend()
                .stash(&repo, stash_action, message.as_deref())
                .await
                .map_err(|error| error.payload())?;
            state_emit.inner()(StateChange::Git);
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
                .map_err(|error| error.payload())?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .stash(&repo, stash_action, None)
                    .await
                    .map_err(|error| error.payload())?;
                state_emit.inner()(StateChange::Git);
            }
        }
        GitStashAction::Drop => {
            let outcome = svc
                .confirm()
                .request_and_wait(GitOpKind::StashDrop, "丢弃最新 stash".into(), json!({}))
                .await
                .map_err(|error| error.payload())?;
            if outcome == ConfirmOutcome::Approved {
                svc.backend()
                    .stash(&repo, stash_action, None)
                    .await
                    .map_err(|error| error.payload())?;
                state_emit.inner()(StateChange::Git);
            }
        }
    }
    Ok(())
}

/// 推送到远程分支（需确认）。
#[tauri::command]
async fn git_push(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    remote: Option<String>,
    branch: Option<String>,
) -> TauriResult<()> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    let detail = match svc.backend().status(&repo).await {
        Ok(s) => json!({ "branch": s.branch, "ahead": s.ahead }),
        Err(_) => json!({}),
    };
    let outcome = svc
        .confirm()
        .request_and_wait(GitOpKind::Push, "推送到远程分支".into(), detail)
        .await
        .map_err(|error| error.payload())?;
    if outcome == ConfirmOutcome::Approved {
        svc.backend()
            .push(&repo, remote.as_deref(), branch.as_deref())
            .await
            .map_err(|error| error.payload())?;
        state_emit.inner()(StateChange::Git);
    }
    Ok(())
}

/// 拉取并合并远程改动（需确认）。
#[tauri::command]
async fn git_pull(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
) -> TauriResult<()> {
    let svc = gateway.inner().git_service();
    let repo = svc.active_repo().await.map_err(|error| error.payload())?;
    let outcome = svc
        .confirm()
        .request_and_wait(GitOpKind::Pull, "拉取并合并远程改动".into(), json!({}))
        .await
        .map_err(|error| error.payload())?;
    if outcome == ConfirmOutcome::Approved {
        svc.backend()
            .pull(&repo)
            .await
            .map_err(|error| error.payload())?;
        state_emit.inner()(StateChange::Git);
    }
    Ok(())
}

/// 冲突解决：ours / theirs / both。
#[tauri::command]
async fn git_resolve_conflict(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    repo_id: Option<String>,
    path: String,
    take: String,
) -> TauriResult<()> {
    let take = ConflictTake::parse(&take).map_err(|error| error.payload())?;
    let svc = gateway.inner().git_service();
    let repo = match repo_id {
        Some(id) => svc.repo_by_id(&id).map_err(|error| error.payload())?,
        None => svc.active_repo().await.map_err(|error| error.payload())?,
    };
    svc.backend()
        .resolve_conflict(&repo, &path, take)
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(())
}

/// 确认服务唯一入口：向等待中的写操作投递用户决定。
#[tauri::command]
fn git_confirm(gateway: State<'_, Gateway>, op_id: String, approved: bool) -> TauriResult<()> {
    gateway
        .inner()
        .git_service()
        .confirm()
        .resolve(&op_id, approved)
        .map_err(|error| error.payload())
}

/// 危险写开关回显（前端弹窗/设置读取）。
#[tauri::command]
fn git_get_confirm_config(gateway: State<'_, Gateway>) -> TauriResult<serde_json::Value> {
    Ok(json!({
        "dangerous_writes": gateway.inner().git_service().dangerous_writes(),
    }))
}

/// 持久化并热更新危险写开关（config.json `git` 节）。
#[tauri::command]
async fn git_set_dangerous_writes(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    enabled: bool,
) -> TauriResult<serde_json::Value> {
    gateway
        .inner()
        .set_git_dangerous_writes(enabled)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Git);
    Ok(json!({ "dangerous_writes": enabled }))
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
async fn fs_semantic_search(
    gateway: State<'_, Gateway>,
    query: String,
    top_k: Option<usize>,
    path: Option<String>,
) -> TauriResult<SemanticSearchResult> {
    let store = gateway.inner().workspace_store();
    let ws = require_active_workspace(&store)?;
    let index_root = gateway.inner().search_index_root();
    Retriever::search(&index_root, &ws, &query, top_k, path.as_deref())
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

fn with_hook_judgement_store<T>(
    hook_judgement_store: &Arc<StdMutex<HookJudgementStore>>,
    action: impl FnOnce(&HookJudgementStore) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let store = hook_judgement_store.lock().map_err(|_| {
        crate::core::AppError::RuntimeError("HookJudgementStore lock failed".into()).payload()
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

            // panic hook：把任何 panic（含 tokio task / RPC 分支）落盘到 pulsar.log。
            // hyper 对 handler panic 的默认行为是不发响应直接关闭连接（客户端表现为
            // "Empty reply"），本地复现不到 stderr 时靠这份记录定位。
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                tracing::error!(target: "panic", "panic: {info}");
                default_hook(info);
            }));

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
            // 覆盖链：env(PULSAR_HOST/PORT/TOKEN) > config.json `server` 节 > 内置默认（GUI 无 CLI 层）。
            let (env_host, env_port, env_token) = server_env_overrides();
            let server_cfg = ConfigStore::new(storage_root.clone())
                .read()
                .ok()
                .and_then(|config| config.server)
                .filter(|section| section.enabled.unwrap_or(false))
                .map(|section| ServerConfig {
                    host: env_host
                        .clone()
                        .or_else(|| section.host.clone())
                        .unwrap_or_else(|| DEFAULT_SERVER_HOST.into()),
                    port: env_port
                        .or_else(|| section.port)
                        .unwrap_or(DEFAULT_SERVER_PORT),
                    tokens: env_token
                        .clone()
                        .map(|t| vec![t])
                        .or(section.tokens)
                        .unwrap_or_default(),
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

            // 终端事件 hub：桌面 IPC 事件 + WS 广播双路发布（headless 用 new_headless）。
            let terminal_hub = terminal::TerminalEventHub::new(handle.clone());
            // 服务器公共运行时：Gateway + 分域服务统一初始化（GUI 与 headless 复用）。
            let runtime =
                server_runtime::build_server_runtime(&storage_root, state_emit.clone(), terminal_hub.clone())
                    .map_err(|error| error.to_string())?;
            let neuron_manager = runtime.neuron_manager.clone();

            app.manage(runtime.neuron_manager.clone());
            app.manage(runtime.topic_store.clone());
            app.manage(runtime.hook_judgement_store.clone());
            app.manage(runtime.assistant.clone());
            app.manage(runtime.poller.clone());
            app.manage(runtime.sessions.clone());
            app.manage(runtime.providers.clone());
            app.manage(runtime.conversation_store.clone());
            // 终端事件 hub：IPC 命令与 WS 公共通道（net/ws.rs）共享的会话事件广播器。
            app.manage(runtime.terminal_hub.clone());
            // 终端浏览器支持：随内嵌 server 的 `/ws` 端点启动（net::NetState 注入
            // terminal manager 与 hub，见下方远程模式分支）；不再独立监听端口。
            let ws_manager = Arc::clone(&runtime.terminal_manager);
            app.manage(runtime.terminal_manager);
            let gateway_for_server = runtime.gateway.clone();
            app.manage(runtime.gateway);
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
                    terminal_hub: runtime.terminal_hub.clone(),
                    host: cfg.host.clone(),
                    port: cfg.port,
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
            server_info,
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
            list_conversation_summaries,
            history,
            history_page,
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
            // Hook Judgements
            hook_judgements_list,
            hook_defs_list,
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
            reset_system_prompts,
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
            fs_semantic_search,
            fs_info,
            get_home_dir,
            fs_suggest_abs,
            // Terminal
            terminal_spawn,
            terminal_write,
            terminal_resize,
            terminal_kill,
            terminal_list,
            // Git
            git_repos,
            git_status,
            git_diff,
            git_log,
            git_show_files,
            git_show_diff,
            git_branches,
            git_blame,
            git_stash_list,
            git_set_active_repo,
            git_add,
            git_unstage,
            git_restore,
            git_commit,
            git_reset,
            git_checkout,
            git_stash,
            git_push,
            git_pull,
            git_resolve_conflict,
            git_confirm,
            git_get_confirm_config,
            git_set_dangerous_writes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
