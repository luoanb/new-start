//! Agent 可见执行桥接（方案 A）：core 层 `execute_command` 通过本类型创建
//! 一次性 PTY 会话，并把输出实时广播给前端。
//!
//! 复用与用户手动打开终端完全相同的 `app://terminal-output` / `app://terminal-exit`
//! 事件（载荷一致），前端 Terminal 面板零区分即可渲染 Agent 的执行过程。

use std::sync::Arc;

use super::events::TerminalEventHub;
use super::manager::TerminalManager;

/// 持有 manager + 事件 hub：core 层不直接依赖 tauri，只依赖本桥接类型。
///
/// 输出经 hub 双路广播（桌面 IPC 事件 + WS 网关订阅者），与用户手动终端一致。
pub struct AgentTerminalBridge {
    manager: Arc<TerminalManager>,
    hub: TerminalEventHub,
}

/// `TerminalEventHub` 不实现 `Debug`，手动占位实现（与 `core/gateway::EmitterSlot` 同模式），
/// 使 `Gateway` 可继续 `#[derive(Debug)]`。
impl std::fmt::Debug for AgentTerminalBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentTerminalBridge")
            .field("sessions", &self.manager.len())
            .finish_non_exhaustive()
    }
}

impl AgentTerminalBridge {
    pub fn new(manager: Arc<TerminalManager>, hub: TerminalEventHub) -> Self {
        Self { manager, hub }
    }

    /// 会话注册表句柄（core 层创建 / 登记会话用）。
    pub fn manager(&self) -> &Arc<TerminalManager> {
        &self.manager
    }

    /// 广播输出块到指定会话（前端 Terminal 面板实时渲染）。
    pub fn emit_output(&self, session_id: &str, data: Vec<u8>) {
        self.hub.publish_output(session_id, data);
    }

    /// 广播退出事件（前端标记 tab 为已退出）。
    pub fn emit_exit(&self, session_id: &str, exit_code: i32) {
        self.hub.publish_exit(session_id, exit_code);
    }
}
