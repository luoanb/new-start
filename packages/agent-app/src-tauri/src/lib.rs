pub mod core;
pub mod tui;

use crate::core::{
    app_log::{self, LogEntry},
    conversation_store::ConversationStore,
    error::AppErrorPayload,
    neuron_manager::NeuronManager,
    poller::Poller,
    providers::ProviderRegistry,
    session_tracker::{RunningSession, SessionTracker},
    topic_store::TopicStore,
    ChatOptions, ChatResponse, Connection, Conversation, ConversationMode, Gateway, Message,
    ModelCallRequest, ModelCallResponse, ModelInfo, Neuron, NeuronCreate, NeuronSubgraph,
    NeuronUpdate, StateChange, StateEmitter, STATE_CHANGED_EVENT,
    PollerStatus, ProviderInfo, RuntimeStatus, SkillInfo, Topic, TopicStatus, TopicUpdate,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};
use tauri::{Emitter, Manager, State};

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
            .join(".agent-app")
    )
}

// ── Chat ──

#[tauri::command]
async fn send_chat_message(
    gateway: State<'_, Gateway>,
    state_emit: State<'_, StateEmitter>,
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
) -> TauriResult<ChatResponse> {
    // Gateway is shared via Tauri State (Arc); send_model_message is &self and
    // clone-outs before network await — no outer Mutex held across I/O.
    let response = gateway
        .inner()
        .send_model_message(
            message,
            ChatOptions {
                provider_id,
                model_id,
                conversation_id,
            },
        )
        .await
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations);
    Ok(response)
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
        _ => ConversationMode::Chat,
    };
    let conversation_id = gateway
        .inner()
        .create_new_conversation(conv_mode)
        .map_err(|error| error.payload())?;
    state_emit.inner()(StateChange::Conversations);
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
    sessions
        .inner()
        .list()
        .map_err(|error| error.payload())
}

// ── Info ──

#[tauri::command]
async fn list_skills(gateway: State<'_, Gateway>) -> TauriResult<Vec<SkillInfo>> {
    Ok(gateway.inner().list_skills())
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
    state_emit.inner()(StateChange::Conversations);
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
    state_emit: State<'_, StateEmitter>,
    name: String,
    description: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| {
        store.create(&name, &description, TopicStatus::Todo, vec![], None)
    })?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn update_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| {
        store.update(
            &id,
            TopicUpdate {
                name,
                description,
                extra: None,
            },
        )
    })?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn delete_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
) -> TauriResult<bool> {
    let deleted = with_topic_store(&topic_store, |store| store.delete(&id))?;
    state_emit.inner()(StateChange::Topics);
    Ok(deleted)
}

#[tauri::command]
async fn add_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    topic_id: String,
    goal: String,
    done_contract: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| {
        store.add_scope_item(&topic_id, &goal, &done_contract)
    })?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn delete_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| {
        store.delete_scope_item(&topic_id, &item_id)
    })?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn complete_topic_scope_item(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| {
        store.complete_scope_item(&topic_id, &item_id)
    })?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn pause_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| store.pause(&id))?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
}

#[tauri::command]
async fn resume_topic(
    topic_store: State<'_, Arc<StdMutex<TopicStore>>>,
    state_emit: State<'_, StateEmitter>,
    id: String,
) -> TauriResult<Topic> {
    let topic = with_topic_store(&topic_store, |store| store.resume(&id))?;
    state_emit.inner()(StateChange::Topics);
    Ok(topic)
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
    app_log::set_level(&level).map_err(|message| {
        crate::core::AppError::InvalidInput(message).payload()
    })
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
    let storage_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join(".agent-app");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
                "agent-app logging initialized"
            );

            // 统一状态事件发射器：command 层写操作与后台推进完成后广播，
            // 前端 dataStore 监听 STATE_CHANGED_EVENT 并按 kind 重新拉取。
            let state_emit_handle = handle.clone();
            let state_emit: StateEmitter = Arc::new(move |change: StateChange| {
                let _ = state_emit_handle.emit(STATE_CHANGED_EVENT, change);
            });

            let store = ConversationStore::new(&storage_root)
                .map_err(|error| error.to_string())?;
            let gateway =
                Gateway::with_state_emitter(store, Some(state_emit.clone()))
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
            app.manage(gateway);
            app.manage(state_emit);

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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            debug_storage_path,
            send_chat_message,
            create_conversation,
            close_session,
            list_running_sessions,
            list_skills,
            list_providers,
            list_models,
            call_model,
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
            // Neuron
            list_neurons,
            get_neuron,
            update_neuron,
            get_connections,
            get_network,
            create_neuron_plain,
            adjust_neuron_weight,
            adjust_edge_weight,
            // Logs
            logs_snapshot,
            logs_get_level,
            logs_set_level,
            logs_clear_buffer,
            logs_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
