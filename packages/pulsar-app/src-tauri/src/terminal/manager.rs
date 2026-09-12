//! 终端会话管理器：session_id ↔ TerminalSession 注册表。
//!
//! 供 tauri command 层（spawn/write/resize/kill/list）与 agent 工具桥接层共用，
//! 以后者为目标保留能力（agent 可见执行需要拿到会话句柄旁路广播输出）。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::session::{SessionInfo, TerminalSession};
use crate::core::{AppError, AppResult};

pub struct TerminalManager {
    sessions: Mutex<HashMap<String, Arc<TerminalSession>>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<TerminalSession>> {
        self.sessions.lock().get(session_id).cloned()
    }

    pub fn insert(&self, session: Arc<TerminalSession>) {
        self.sessions
            .lock()
            .insert(session.session_id().to_string(), session);
    }

    pub fn remove(&self, session_id: &str) {
        self.sessions.lock().remove(session_id);
    }

    /// 结束会话并从注册表摘除：kill 语义即「会话终结」，故摘除与 kill 绑定，
    /// 避免 IPC `terminal_kill` / WS kill / agent 桥接三处调用方各自记得移除而漏掉其一。
    ///
    /// 会话不存在（含已被摘除后重复调用）时返回 `InvalidInput`。
    pub fn kill_and_remove(&self, session_id: &str) -> AppResult<()> {
        let session = self.get(session_id).ok_or_else(|| {
            AppError::InvalidInput(format!("terminal session not found: {session_id}"))
        })?;
        let result = session.kill();
        // 立即摘除：读线程随后仍会把退出事件广播给前端（事件按 session_id 发送，
        // 不依赖注册表），但不让会话残留在 `terminal_list` 里。
        self.remove(session_id);
        result
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions.lock().values().map(|s| s.info()).collect()
    }

    pub fn len(&self) -> usize {
        self.sessions.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_list_remove_roundtrip() {
        let manager = TerminalManager::new();
        let (session, _output_rx, _exit_rx) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        let id = session.session_id().to_string();
        let _ = session.kill();

        manager.insert(Arc::clone(&session));
        assert_eq!(manager.len(), 1);
        assert!(manager.get(&id).is_some());
        assert!(!manager.is_empty());

        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].session_id, id);

        manager.remove(&id);
        assert!(manager.is_empty());
        assert!(manager.get(&id).is_none());
    }

    /// kill 路径：`kill_and_remove`（IPC `terminal_kill` / WS kill / agent 超时共用）
    /// 结束后会话必须已从注册表摘除，`terminal_list` 不再返回它。
    #[test]
    fn kill_and_remove_reaps_session() {
        let manager = TerminalManager::new();
        let (session, _output_rx, _exit_rx) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        let id = session.session_id().to_string();
        manager.insert(Arc::clone(&session));
        assert_eq!(manager.len(), 1);

        manager.kill_and_remove(&id).unwrap();

        assert!(manager.get(&id).is_none());
        assert!(manager.list().is_empty(), "terminal_list must not return killed session");
        assert!(manager.is_empty());
        // 已摘除的会话再次 kill：报 not found，不会 panic 或重新插入。
        assert!(manager.kill_and_remove(&id).is_err());
        assert!(manager.is_empty());
    }

    /// 多个会话并存时只摘除目标会话，其余不受影响（防止误摘）。
    #[test]
    fn kill_and_remove_only_reaps_target_session() {
        let manager = TerminalManager::new();
        let (keep, _o1, _e1) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        let (drop, _o2, _e2) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        let keep_id = keep.session_id().to_string();
        let drop_id = drop.session_id().to_string();
        manager.insert(Arc::clone(&keep));
        manager.insert(Arc::clone(&drop));
        assert_eq!(manager.len(), 2);

        manager.kill_and_remove(&drop_id).unwrap();

        assert!(manager.get(&drop_id).is_none());
        assert!(manager.get(&keep_id).is_some(), "other sessions must survive");
        assert_eq!(manager.list().len(), 1);

        let _ = keep.kill();
        manager.remove(&keep_id);
    }
}
