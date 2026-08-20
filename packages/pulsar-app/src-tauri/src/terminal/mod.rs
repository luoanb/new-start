//! 终端面板后端：PTY 会话 + Tauri IPC（独立于 `core/`，对标 VS Code 集成终端）。
//!
//! 模块职责：
//! - `session`：单个 PTY 会话封装（spawn / write / resize / kill / 读循环）。
//! - `manager`：会话注册表（session_id ↔ TerminalSession）。
//! - `commands`：Tauri invoke 命令 + 高频输出事件（`app://terminal-output` / `app://terminal-exit`）。
//!
//! Agent 可见执行（execute_command 接入）由 `core/cmd_exec` 通过
//! `TerminalManager` 的会话句柄旁路广播，本模块不感知 core 内部细节。

pub mod bridge;
pub mod commands;
pub mod events;
pub mod manager;
pub mod session;
pub mod ws;

pub use bridge::AgentTerminalBridge;
pub use events::TerminalEventHub;
pub use manager::TerminalManager;
pub use session::{SessionInfo, TerminalSession, TERMINAL_EXIT_EVENT, TERMINAL_OUTPUT_EVENT};
