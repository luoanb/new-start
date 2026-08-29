//! 会话级串行协调器（B 方案）：同一会话同一时刻仅一个轮次在跑。
//!
//! 语义（用户优先）：
//! - **User 轮**到达：抢占——cancel 当前正在跑的轮次（若存在），已产出落库后自然终止，
//!   随即开始新一轮；同时向新轮交付「旧轮收敛等待句柄」（watch 通道），新轮据此等待
//!   旧轮完成「回复多少存储多少」收敛落库后再取消息快照（Bug #2 修复）；
//! - **非 User 轮**（Poller / ManualStep / AgentLoop）到达：遇忙直接跳过（不抢占，避免
//!   轮询与手动推进打断正在进行的对话）。
//!
//! 取消采用协作式 `CancellationToken`：被抢占的轮次在其注入点（模型流式 / 工具执行）
//! 通过 `tokio::select!` 感知取消并执行「回复多少存储多少」的收敛写。
//!
//! 「仅中断」（停止按钮）桥接（Bug #1 修复）：[`SessionCoordinator::cancel_active`]
//! 供 `SessionTracker` 注册的 abort 回调调用——`close_session` 时外部取消活动轮次，
//! 取消语义与 User 抢占完全一致。
//!
//! busy 必然释放：`begin` 返回 RAII guard [`ActiveRound`]，`Drop` 自动 `end()`——
//! 任何早退路径（`?` / `return` / 取消分支）都不会遗留 busy 状态；同时 `Drop` 兜底
//! 发送收敛信号，保证等待方不会因旧轮异常早退而悬挂。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::conversation_runner::RoundTriggerKind;

/// 活动轮次条目：取消令牌 + 收敛信号发送端。
///
/// 收敛信号约定：轮次结束（收敛落库完成 / guard Drop / sender 释放）时置 `true`，
/// 抢占方（新 User 轮）据此解除等待并重读消息快照。
struct ActiveEntry {
    token: Arc<CancellationToken>,
    converged: Arc<watch::Sender<bool>>,
}

/// 会话协调器：`session_id → 当前活动轮次的取消令牌`。
#[derive(Default)]
pub struct SessionCoordinator {
    active: Mutex<HashMap<String, ActiveEntry>>,
}

impl std::fmt::Debug for SessionCoordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self
            .active
            .lock()
            .map(|map| map.len())
            .unwrap_or(usize::MAX);
        f.debug_struct("SessionCoordinator")
            .field("active_sessions", &count)
            .finish()
    }
}

impl SessionCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一轮的开始，返回 RAII guard（`Drop` 自动结束本轮）。
    /// - `Some(ActiveRound)`：本轮可执行（User 触发时抢占并 cancel 已有轮次，
    ///   guard 同时携带「被抢占轮的收敛等待句柄」）；
    /// - `None`：非 User 触发且该会话已有活动轮次（跳过本轮）。
    pub fn begin(
        self: &Arc<Self>,
        session_id: &str,
        trigger: RoundTriggerKind,
    ) -> Option<ActiveRound> {
        let mut map = self.active.lock().expect("coordinator lock should not be poisoned");
        let mut preempt_wait = None;
        if let Some(existing) = map.get(session_id) {
            if trigger == RoundTriggerKind::User {
                // 用户优先：立即终止当前轮（协作式取消，已产出由该轮自行收敛落库），
                // 并把旧轮的收敛信号接收端交付给新轮（等待收敛后再取快照，Bug #2）。
                tracing::info!(
                    phase = "session_coordinator",
                    session_id,
                    "user round preempts active round"
                );
                existing.token.cancel();
                preempt_wait = Some(existing.converged.subscribe());
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
        let (converged_tx, _) = watch::channel(false);
        let converged = Arc::new(converged_tx);
        map.insert(
            session_id.to_string(),
            ActiveEntry {
                token: Arc::clone(&token),
                converged: Arc::clone(&converged),
            },
        );
        Some(ActiveRound {
            coordinator: Arc::clone(self),
            session_id: session_id.to_string(),
            token,
            converged,
            preempt_wait,
        })
    }

    /// 结束一轮：仅当活动表中仍是本轮令牌时移除（防止旧轮收尾时误删已抢占的新轮注册）。
    /// 移除时兜底发送收敛信号（轮次结束即视为已收敛，解除抢占方等待）。
    pub fn end(&self, session_id: &str, token: &Arc<CancellationToken>) {
        let entry = {
            let mut map = self.active.lock().expect("coordinator lock should not be poisoned");
            match map.get(session_id) {
                Some(existing) if Arc::ptr_eq(&existing.token, token) => map.remove(session_id),
                _ => None,
            }
        };
        if let Some(entry) = entry {
            let _ = entry.converged.send(true);
        }
    }

    /// 外部中断（Bug #1 桥接）：取消该会话当前活动轮次（停止按钮 → Gateway::stop_session）。
    /// 无活动轮次时为无害 no-op（返回 false）——与 User 抢占共用同一协作式取消语义。
    pub fn cancel_active(&self, session_id: &str) -> bool {
        let map = self.active.lock().expect("coordinator lock should not be poisoned");
        match map.get(session_id) {
            Some(entry) => {
                entry.token.cancel();
                true
            }
            None => false,
        }
    }
}

/// RAII guard：持有一轮的注册信息，`Drop` 时自动 `end()`，保证 busy 不泄漏；
/// 同时兜底发送收敛信号（收敛写在取消分支内先于 return 完成，时序正确）。
#[derive(Debug)]
pub struct ActiveRound {
    coordinator: Arc<SessionCoordinator>,
    session_id: String,
    token: Arc<CancellationToken>,
    converged: Arc<watch::Sender<bool>>,
    /// 本轮抢占旧轮时交付的旧轮收敛等待句柄（仅 User 抢占时非空）。
    preempt_wait: Option<watch::Receiver<bool>>,
}

impl ActiveRound {
    /// 同步查询：本轮是否已被抢占取消。
    pub fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// 取出「被本抢占终止的旧轮」的收敛等待句柄（仅可取一次）。
    pub fn take_preempt_wait(&mut self) -> Option<watch::Receiver<bool>> {
        self.preempt_wait.take()
    }

    /// 显式发送收敛信号（轮次结束/收敛完成）。`Drop` 兜底亦会发送，重复发送无害。
    #[allow(dead_code)]
    pub fn notify_converged(&self) {
        let _ = self.converged.send(true);
    }

    /// 取消令牌：供 `tokio::select!` 取消分支监听。
    pub fn token(&self) -> &Arc<CancellationToken> {
        &self.token
    }
}

impl Drop for ActiveRound {
    fn drop(&mut self) {
        // 兜底：无论本轮从哪条路径结束（正常 / 取消收敛 / 错误早退），都解除抢占方等待。
        // 取消分支的收敛写在 return 之前完成，Drop 信号时序正确（先落库后放行）。
        let _ = self.converged.send(true);
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

    /// Bug #1 回归：外部取消入口（停止按钮 → abort 回调）必须能终止活动轮次；
    /// 无活动轮次时为无害 no-op。
    #[test]
    fn cancel_active_cancels_running_round() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let round = coordinator.begin("s1", user()).unwrap();
        assert!(coordinator.cancel_active("s1"), "active round must be cancellable");
        assert!(round.cancelled(), "stop path must cancel the active round token");
        drop(round);
        assert!(
            !coordinator.cancel_active("s1"),
            "no active round → no-op (must not panic or resurrect)"
        );
    }

    /// Bug #2 回归：User 抢占交付收敛等待句柄；旧轮 guard Drop（RAII 收尾）必须解除
    /// 新轮的收敛等待（先落库后放行的时序由 Drop 兜底保证）。
    #[tokio::test]
    async fn preempt_wait_is_signaled_when_preempted_round_ends() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let first = coordinator.begin("s1", user()).unwrap();
        let mut second = coordinator.begin("s1", user()).unwrap();
        let mut wait = second
            .take_preempt_wait()
            .expect("user preemption must yield the convergence wait handle");
        assert!(second.take_preempt_wait().is_none(), "wait handle is take-once");
        assert!(first.cancelled());
        drop(first);
        let signaled = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if *wait.borrow_and_update() {
                    return true;
                }
                if wait.changed().await.is_err() {
                    return true; // sender 全部释放 = 轮次已结束
                }
            }
        })
        .await
        .expect("guard drop must signal convergence");
        assert!(signaled);
    }
}
