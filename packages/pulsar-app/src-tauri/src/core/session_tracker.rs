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
    abort: Option<Box<dyn FnOnce() + Send>>,
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
    /// `abort` is an optional callback invoked when `close()` is called,
    /// allowing the caller to cancel the in-flight execution.
    pub fn register(
        &self,
        session_id: &str,
        abort: Option<Box<dyn FnOnce() + Send>>,
    ) -> AppResult<()> {
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
                    abort,
                },
            );
        }
        self.notify();
        Ok(())
    }

    /// Remove a session from the tracker (normal completion).
    /// Does NOT invoke the abort callback.
    pub fn unregister(&self, session_id: &str) {
        {
            if let Ok(mut map) = self.inner.lock() {
                let _ = map.remove(session_id);
            }
        }
        self.notify();
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

    /// Force-close a running session: invokes the abort callback and removes it.
    pub fn close(&self, session_id: &str) -> AppResult<String> {
        let abort = {
            let mut map = self
                .inner
                .lock()
                .map_err(|e| AppError::StorageError(format!("Lock error: {}", e)))?;
            map.remove(session_id).map(|ctx| ctx.abort)
        };
        if let Some(abort) = abort {
            // Release lock before calling the callback
            if let Some(f) = abort {
                f();
            }
            self.notify();
            Ok(format!("Closed session: {session_id}"))
        } else {
            Err(AppError::ConversationNotFound(format!(
                "Running session not found: {session_id}"
            )))
        }
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
        st.register("sess-1", None).unwrap();
        st.register("sess-2", None).unwrap();
        let list = st.list().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_unregister_removes_session() {
        let st = SessionTracker::new();
        st.register("sess-1", None).unwrap();
        st.unregister("sess-1");
        assert!(st.list().unwrap().is_empty());
    }

    #[test]
    fn test_update_step() {
        let st = SessionTracker::new();
        st.register("sess-1", None).unwrap();
        st.update_step("sess-1", "calculate").unwrap();
        let s = st.get("sess-1").unwrap().unwrap();
        assert_eq!(s.current_step.as_deref(), Some("calculate"));
    }

    #[test]
    fn test_close_invokes_abort() {
        let st = SessionTracker::new();
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        let abort = Box::new(move || {
            called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        st.register("sess-1", Some(abort)).unwrap();
        st.close("sess-1").unwrap();
        assert!(called.load(std::sync::atomic::Ordering::Relaxed));
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
