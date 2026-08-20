//! 写操作确认服务：任何需用户确认的 git 写操作先入 pending 队列并广播确认请求，
//! 等待 `git_confirm { op_id, approved }`；超时（60s）自动作废。
//!
//! GUI 为当前唯一消费方；接口形状与事件（`StateChange::GitConfirm`）即未来 TUI 的
//! 接入点——TUI 只需消费同一事件并提供 `resolve`，不触碰 backend。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::oneshot;

use crate::core::error::{AppError, AppResult};
use crate::core::events::{StateChange, StateEmitter};

/// 确认超时：pending 操作过期自动作废。
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(60);

/// 写操作种类（确认弹窗标题/详情由请求方给出，kind 用于前端分类展示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOpKind {
    Commit,
    Push,
    Pull,
    Reset,
    Checkout,
    StashApply,
    StashDrop,
    Clean,
}

impl GitOpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::Push => "Push",
            Self::Pull => "Pull",
            Self::Reset => "Reset",
            Self::Checkout => "Checkout",
            Self::StashApply => "StashApply",
            Self::StashDrop => "StashDrop",
            Self::Clean => "Clean",
        }
    }
}

/// 用户确认结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    Approved,
    Rejected,
}

struct PendingGitOp {
    created_ms: i64,
    tx: oneshot::Sender<bool>,
}

/// 唯一计数：op_id 保证并发请求不冲突。
static OP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_millis() as i64
}

pub struct GitConfirmService {
    pending: RwLock<HashMap<String, PendingGitOp>>,
    timeout: Duration,
    emit: Option<StateEmitter>,
}

impl GitConfirmService {
    pub fn new(emit: Option<StateEmitter>) -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            timeout: CONFIRM_TIMEOUT,
            emit,
        }
    }

    /// 广播确认请求并等待用户响应；超时/通道断裂 → 作废返回错误。
    pub async fn request_and_wait(
        &self,
        kind: GitOpKind,
        title: String,
        detail: Value,
    ) -> AppResult<ConfirmOutcome> {
        let op_id = format!(
            "git-op-{}-{}",
            now_ms(),
            OP_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let (tx, rx) = oneshot::channel();

        self.prune_expired();
        {
            let mut guard = self.pending.write().map_err(|e| {
                AppError::RuntimeError(format!("git confirm lock: {e}"))
            })?;
            guard.insert(
                op_id.clone(),
                PendingGitOp {
                    created_ms: now_ms(),
                    tx,
                },
            );
        }

        if let Some(emit) = self.emit.as_ref() {
            emit(StateChange::GitConfirm {
                op_id: op_id.clone(),
                op_kind: kind.as_str().to_string(),
                title,
                detail,
            });
        }

        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(true)) => {
                self.remove(&op_id);
                Ok(ConfirmOutcome::Approved)
            }
            Ok(Ok(false)) => {
                self.remove(&op_id);
                Ok(ConfirmOutcome::Rejected)
            }
            Ok(Err(_)) => {
                self.remove(&op_id);
                Err(AppError::RuntimeError(
                    "git confirmation channel closed unexpectedly".into(),
                ))
            }
            Err(_elapsed) => {
                self.remove(&op_id);
                Err(AppError::RuntimeError(format!(
                    "git operation confirmation timed out after {}s",
                    self.timeout.as_secs()
                )))
            }
        }
    }

    /// `git_confirm` 唯一入口：向等待方投递用户决定。
    pub fn resolve(&self, op_id: &str, approved: bool) -> AppResult<()> {
        let tx = {
            let mut guard = self.pending.write().map_err(|e| {
                AppError::RuntimeError(format!("git confirm lock: {e}"))
            })?;
            guard.remove(op_id).map(|op| op.tx)
        };
        match tx {
            Some(tx) => {
                let _ = tx.send(approved);
                Ok(())
            }
            None => Err(AppError::InvalidInput(format!(
                "unknown or expired git operation: {op_id}"
            ))),
        }
    }

    fn remove(&self, op_id: &str) {
        if let Ok(mut guard) = self.pending.write() {
            guard.remove(op_id);
        }
    }

    fn prune_expired(&self) {
        let cutoff = now_ms() - self.timeout.as_millis() as i64;
        if let Ok(mut guard) = self.pending.write() {
            guard.retain(|_, op| op.created_ms >= cutoff);
        }
    }

    /// 当前 pending 数量（供测试与诊断）。
    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending
            .read()
            .map(|g| g.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn svc() -> GitConfirmService {
        GitConfirmService::new(None)
    }

    #[tokio::test]
    async fn approved_resolves_immediately() {
        let s = svc();
        let s2 = std::sync::Arc::new(s);
        let handle = {
            let s = s2.clone();
            tokio::spawn(async move {
                s.request_and_wait(GitOpKind::Commit, "提交".into(), Value::Null)
                    .await
            })
        };
        // 等 pending 入队后 resolve
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(s2.pending_len(), 1);
        let ids: Vec<String> = s2
            .pending
            .read()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        s2.resolve(&ids[0], true).unwrap();
        assert_eq!(handle.await.unwrap().unwrap(), ConfirmOutcome::Approved);
        assert_eq!(s2.pending_len(), 0);
    }

    #[tokio::test]
    async fn rejected_returns_rejected() {
        let s = Arc::new(svc());
        let handle = {
            let s = s.clone();
            tokio::spawn(async move {
                s.request_and_wait(GitOpKind::Push, "推送".into(), Value::Null)
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        let ids: Vec<String> = s.pending.read().unwrap().keys().cloned().collect();
        s.resolve(&ids[0], false).unwrap();
        assert_eq!(handle.await.unwrap().unwrap(), ConfirmOutcome::Rejected);
    }

    #[tokio::test]
    async fn timeout_expires_pending() {
        let s = Arc::new(GitConfirmService {
            pending: RwLock::new(HashMap::new()),
            timeout: Duration::from_millis(100),
            emit: None,
        });
        let handle = {
            let s = s.clone();
            tokio::spawn(async move {
                s.request_and_wait(GitOpKind::Reset, "重置".into(), Value::Null)
                    .await
            })
        };
        let err = handle.await.unwrap().err().expect("timeout error");
        assert!(err.to_string().contains("timed out"));
        assert_eq!(s.pending_len(), 0);
    }

    #[test]
    fn resolve_unknown_op_errors() {
        let s = svc();
        assert!(s.resolve("nope", true).is_err());
    }
}
