pub mod core;
pub mod tui;

use crate::core::{
    app_log::{self, LogEntry},
    conversation_store::ConversationStore,
    error::AppErrorPayload,
    ChatOptions, ChatResponse, Connection, Conversation, ConversationMode, Gateway, Message,
    ModelCallRequest, ModelCallResponse, ModelInfo, Neuron, NeuronSubgraph, NeuronUpdate,
    PollerStatus, ProviderInfo, RuntimeStatus, SkillInfo, Topic, TopicStatus, TopicUpdate,
};
use core::topic_store::TopicStore;
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

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
    state: State<'_, Mutex<Gateway>>,
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
) -> TauriResult<ChatResponse> {
    let mut gateway = state.lock().await;
    gateway
        .send_model_message(
            message,
            ChatOptions {
                provider_id,
                model_id,
                conversation_id,
            },
        )
        .await
        .map_err(|error| error.payload())
}

#[tauri::command]
fn create_conversation(state: State<'_, Mutex<Gateway>>, mode: String) -> TauriResult<String> {
    let conv_mode = match mode.to_lowercase().as_str() {
        "agent" => ConversationMode::Agent,
        "assistant" => ConversationMode::Assistant,
        _ => ConversationMode::Chat,
    };
    let mut gateway = state.blocking_lock();
    gateway
        .create_new_conversation(conv_mode)
        .map_err(|error| error.payload())
}

#[tauri::command]
fn close_session(
    state: State<'_, Mutex<Gateway>>,
    session_id: String,
) -> TauriResult<String> {
    let gateway = state.blocking_lock();
    gateway
        .session_tracker()
        .close(&session_id)
        .map_err(|error| error.payload())
}

// ── Info ──

#[tauri::command]
fn list_skills(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<SkillInfo>> {
    with_gateway(state, |gateway| Ok(gateway.list_skills()))
}

#[tauri::command]
fn list_providers(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<ProviderInfo>> {
    with_gateway(state, |gateway| Ok(gateway.list_providers()))
}

#[tauri::command]
fn list_models(
    state: State<'_, Mutex<Gateway>>,
    provider_id: Option<String>,
) -> TauriResult<Vec<ModelInfo>> {
    with_gateway(state, |gateway| gateway.list_models(provider_id))
}

#[tauri::command]
async fn call_model(
    state: State<'_, Mutex<Gateway>>,
    request: ModelCallRequest,
) -> TauriResult<ModelCallResponse> {
    let gateway = state.lock().await;
    gateway
        .call_model(request)
        .await
        .map_err(|error| error.payload())
}

#[tauri::command]
fn list_conversations(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<Conversation>> {
    with_gateway(state, |gateway| gateway.list_conversations())
}

#[tauri::command]
fn history(
    state: State<'_, Mutex<Gateway>>,
    conversation_id: Option<String>,
) -> TauriResult<Vec<Message>> {
    with_gateway(state, |gateway| gateway.history(conversation_id))
}

#[tauri::command]
fn clear_conversation(
    state: State<'_, Mutex<Gateway>>,
    conversation_id: Option<String>,
) -> TauriResult<String> {
    with_gateway(state, |gateway| gateway.clear_conversation(conversation_id))
}

#[tauri::command]
fn status(state: State<'_, Mutex<Gateway>>) -> TauriResult<RuntimeStatus> {
    with_gateway(state, |gateway| gateway.status())
}

// ── Topic ──

#[tauri::command]
fn list_topics(
    state: State<'_, Mutex<Gateway>>,
    status: Option<String>,
) -> TauriResult<Vec<Topic>> {
    with_topic_store(state, |store| {
        let filter = status
            .as_deref()
            .and_then(|s| match s {
                "todo" => Some(TopicStatus::Todo),
                "in_progress" => Some(TopicStatus::InProgress),
                "paused" => Some(TopicStatus::Paused),
                "done" => Some(TopicStatus::Done),
                "cancelled" => Some(TopicStatus::Cancelled),
                _ => None,
            });
        store.list(filter)
    })
}

#[tauri::command]
fn get_topic(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
        store
            .get(&id)?
            .ok_or_else(|| {
                crate::core::AppError::ConversationNotFound(format!("Topic not found: {id}"))
            })
    })
}

#[tauri::command]
fn create_topic(
    state: State<'_, Mutex<Gateway>>,
    name: String,
    description: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
        store.create(&name, &description, TopicStatus::Todo, vec![], None)
    })
}

#[tauri::command]
fn update_topic(
    state: State<'_, Mutex<Gateway>>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
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
fn delete_topic(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<bool> {
    with_topic_store(state, |store| store.delete(&id))
}

#[tauri::command]
fn add_topic_scope_item(
    state: State<'_, Mutex<Gateway>>,
    topic_id: String,
    goal: String,
    done_contract: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
        store.add_scope_item(&topic_id, &goal, &done_contract)
    })
}

#[tauri::command]
fn delete_topic_scope_item(
    state: State<'_, Mutex<Gateway>>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
        store.delete_scope_item(&topic_id, &item_id)
    })
}

#[tauri::command]
fn complete_topic_scope_item(
    state: State<'_, Mutex<Gateway>>,
    topic_id: String,
    item_id: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| {
        store.complete_scope_item(&topic_id, &item_id)
    })
}

#[tauri::command]
fn pause_topic(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| store.pause(&id))
}

#[tauri::command]
fn resume_topic(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Topic> {
    with_topic_store(state, |store| store.resume(&id))
}

// ── Poller ──

#[tauri::command]
fn poll_status(state: State<'_, Mutex<Gateway>>) -> TauriResult<PollerStatus> {
    with_gateway(state, |gateway| gateway.poll_status())
}

#[tauri::command]
fn poll_pause(state: State<'_, Mutex<Gateway>>) -> TauriResult<()> {
    with_gateway(state, |gateway| gateway.poll_pause())
}

#[tauri::command]
fn poll_resume(state: State<'_, Mutex<Gateway>>) -> TauriResult<()> {
    with_gateway(state, |gateway| gateway.poll_resume())
}

#[tauri::command]
fn poll_trigger(state: State<'_, Mutex<Gateway>>) -> TauriResult<()> {
    with_gateway(state, |gateway| gateway.poll_trigger())
}

// ── Neuron ──

#[tauri::command]
fn list_neurons(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<Neuron>> {
    with_neuron_manager(state, |mgr| mgr.list_neurons())
}

#[tauri::command]
fn get_neuron(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Neuron> {
    with_neuron_manager(state, |mgr| {
        mgr.get_neuron(&id)?
            .ok_or_else(|| {
                crate::core::AppError::NeuronNotFound(id)
            })
    })
}

#[tauri::command]
fn update_neuron(
    state: State<'_, Mutex<Gateway>>,
    id: String,
    desc: Option<String>,
    content: Option<String>,
) -> TauriResult<Neuron> {
    with_neuron_manager(state, |mgr| {
        mgr.update_content_for_admin(&id, NeuronUpdate { desc, content })
    })
}

#[tauri::command]
fn get_connections(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Vec<Connection>> {
    with_neuron_manager(state, |mgr| mgr.get_connections(&id))
}

#[tauri::command]
fn get_network(
    state: State<'_, Mutex<Gateway>>,
    id: String,
    max_depth: Option<usize>,
) -> TauriResult<NeuronSubgraph> {
    with_neuron_manager(state, |mgr| {
        mgr.get_network(&id, max_depth.unwrap_or(2))
    })
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

fn with_gateway<T>(
    state: State<'_, Mutex<Gateway>>,
    action: impl FnOnce(&mut Gateway) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let mut gateway = state.blocking_lock();
    action(&mut gateway).map_err(|error| error.payload())
}

fn with_topic_store<T>(
    state: State<'_, Mutex<Gateway>>,
    action: impl FnOnce(&TopicStore) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let gateway = state.blocking_lock();
    let topic_store_arc = gateway
        .topic_store()
        .map_err(|error| error.payload())?;
    let store = topic_store_arc
        .lock()
        .map_err(|_| {
            crate::core::AppError::RuntimeError("TopicStore lock failed".into()).payload()
        })?;
    action(&store).map_err(|error| error.payload())
}

fn with_neuron_manager<T>(
    state: State<'_, Mutex<Gateway>>,
    action: impl FnOnce(&crate::core::neuron_manager::NeuronManager) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let gateway = state.blocking_lock();
    let mgr = gateway.neuron_manager();
    action(&mgr).map_err(|error| error.payload())
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

            let store = ConversationStore::new(&storage_root)
                .map_err(|error| error.to_string())?;
            let gateway =
                Gateway::new(store).map_err(|error| error.to_string())?;
            app.manage(Mutex::new(gateway));

            let boot_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = boot_handle.state::<Mutex<Gateway>>();
                let gateway = state.lock().await;
                tracing::info!(phase = "bootstrap_neurons", "starting neuron bootstrap");
                match gateway.bootstrap_neurons().await {
                    Ok(()) => {
                        tracing::info!(phase = "bootstrap_neurons", "neuron bootstrap complete")
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
