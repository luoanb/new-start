use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;

use super::{
    error::{AppError, AppResult},
    tool_registry::{Tool, ToolRegistry},
};

/// Information about a currently running session.
#[derive(Debug, Clone, Serialize)]
pub struct RunningSession {
    pub session_id: String,
    pub started_at: u128,
    pub current_step: Option<String>,
}

struct SessionCtx {
    info: RunningSession,
    /// 归属令牌：`register` 每次生成新令牌，`unregister` 仅在令牌与当前条目一致
    /// （`Arc::ptr_eq`）时移除——旧轮收尾的过期句柄不会误删新轮的注册条目
    /// （发送消息中断：旧轮被抢占收敛返回时，新轮已重新注册）。
    token: Arc<()>,
}

/// 注册归属句柄：由 `register` 返回，调用方收尾时传回 `unregister` 做归属校验。
#[derive(Clone)]
pub struct SessionHandle {
    token: Arc<()>,
}

/// Pure in-memory session tracker.
///
/// Manages the lifecycle of active conversation executions.
/// Register at start, unregister on completion, close for forced cancellation.
#[derive(Clone)]
pub struct SessionTracker {
    inner: Arc<Mutex<HashMap<String, SessionCtx>>>,
    on_change: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl std::fmt::Debug for SessionTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionTracker").finish_non_exhaustive()
    }
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            on_change: Arc::new(Mutex::new(None)),
        }
    }

    /// Set a callback invoked whenever the set of running sessions changes
    /// (register / unregister / update_step / close).
    pub fn set_on_change(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut slot) = self.on_change.lock() {
            *slot = Some(callback);
        }
    }

    fn notify(&self) {
        if let Ok(slot) = self.on_change.lock() {
            if let Some(cb) = slot.as_ref() {
                cb();
            }
        }
    }

    /// Register a session as running.
    ///
    /// 返回归属句柄：`unregister` 仅接受当前最新注册的句柄（见 `SessionCtx::token`）。
    /// tracker 为纯运行状态展示（list / get / update_step），不承载取消行为——
    /// 停止语义统一走 `Gateway::stop_session`（协调器 cancel_active + 暂停课题）。
    pub fn register(&self, session_id: &str) -> AppResult<SessionHandle> {
        let token = Arc::new(());
        {
            let mut map = self
                .inner
                .lock()
                .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
            map.insert(
                session_id.to_string(),
                SessionCtx {
                    info: RunningSession {
                        session_id: session_id.to_string(),
                        started_at: now_ms(),
                        current_step: None,
                    },
                    token: Arc::clone(&token),
                },
            );
        }
        self.notify();
        Ok(SessionHandle { token })
    }

    /// Remove a session from the tracker (normal completion).
    ///
    /// 归属校验：仅当 `handle` 仍对应最新注册时才移除；过期句柄（该会话已被新轮
    /// 重新注册）为无害 no-op，防止旧轮收尾误删新轮条目。
    pub fn unregister(&self, session_id: &str, handle: &SessionHandle) {
        let removed = self.inner.lock().is_ok_and(|mut map| {
            let is_owner = matches!(
                map.get(session_id),
                Some(ctx) if Arc::ptr_eq(&ctx.token, &handle.token)
            );
            if is_owner {
                map.remove(session_id);
                true
            } else {
                false
            }
        });
        if removed {
            self.notify();
        }
    }

    /// Update the current execution step for an agent session.
    pub fn update_step(&self, session_id: &str, step: &str) -> AppResult<()> {
        let changed = {
            let mut map = self
                .inner
                .lock()
                .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
            if let Some(ctx) = map.get_mut(session_id) {
                ctx.info.current_step = Some(step.to_string());
                true
            } else {
                false
            }
        };
        if changed {
            self.notify();
            Ok(())
        } else {
            Err(AppError::ConversationNotFound(format!(
                "Running session not found: {session_id}"
            )))
        }
    }

    /// Remove a running entry without ownership check（强制摘除）。
    ///
    /// 生产路径唯一调用方是 `Gateway::stop_session`（停止 = 摘除展示条目）。
    /// 条目不存在时返回 `ConversationNotFound`——调用方按需静默（停止幂等）。
    pub fn close(&self, session_id: &str) -> AppResult<String> {
        {
            let mut map = self
                .inner
                .lock()
                .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
            if map.remove(session_id).is_none() {
                return Err(AppError::ConversationNotFound(format!(
                    "Running session not found: {session_id}"
                )));
            }
        }
        self.notify();
        Ok(format!("Closed session: {session_id}"))
    }

    /// List all currently running sessions.
    pub fn list(&self) -> AppResult<Vec<RunningSession>> {
        let map = self
            .inner
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        Ok(map.values().map(|ctx| ctx.info.clone()).collect())
    }

    /// Get a single running session by id.
    pub fn get(&self, session_id: &str) -> AppResult<Option<RunningSession>> {
        let map = self
            .inner
            .lock()
            .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
        Ok(map.get(session_id).map(|ctx| ctx.info.clone()))
    }
}

impl Default for SessionTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Session tools are not pre-registered (await self-describing inserts).
pub fn register_session_tracker_tools(_registry: &mut ToolRegistry, _tracker: SessionTracker) {}

// ── GetRunningSessionsTool (unregistered until inserts exist) ──

#[allow(dead_code)]
struct GetRunningSessionsTool {
    tracker: SessionTracker,
}

#[allow(dead_code)]
impl GetRunningSessionsTool {
    fn new(tracker: SessionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for GetRunningSessionsTool {
    fn name(&self) -> &str {
        "get_running_sessions"
    }
    fn description(&self) -> &str {
        "List all currently running agent sessions"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> AppResult<String> {
        let sessions = self.tracker.list()?;
        if sessions.is_empty() {
            return Ok("No running sessions.".into());
        }
        let mut lines = vec!["Running sessions:".to_string()];
        for s in &sessions {
            let step = s.current_step.as_deref().unwrap_or("awaiting response");
            lines.push(format!(
                "  {} | step: {}",
                &s.session_id[..s.session_id.len().min(16)],
                step
            ));
        }
        Ok(lines.join("\n"))
    }
}

// ── CloseSessionTool ───────────────────────────────────────────

#[allow(dead_code)]
struct CloseSessionTool {
    tracker: SessionTracker,
}

#[allow(dead_code)]
impl CloseSessionTool {
    fn new(tracker: SessionTracker) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for CloseSessionTool {
    fn name(&self) -> &str {
        "close_session"
    }
    fn description(&self) -> &str {
        "Close a running session by ID"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "ID of the running session to close"
                }
            },
            "required": ["session_id"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let session_id = args
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("Missing: session_id".into()))?;
        self.tracker.close(session_id)
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_list() {
        let st = SessionTracker::new();
        st.register("sess-1").unwrap();
        st.register("sess-2").unwrap();
        let list = st.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_unregister_removes_session() {
        let st = SessionTracker::new();
        let handle = st.register("sess-1").unwrap();
        st.unregister("sess-1", &handle);
        assert!(st.list().unwrap().is_empty());
    }

    /// 归属校验回归（发送消息中断）：旧轮过期句柄的 unregister 不得删除新轮注册条目。
    #[test]
    fn test_unregister_stale_handle_keeps_newer_registration() {
        let st = SessionTracker::new();
        let old = st.register("sess-1").unwrap();
        let new = st.register("sess-1").unwrap(); // 新轮覆盖注册（发送消息中断）

        st.unregister("sess-1", &old); // 旧轮被抢占后收尾
        assert!(
            st.get("sess-1").unwrap().is_some(),
            "stale unregister must not remove the newer registration"
        );

        st.unregister("sess-1", &new); // 新轮正常收尾
        assert!(st.get("sess-1").unwrap().is_none());
    }

    #[test]
    fn test_update_step() {
        let st = SessionTracker::new();
        st.register("sess-1").unwrap();
        st.update_step("sess-1", "calculate").unwrap();
        let s = st.get("sess-1").unwrap().unwrap();
        assert_eq!(s.current_step.as_deref(), Some("calculate"));
    }

    #[test]
    fn test_close_removes_entry() {
        let st = SessionTracker::new();
        st.register("sess-1").unwrap();
        st.close("sess-1").unwrap();
        assert!(st.list().unwrap().is_empty());
    }

    #[test]
    fn test_close_nonexistent_returns_error() {
        let st = SessionTracker::new();
        let result = st.close("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_update_step_nonexistent_returns_error() {
        let st = SessionTracker::new();
        let result = st.update_step("nonexistent", "step");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent() {
        let st = SessionTracker::new();
        assert!(st.get("nonexistent").unwrap().is_none());
    }
}
