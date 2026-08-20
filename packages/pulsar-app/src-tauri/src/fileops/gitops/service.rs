//! `GitService`：git 能力组合服务——backend + 确认服务 + 当前操作 repo 内存态
//! + `dangerous_writes` 开关。Tauri commands、AI 工具、RPC 共用同一实例。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use super::confirm::GitConfirmService;
use super::repo::CliGitBackend;
use super::{GitBackend, GitRepo};
use crate::core::error::{AppError, AppResult};
use crate::core::events::StateEmitter;
use crate::fileops::workspace::WorkspaceStore;

pub struct GitService {
    backend: Arc<dyn GitBackend>,
    confirm: Arc<GitConfirmService>,
    store: Arc<WorkspaceStore>,
    /// 前端经 `git_set_active_repo` 指定的当前操作仓库 id。
    active_repo_id: RwLock<Option<String>>,
    /// 危险写开关（reset hard / clean / checkout 丢弃改动），默认关。
    dangerous_writes: AtomicBool,
    /// 最近一次 discover 的结果缓存（`git_repos` 结果带版本，前端按需刷新）。
    repos_cache: RwLock<Option<Vec<GitRepo>>>,
}

impl std::fmt::Debug for GitService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitService")
            .field("active_repo_id", &self.active_repo_id())
            .field("dangerous_writes", &self.dangerous_writes())
            .finish()
    }
}

impl GitService {
    pub fn new(
        backend: Arc<dyn GitBackend>,
        store: Arc<WorkspaceStore>,
        emit: Option<StateEmitter>,
        dangerous_writes: bool,
    ) -> Self {
        Self {
            backend,
            confirm: Arc::new(GitConfirmService::new(emit)),
            store,
            active_repo_id: RwLock::new(None),
            dangerous_writes: AtomicBool::new(dangerous_writes),
            repos_cache: RwLock::new(None),
        }
    }

    pub fn backend(&self) -> Arc<dyn GitBackend> {
        Arc::clone(&self.backend)
    }

    pub fn confirm(&self) -> Arc<GitConfirmService> {
        Arc::clone(&self.confirm)
    }

    pub fn dangerous_writes(&self) -> bool {
        self.dangerous_writes.load(Ordering::SeqCst)
    }

    pub fn set_dangerous_writes(&self, enabled: bool) {
        self.dangerous_writes.store(enabled, Ordering::SeqCst);
    }

    pub fn set_active_repo(&self, repo_id: Option<String>) -> AppResult<()> {
        if let Some(id) = &repo_id {
            // 校验 id 存在于已发现 repos（防御未知 id）。
            let known = self.repos_cache.read().map_err(lock_err)?;
            if let Some(repos) = known.as_ref() {
                if !repos.iter().any(|r| &r.id == id) {
                    return Err(AppError::InvalidInput(format!(
                        "unknown repo id: {id}"
                    )));
                }
            }
        }
        let mut guard = self.active_repo_id.write().map_err(lock_err)?;
        *guard = repo_id;
        Ok(())
    }

    pub fn active_repo_id(&self) -> Option<String> {
        self.active_repo_id.read().ok().and_then(|g| g.clone())
    }

    /// 发现 active workspace 内全部 repo（仅向内扫描），结果缓存。
    pub async fn discover_repos(&self) -> AppResult<Vec<GitRepo>> {
        let ws = self
            .store
            .active()?
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "no active workspace; add one via the Files view before using git".into(),
                )
            })?;
        let repos = self
            .backend
            .discover_repos(&ws.root, &ws.ignore)
            .await?;
        {
            let mut guard = self.repos_cache.write().map_err(lock_err)?;
            *guard = Some(repos.clone());
        }
        Ok(repos)
    }

    /// 当前操作 repo：active_repo_id 命中优先；未设置时回落第一个 repo。
    pub async fn active_repo(&self) -> AppResult<GitRepo> {
        // 保证缓存已填充（首次调用/工作区变化后自动 discover）。
        if self.repos_cache.read().map_err(lock_err)?.is_none() {
            self.discover_repos().await?;
        }
        let guard = self.repos_cache.read().map_err(lock_err)?;
        let repos = guard.as_ref().ok_or_else(|| {
            AppError::InvalidInput("no git repo found in the active workspace".into())
        })?;
        let desired = self.active_repo_id.read().map_err(lock_err)?;
        if let Some(id) = desired.as_ref() {
            if let Some(repo) = repos.iter().find(|r| &r.id == id) {
                return Ok(repo.clone());
            }
        }
        // 回落：默认第一个（并同步 active_repo_id，保持 UI/工具一致）。
        if let Some(repo) = repos.first() {
            drop(desired);
            let mut guard = self.active_repo_id.write().map_err(lock_err)?;
            *guard = Some(repo.id.clone());
            return Ok(repo.clone());
        }
        Err(AppError::InvalidInput(
            "no git repo found in the active workspace".into(),
        ))
    }

    /// 直接解析 repo id（不触发 discover）：供命令层校验目标 id 后落 active。
    pub fn repo_by_id(&self, repo_id: &str) -> AppResult<GitRepo> {
        let guard = self.repos_cache.read().map_err(lock_err)?;
        let repos = guard.as_ref().ok_or_else(|| {
            AppError::InvalidInput("git repos not discovered yet; call git_repos first".into())
        })?;
        repos
            .iter()
            .find(|r| r.id == repo_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidInput(format!("unknown repo id: {repo_id}")))
    }

    /// 清空缓存（工作区增删/ignore 变化后由命令层调用）。
    pub fn invalidate_repos(&self) {
        if let Ok(mut guard) = self.repos_cache.write() {
            *guard = None;
        }
    }
}

fn lock_err<T>(e: std::sync::PoisonError<T>) -> AppError {
    AppError::RuntimeError(format!("git service lock: {e}"))
}

// 辅助：统一构造（命令层/AI 工具装配用）。
pub fn build_git_service(
    store: Arc<WorkspaceStore>,
    emit: Option<StateEmitter>,
    dangerous_writes: bool,
) -> Arc<GitService> {
    Arc::new(GitService::new(
        Arc::new(CliGitBackend::new()),
        store,
        emit,
        dangerous_writes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_writes_toggles() {
        let store = Arc::new(
            WorkspaceStore::new(&std::env::temp_dir()).expect("store"),
        );
        let svc = GitService::new(
            Arc::new(CliGitBackend::new()),
            store,
            None,
            false,
        );
        assert!(!svc.dangerous_writes());
        svc.set_dangerous_writes(true);
        assert!(svc.dangerous_writes());
    }

    #[test]
    fn build_helper_constructs() {
        let store = Arc::new(
            WorkspaceStore::new(&std::env::temp_dir()).expect("store"),
        );
        let svc = build_git_service(store, None, false);
        let backend = svc.backend();
        assert!(Arc::as_ptr(&backend) as *const () as usize != 0);
        assert!(!svc.dangerous_writes());
    }
}
