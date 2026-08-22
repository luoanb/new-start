//! 会话级串行协调器（B 方案）：同一会话同一时刻仅一个轮次在跑。
//!
//! 语义（用户优先）：
//! - **User 轮**到达：抢占——cancel 当前正在跑的轮次（若存在），已产出落库后自然终止，
//!   随即开始新一轮；
//! - **非 User 轮**（Poller / ManualStep / AgentLoop）到达：遇忙直接跳过（不抢占，避免
//!   轮询与手动推进打断正在进行的对话）。
//!
//! 取消采用协作式 `CancellationToken`：被抢占的轮次在其注入点（模型流式 / 工具执行）
//! 通过 `tokio::select!` 感知取消并执行「回复多少存储多少」的收敛写。
//!
//! busy 必然释放：`begin` 返回 RAII guard [`ActiveRound`]，`Drop` 自动 `end()`——
//! 任何早退路径（`?` / `return` / 取消分支）都不会遗留 busy 状态。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::conversation_runner::RoundTriggerKind;

/// 会话协调器：`session_id → 当前活动轮次的取消令牌`。
#[derive(Debug, Default)]
pub struct SessionCoordinator {
    active: Mutex<HashMap<String, Arc<CancellationToken>>>,
}

impl SessionCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一轮的开始，返回 RAII guard（`Drop` 自动结束本轮）。
    /// - `Some(ActiveRound)`：本轮可执行（User 触发时抢占并 cancel 已有轮次）；
    /// - `None`：非 User 触发且该会话已有活动轮次（跳过本轮）。
    pub fn begin(
        self: &Arc<Self>,
        session_id: &str,
        trigger: RoundTriggerKind,
    ) -> Option<ActiveRound> {
        let mut map = self.active.lock().expect("coordinator lock should not be poisoned");
        if let Some(existing) = map.get(session_id) {
            if trigger == RoundTriggerKind::User {
                // 用户优先：立即终止当前轮（协作式取消，已产出由该轮自行收敛落库）。
                tracing::info!(
                    phase = "session_coordinator",
                    session_id,
                    "user round preempts active round"
                );
                existing.cancel();
            } else {
                tracing::info!(
                    phase = "session_coordinator",
                    session_id,
                    trigger = ?trigger,
                    "round skipped: session busy"
                );
                return None;
            }
        }
        let token = Arc::new(CancellationToken::new());
        map.insert(session_id.to_string(), Arc::clone(&token));
        Some(ActiveRound {
            coordinator: Arc::clone(self),
            session_id: session_id.to_string(),
            token,
        })
    }

    /// 结束一轮：仅当活动表中仍是本轮令牌时移除（防止旧轮收尾时误删已抢占的新轮注册）。
    pub fn end(&self, session_id: &str, token: &Arc<CancellationToken>) {
        let mut map = self.active.lock().expect("coordinator lock should not be poisoned");
        if let Some(existing) = map.get(session_id) {
            if Arc::ptr_eq(existing, token) {
                map.remove(session_id);
            }
        }
    }
}

/// RAII guard：持有一轮的注册信息，`Drop` 时自动 `end()`，保证 busy 不泄漏。
#[derive(Debug)]
pub struct ActiveRound {
    coordinator: Arc<SessionCoordinator>,
    session_id: String,
    token: Arc<CancellationToken>,
}

impl ActiveRound {
    /// 同步查询：本轮是否已被抢占取消。
    pub fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// 取消令牌：供 `tokio::select!` 取消分支监听。
    pub fn token(&self) -> &Arc<CancellationToken> {
        &self.token
    }
}

impl Drop for ActiveRound {
    fn drop(&mut self) {
        self.coordinator.end(&self.session_id, &self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> RoundTriggerKind {
        RoundTriggerKind::User
    }

    fn poller() -> RoundTriggerKind {
        RoundTriggerKind::Poller
    }

    #[test]
    fn user_preempts_active_round() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let first = coordinator.begin("s1", user()).expect("first user round runs");
        let second = coordinator.begin("s1", user()).expect("user round always preempts");
        assert!(first.cancelled(), "preempted round should be cancelled");
        assert!(!second.cancelled());
        drop(second);
    }

    #[test]
    fn non_user_round_skipped_when_busy() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let _first = coordinator.begin("s1", user()).unwrap();
        assert!(
            coordinator.begin("s1", poller()).is_none(),
            "non-user round must skip while busy"
        );
    }

    #[test]
    fn end_only_clears_own_token() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let first = coordinator.begin("s1", user()).unwrap();
        // 用户抢占后旧令牌遗留（旧轮收尾时不应误删新轮）。
        let second = coordinator.begin("s1", user()).unwrap();
        coordinator.end("s1", first.token());
        assert!(coordinator.begin("s1", poller()).is_none(), "second still busy");
        drop(second);
        assert!(coordinator.begin("s1", poller()).is_some(), "busy released after drop");
    }

    #[test]
    fn drop_releases_busy() {
        let coordinator = Arc::new(SessionCoordinator::new());
        {
            let _guard = coordinator.begin("s1", user()).unwrap();
            assert!(coordinator.begin("s1", poller()).is_none(), "busy while guard alive");
        }
        // guard 出作用域 → Drop 自动 end
        assert!(coordinator.begin("s1", poller()).is_some(), "busy released by drop");
    }

    #[test]
    fn different_sessions_are_independent() {
        let coordinator = Arc::new(SessionCoordinator::new());
        coordinator.begin("s1", user()).unwrap();
        assert!(coordinator.begin("s2", poller()).is_some());
    }
}
