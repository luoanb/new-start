pub mod core;
pub mod tui;

use crate::core::{
    conversation_store::ConversationStore, error::AppErrorPayload, ChatOptions, ChatResponse,
    Conversation, ConversationMode, Gateway, Message, ModelCallRequest, ModelCallResponse,
    ModelInfo, ProviderInfo, RuntimeStatus, SkillInfo,
};
use std::path::PathBuf;
use tauri::State;
use tokio::sync::Mutex;

type TauriResult<T> = Result<T, AppErrorPayload>;

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

#[tauri::command]
async fn send_chat_message(
    state: State<'_, Mutex<Gateway>>,
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
) -> TauriResult<ChatResponse> {
    let mut gateway = state
        .lock()
        .await;

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
fn create_conversation(
    state: State<'_, Mutex<Gateway>>,
    mode: String,
) -> TauriResult<String> {
    let conv_mode = match mode.to_lowercase().as_str() {
        "agent" => ConversationMode::Agent,
        "assistant" => ConversationMode::Assistant,
        _ => ConversationMode::Chat,
    };

    let mut gateway = state
        .blocking_lock();

    gateway
        .create_new_conversation(conv_mode)
        .map_err(|error| error.payload())
}

#[tauri::command]
fn close_session(
    state: State<'_, Mutex<Gateway>>,
    session_id: String,
) -> TauriResult<String> {
    let gateway = state
        .blocking_lock();

    gateway
        .session_tracker()
        .close(&session_id)
        .map_err(|error| error.payload())
}

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
    let gateway = state
        .lock()
        .await;
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

fn with_gateway<T>(
    state: State<'_, Mutex<Gateway>>,
    action: impl FnOnce(&mut Gateway) -> crate::core::AppResult<T>,
) -> TauriResult<T> {
    let mut gateway = state
        .blocking_lock();

    action(&mut gateway).map_err(|error| error.payload())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Resolve storage root: prefer Cargo manifest dir (src-tauri/) parent
    // so that .agent-app/ is found at packages/agent-app/.agent-app/
    let storage_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join(".agent-app");
    let store = ConversationStore::new(&storage_root)
        .expect("failed to initialize conversation store");
    let gateway = Gateway::new(store).expect("failed to initialize agent app gateway");

    tauri::Builder::default()
        .manage(Mutex::new(gateway))
        .plugin(tauri_plugin_opener::init())
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
            status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
