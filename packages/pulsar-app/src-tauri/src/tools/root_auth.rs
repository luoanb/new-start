//! root 授权服务：命令因权限不足失败后广播一次授权请求，等待用户决定。
//!
//! 与 `GitConfirmService` 同构（`oneshot` + `StateChange` 事件 + `resolve` + 超时），
//! 差别在于：**无消费方时立即返回 `Unavailable` 而不等待**——没有前端就没有弹窗，
//! 此时保持「拒绝」语义（不静默提权）。
//!
//! 授权只表示「允许以 root 重跑」，root 本身由平台原生提权程序（pkexec / osascript）
//! 提供，密码由系统弹窗收集，不经过本进程。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot;

use crate::core::error::{AppError, AppResult};
use crate::core::events::{StateChange, StateEmitter};

/// 授权等待超时：用户未在窗口期内决定则作废（对齐 git 写确认）。
pub const ROOT_AUTH_TIMEOUT: Duration = Duration::from_secs(60);

/// 事件中回传的失败原因摘要上限（字符）。
const EXCERPT_MAX_CHARS: usize = 480;

/// 用户决定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootAuthOutcome {
    /// 用户授权：调用方应以 root 重跑。
    Approved,
    /// 用户拒绝。
    Rejected,
    /// 无消费方（无前端 / 无状态发射器）：不等待，保持失败。
    Unavailable,
}

struct PendingRequest {
    created_ms: i64,
    tx: oneshot::Sender<bool>,
}

/// 唯一计数：request_id 保证并发请求不冲突。
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_millis() as i64
}

fn truncate_excerpt(text: &str) -> String {
    text.chars().take(EXCERPT_MAX_CHARS).collect()
}

pub struct RootAuthService {
    pending: RwLock<HashMap<String, PendingRequest>>,
    timeout: Duration,
    emit: Option<StateEmitter>,
}

impl RootAuthService {
    pub fn new(emit: Option<StateEmitter>) -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            timeout: ROOT_AUTH_TIMEOUT,
            emit,
        }
    }

    /// 广播授权请求并等待用户响应；无消费方立即返回 `Unavailable`，
    /// 超时 / 通道断裂按「未授权」处理（返回 `Rejected`，调用方维持原失败）。
    pub async fn request_and_wait(
        &self,
        command: &str,
        cwd: Option<&str>,
        stderr_excerpt: &str,
    ) -> AppResult<RootAuthOutcome> {
        let emit = match self.emit.as_ref() {
            Some(emit) => Arc::clone(emit),
            None => return Ok(RootAuthOutcome::Unavailable),
        };

        let request_id = format!(
            "root-auth-{}-{}",
            now_ms(),
            REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let (tx, rx) = oneshot::channel();

        self.prune_expired();
        {
            let mut guard = self
                .pending
                .write()
                .map_err(|e| AppError::RuntimeError(format!("root auth lock: {e}")))?;
            guard.insert(
                request_id.clone(),
                PendingRequest {
                    created_ms: now_ms(),
                    tx,
                },
            );
        }

        emit(StateChange::RootAuth {
            request_id: request_id.clone(),
            command: command.to_string(),
            cwd: cwd.map(str::to_string),
            stderr_excerpt: truncate_excerpt(stderr_excerpt),
        });

        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(true)) => {
                self.remove(&request_id);
                Ok(RootAuthOutcome::Approved)
            }
            // 显式拒绝，以及超时 / 通道断裂（本轮被 stop 抢占时接收端已被丢弃）：
            // 都不得提权，一律按未授权收敛。
            Ok(Ok(false)) | Ok(Err(_)) | Err(_) => {
                self.remove(&request_id);
                Ok(RootAuthOutcome::Rejected)
            }
        }
    }

    /// `root_auth_resolve` 唯一入口：向等待方投递用户决定。
    pub fn resolve(&self, request_id: &str, approved: bool) -> AppResult<()> {
        let tx = {
            let mut guard = self
                .pending
                .write()
                .map_err(|e| AppError::RuntimeError(format!("root auth lock: {e}")))?;
            guard.remove(request_id).map(|req| req.tx)
        };
        match tx {
            Some(tx) => {
                let _ = tx.send(approved);
                Ok(())
            }
            None => Err(AppError::InvalidInput(format!(
                "unknown or expired root authorization request: {request_id}"
            ))),
        }
    }

    fn remove(&self, request_id: &str) {
        if let Ok(mut guard) = self.pending.write() {
            guard.remove(request_id);
        }
    }

    fn prune_expired(&self) {
        let cutoff = now_ms() - self.timeout.as_millis() as i64;
        if let Ok(mut guard) = self.pending.write() {
            guard.retain(|_, req| req.created_ms >= cutoff);
        }
    }

    /// 当前 pending 数量（供测试与诊断）。
    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending.read().map(|g| g.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 无消费方（无 emit）的服务：请求不等待，直接 Unavailable。
    fn svc() -> Arc<RootAuthService> {
        Arc::new(RootAuthService::new(None))
    }

    /// 有消费方的服务（emitter 丢弃事件）：请求会入队等待用户决定。
    fn svc_with_emit() -> Arc<RootAuthService> {
        Arc::new(RootAuthService::new(Some(Arc::new(|_| {}))))
    }

    fn first_id(s: &RootAuthService) -> String {
        s.pending
            .read()
            .unwrap()
            .keys()
            .next()
            .cloned()
            .expect("one pending request")
    }

    #[tokio::test]
    async fn approved_resolves_immediately() {
        let s = svc_with_emit();
        let handle = {
            let s = Arc::clone(&s);
            tokio::spawn(async move { s.request_and_wait("mkdir /opt/x", Some("/tmp"), "denied").await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(s.pending_len(), 1);
        s.resolve(&first_id(&s), true).unwrap();
        assert_eq!(handle.await.unwrap().unwrap(), RootAuthOutcome::Approved);
        assert_eq!(s.pending_len(), 0, "resolve 后必须出队");
    }

    #[tokio::test]
    async fn rejected_returns_rejected() {
        let s = svc_with_emit();
        let handle = {
            let s = Arc::clone(&s);
            tokio::spawn(async move { s.request_and_wait("mkdir /opt/x", None, "").await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        s.resolve(&first_id(&s), false).unwrap();
        assert_eq!(handle.await.unwrap().unwrap(), RootAuthOutcome::Rejected);
    }

    #[tokio::test]
    async fn no_consumer_returns_unavailable_without_waiting() {
        let s = svc();
        // emit 为 None：必须立即返回，不进入 60s 等待。
        let outcome = s.request_and_wait("mkdir /opt/x", None, "").await.unwrap();
        assert_eq!(outcome, RootAuthOutcome::Unavailable);
        assert_eq!(s.pending_len(), 0, "无消费方不应入队");
    }

    #[tokio::test]
    async fn timeout_is_treated_as_rejected() {
        let s = Arc::new(RootAuthService {
            pending: RwLock::new(HashMap::new()),
            timeout: Duration::from_millis(100),
            emit: Some(Arc::new(|_| {})),
        });
        let outcome = s.request_and_wait("mkdir /opt/x", None, "").await.unwrap();
        assert_eq!(outcome, RootAuthOutcome::Rejected);
        assert_eq!(s.pending_len(), 0);
    }

    #[tokio::test]
    async fn emitted_event_carries_command_and_truncated_excerpt() {
        let seen: Arc<RwLock<Vec<StateChange>>> = Arc::new(RwLock::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let s = Arc::new(RootAuthService::new(Some(Arc::new(move |change| {
            sink.write().unwrap().push(change);
        }))));
        let long = "x".repeat(EXCERPT_MAX_CHARS + 100);
        let handle = {
            let s = Arc::clone(&s);
            let long = long.clone();
            tokio::spawn(async move { s.request_and_wait("mkdir /opt/x", Some("/w"), &long).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        s.resolve(&first_id(&s), true).unwrap();
        let _ = handle.await.unwrap();

        let guard = seen.read().unwrap();
        match &guard[0] {
            StateChange::RootAuth {
                command,
                cwd,
                stderr_excerpt,
                request_id,
            } => {
                assert_eq!(command, "mkdir /opt/x");
                assert_eq!(cwd.as_deref(), Some("/w"));
                assert_eq!(stderr_excerpt.chars().count(), EXCERPT_MAX_CHARS);
                assert!(request_id.starts_with("root-auth-"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn resolve_unknown_request_errors() {
        let s = RootAuthService::new(None);
        assert!(s.resolve("nope", true).is_err());
    }

    #[tokio::test]
    async fn concurrent_requests_get_distinct_ids() {
        let s = svc_with_emit();
        let mut handles = Vec::new();
        for _ in 0..2 {
            let s = Arc::clone(&s);
            handles.push(tokio::spawn(async move {
                s.request_and_wait("cmd", None, "").await
            }));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        let ids: Vec<String> = s.pending.read().unwrap().keys().cloned().collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "并发请求必须拿到不同 request_id");
        for id in &ids {
            s.resolve(id, false).unwrap();
        }
        for handle in handles {
            assert_eq!(handle.await.unwrap().unwrap(), RootAuthOutcome::Rejected);
        }
        assert_eq!(s.pending_len(), 0);
    }
}
