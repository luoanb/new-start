pub mod core;
pub mod infra;
pub mod sinks;
pub mod stores;

use crate::core::log_phase::{
    PHASE_APP_SETUP, PHASE_APP_WINDOW, PHASE_LOGS_CLEAR, PHASE_LOGS_LIST, PHASE_LOGS_SET_LEVEL,
};
use crate::core::{AppError, AppErrorPayload, StateChange, StateEmitter, STATE_CHANGED_EVENT};
use crate::infra::config::ConfigStore;
use crate::sinks::app_log::{self, LogEntry};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

type TauriResult<T> = Result<T, AppErrorPayload>;

/// `logs_list` 的返回：内存缓冲条目 + 当前级别 + 日志目录三合一。
#[derive(serde::Serialize)]
struct LogListResult {
    entries: Vec<LogEntry>,
    level: String,
    dir: Option<String>,
}

// ── Debug ──

#[tauri::command]
fn debug_storage_path(config_store: State<'_, ConfigStore>) -> String {
    // 直接读运行期数据根（数据目录选择的唯一事实源），不重复推导路径。
    format!(
        "cwd={:?} storage={:?}",
        std::env::current_dir(),
        config_store.root()
    )
}

// ── Logs ──

#[tauri::command]
fn logs_list() -> LogListResult {
    let entries = app_log::snapshot();
    tracing::debug!(
        phase = PHASE_LOGS_LIST,
        count = entries.len(),
        "listed buffered log entries"
    );
    LogListResult {
        entries,
        level: app_log::get_level(),
        dir: app_log::log_dir().map(|path| path.display().to_string()),
    }
}

#[tauri::command]
fn logs_phases() -> &'static [crate::core::log_phase::PhaseInfo] {
    app_log::phases()
}

#[tauri::command]
fn logs_clear() {
    app_log::clear_buffer();
    tracing::info!(phase = PHASE_LOGS_CLEAR, "log buffer cleared");
}

#[tauri::command]
fn logs_set_level(level: String) -> TauriResult<String> {
    let applied = app_log::set_level(&level)
        .map_err(|message| AppError::InvalidInput(message).payload())?;
    tracing::info!(phase = PHASE_LOGS_SET_LEVEL, level = %applied, "log level updated");
    Ok(applied)
}

// ── App Entry ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            tracing::info!(phase = PHASE_APP_SETUP, "pulsar-harness starting");

            // 数据目录契约（见 docs/pulsar-harness/storage.md）：开发态用项目目录
            // （数据可检查），发布态用系统应用数据目录——发布包写项目目录会落到
            // 构建机路径而失败。
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            let storage_root = stores::storage::StorageBase::for_gui(app_data_dir)
                .resolve()
                .map_err(|error| error.to_string())?;

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
                "pulsar-harness logging initialized"
            );

            // panic hook：把任何 panic（含 tokio task）落盘到 pulsar-harness.log，
            // 只在 stderr 不可见时靠这份记录定位。
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

            // 统一状态事件发射器：command 层写操作完成后广播，前端 dataStore 监听
            // STATE_CHANGED_EVENT 并按 kind 重新拉取。
            let state_emit_handle = handle.clone();
            let state_emit: StateEmitter = Arc::new(move |change: StateChange| {
                let _ = state_emit_handle.emit(STATE_CHANGED_EVENT, change);
            });

            // 配置读写入口：数据根由 setup 解析后注入，供命令层与后续业务模块共享。
            app.manage(ConfigStore::new(storage_root.clone()));
            app.manage(Arc::clone(&state_emit));

            // 手动创建主窗口（tauri.conf.json 中 `"create": false`）并给所有资源响应加
            // `Cache-Control: no-store`：Tauri 的 tauri:// 协议默认不设缓存头，WebKitGTK
            // 会把旧版 index.html/CSS/JS 写入跨重启持久化的磁盘缓存，重启后按 URL 复用
            // 旧资源导致样式错乱。no-store 使 webview 每次都取当前 bundle。
            let window_config = app.config().app.windows[0].clone();
            tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)
                .map_err(|error| error.to_string())?
                .on_web_resource_request(|_request, response| {
                    response.headers_mut().insert(
                        tauri::http::header::CACHE_CONTROL,
                        tauri::http::HeaderValue::from_static("no-store"),
                    );
                })
                .build()
                .map_err(|error| error.to_string())?;
            tracing::info!(phase = PHASE_APP_WINDOW, "main window created");

            // 基建就绪信号：前端停止 loading 遮罩。
            state_emit(StateChange::AppReady);
            tracing::info!(phase = PHASE_APP_SETUP, "pulsar-harness ready");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            debug_storage_path,
            logs_list,
            logs_phases,
            logs_clear,
            logs_set_level,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
