//! 终端会话管理器：session_id ↔ TerminalSession 注册表。
//!
//! 供 tauri command 层（spawn/write/resize/kill/list）与 agent 工具桥接层共用，
//! 以后者为目标保留能力（agent 可见执行需要拿到会话句柄旁路广播输出）。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::session::{SessionInfo, TerminalSession};

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
}
