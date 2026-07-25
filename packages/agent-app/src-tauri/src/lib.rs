pub mod core;

use crate::core::{
    error::AppErrorPayload, AppError, ChatResponse, Conversation, Gateway, Message, RuntimeStatus,
    SkillInfo,
};
use std::sync::Mutex;
use tauri::State;

type TauriResult<T> = Result<T, AppErrorPayload>;

#[tauri::command]
fn send_message(
    state: State<'_, Mutex<Gateway>>,
    message: String,
    conversation_id: Option<String>,
) -> TauriResult<ChatResponse> {
    with_gateway(state, |gateway| {
        gateway.send_message(message, conversation_id)
    })
}

#[tauri::command]
fn list_skills(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<SkillInfo>> {
    with_gateway(state, |gateway| Ok(gateway.list_skills()))
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
        .lock()
        .map_err(|_| AppError::RuntimeError("Gateway state lock failed".into()).payload())?;

    action(&mut gateway).map_err(|error| error.payload())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let gateway = Gateway::default().expect("failed to initialize agent app gateway");

    tauri::Builder::default()
        .manage(Mutex::new(gateway))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            send_message,
            list_skills,
            list_conversations,
            history,
            clear_conversation,
            status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
