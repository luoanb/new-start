//! Tauri IPC 命令：终端面板的后端入口。
//!
//! - invoke：`terminal_spawn` / `terminal_write` / `terminal_resize` / `terminal_kill` / `terminal_list`
//! - event：`app://terminal-output`（高频输出，独立于 state-changed）、`app://terminal-exit`

use std::sync::Arc;

use tauri::State;

use super::events::{pump_session_events, TerminalEventHub};
use super::manager::TerminalManager;
use super::session::{SessionInfo, TerminalSession};
use crate::core::error::AppErrorPayload;
use crate::core::AppError;

type TauriResult<T> = Result<T, AppErrorPayload>;

/// `terminal_spawn` 返回值。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSpawned {
    pub session_id: String,
}

/// 输出事件载荷。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputPayload {
    pub session_id: String,
    /// 输出字节块（可为 UTF-8 文本，含 ANSI 转义序列；前端直接喂 xterm）。
    pub data: Vec<u8>,
}

/// 退出事件载荷。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitPayload {
    pub session_id: String,
    pub exit_code: i32,
}

fn session_not_found(session_id: &str) -> AppErrorPayload {
    AppError::InvalidInput(format!("terminal session not found: {session_id}")).payload()
}

#[tauri::command]
pub async fn terminal_spawn(
    manager: State<'_, Arc<TerminalManager>>,
    hub: State<'_, TerminalEventHub>,
    cwd: Option<String>,
    shell: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> TauriResult<TerminalSpawned> {
    let (session, output_rx, exit_rx) =
        TerminalSession::spawn(cwd, shell, cols, rows).map_err(|e| e.payload())?;
    let session_id = session.session_id().to_string();
    manager.insert(Arc::clone(&session));

    // 事件泵：输出/退出经 hub 双路广播（桌面 IPC 事件 + WS 网关订阅者）。
    pump_session_events(hub.inner().clone(), session_id.clone(), output_rx, exit_rx);

    tracing::info!(session_id = %session_id, "terminal session spawned");
    Ok(TerminalSpawned { session_id })
}

#[tauri::command]
pub async fn terminal_write(
    manager: State<'_, Arc<TerminalManager>>,
    session_id: String,
    data: String,
) -> TauriResult<()> {
    let session = manager
        .get(&session_id)
        .ok_or_else(|| session_not_found(&session_id))?;
    session.write(data.as_bytes()).map_err(|e| e.payload())
}

#[tauri::command]
pub async fn terminal_resize(
    manager: State<'_, Arc<TerminalManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> TauriResult<()> {
    let session = manager
        .get(&session_id)
        .ok_or_else(|| session_not_found(&session_id))?;
    session.resize(cols, rows).map_err(|e| e.payload())
}

#[tauri::command]
pub async fn terminal_kill(
    manager: State<'_, Arc<TerminalManager>>,
    session_id: String,
) -> TauriResult<()> {
    let session = manager
        .get(&session_id)
        .ok_or_else(|| session_not_found(&session_id))?;
    session.kill().map_err(|e| e.payload())
}

#[tauri::command]
pub fn terminal_list(manager: State<'_, Arc<TerminalManager>>) -> TauriResult<Vec<SessionInfo>> {
    Ok(manager.list())
}
