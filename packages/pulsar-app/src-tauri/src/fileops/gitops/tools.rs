//! AI 原生 git 工具（native 通道，均有 `inserts/<name>.md` 门禁）。
//!
//! 工具分级治理（与需求文档一致）：
//! - 只读 6 个 `register_core`（任何对话都带上）：git_status / git_diff / git_log /
//!   git_branch / git_blame / git_stash_list
//! - 写 9 个 `register`（Normal）：
//!   - git_add / git_stash(push|apply) / git_resolve_conflict 直接执行
//!   - git_restore / git_commit / git_push / git_pull 经 `GitConfirmService` 确认
//!   - git_reset（--hard/--keep）与 git_checkout（丢弃改动场景）额外受 `dangerous_writes`
//!     开关约束（默认关），开关开仍走确认
//!
//! 工具作用域：当前 active workspace 内、`git_set_active_repo` 指定的 repo；
//! 路径参数经 repo 根前缀校验（拒绝绝对路径与 `..`）。

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::confirm::{ConfirmOutcome, GitOpKind};
use super::service::GitService;
use super::{ConflictTake, GitRepo, GitResetMode, GitStashAction};
use crate::core::error::{AppError, AppResult};
use crate::core::tool_registry::{Tool, ToolRegistry};

/// git 工具共享上下文：组合服务（backend + 确认 + active repo + 开关）。
pub struct GitToolContext {
    pub service: Arc<GitService>,
}

impl GitToolContext {
    pub fn new(service: Arc<GitService>) -> Self {
        Self { service }
    }
}

/// 把全部 git 工具注册进 registry。
pub fn register_git_tools(registry: &mut ToolRegistry, ctx: Arc<GitToolContext>) {
    // 只读 Core
    registry.register_core(GitStatusTool::new(Arc::clone(&ctx)));
    registry.register_core(GitDiffTool::new(Arc::clone(&ctx)));
    registry.register_core(GitLogTool::new(Arc::clone(&ctx)));
    registry.register_core(GitBranchTool::new(Arc::clone(&ctx)));
    registry.register_core(GitBlameTool::new(Arc::clone(&ctx)));
    registry.register_core(GitStashListTool::new(Arc::clone(&ctx)));
    // 写 Normal
    registry.register(GitAddTool::new(Arc::clone(&ctx)));
    registry.register(GitRestoreTool::new(Arc::clone(&ctx)));
    registry.register(GitCommitTool::new(Arc::clone(&ctx)));
    registry.register(GitResetTool::new(Arc::clone(&ctx)));
    registry.register(GitCheckoutTool::new(Arc::clone(&ctx)));
    registry.register(GitStashTool::new(Arc::clone(&ctx)));
    registry.register(GitPushTool::new(Arc::clone(&ctx)));
    registry.register(GitPullTool::new(Arc::clone(&ctx)));
    registry.register(GitResolveConflictTool::new(Arc::clone(&ctx)));
}

// ── 参数提取 / 序列化 helper（与 fs_tools 同风格）──

fn require_str(args: &Value, key: &str) -> AppResult<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::InvalidInput(format!("missing string argument: {key}")))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

fn opt_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

fn opt_str_vec(args: &Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    })
}

fn ok_json<T: serde::Serialize>(value: T) -> AppResult<String> {
    serde_json::to_string(&value)
        .map_err(|e| AppError::RuntimeError(format!("serialize tool result: {e}")))
}

/// 确认通过后执行；拒绝返回 cancelled 标记（非错误，模型可感知用户取消）。
async fn apply_outcome<T>(
    outcome: ConfirmOutcome,
    then: impl Future<Output = AppResult<T>>,
) -> AppResult<String>
where
    T: serde::Serialize,
{
    match outcome {
        ConfirmOutcome::Approved => {
            let v = then.await?;
            ok_json(json!({ "cancelled": false, "result": v }))
        }
        ConfirmOutcome::Rejected => ok_json(json!({ "cancelled": true })),
    }
}

/// 声明一个只读 git 工具（样板由宏生成，业务闭包返回 async 结果）。
/// 闭包接收**自有值**（Arc 上下文 / 已解析的 active repo / 参数），避免借用跨 await。
macro_rules! git_ro_tool {
    ($ty:ident, $id:literal, $desc:literal, $params:tt, $exec:expr) => {
        pub struct $ty {
            ctx: Arc<GitToolContext>,
        }
        impl $ty {
            pub fn new(ctx: Arc<GitToolContext>) -> Self {
                Self { ctx }
            }
        }
        #[async_trait]
        impl Tool for $ty {
            fn name(&self) -> &str {
                $id
            }
            fn description(&self) -> &str {
                $desc
            }
            fn parameters(&self) -> Value {
                json!($params)
            }
            async fn execute(&self, args: Value) -> AppResult<String> {
                let repo = self.ctx.service.active_repo().await?;
                let out = ($exec)(Arc::clone(&self.ctx), repo, args).await?;
                ok_json(out)
            }
        }
    };
}

git_ro_tool!(
    GitStatusTool,
    "git_status",
    "Show the working tree status of the active repository: current branch, staged/unstaged/untracked/conflicted changes, and ahead/behind counts. Read-only. Mirrors `git status`.",
    {
        "type": "object",
        "properties": {},
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, _args: Value| async move {
        ctx.service.backend().status(&repo).await
    }
);

git_ro_tool!(
    GitDiffTool,
    "git_diff",
    "Show the diff of the active repository. By default unstaged (working tree) changes; pass cached=true for staged changes. Optionally restrict to a single path (relative to the repo root). Read-only.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Optional path (relative to repo root) to restrict the diff to"},
            "cached": {"type": "boolean", "description": "true = staged diff (--cached), false/omitted = unstaged diff"}
        },
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, args: Value| async move {
        let path = opt_str(&args, "path");
        let cached = opt_bool(&args, "cached").unwrap_or(false);
        ctx.service.backend().diff(&repo, cached, path.as_deref()).await
    }
);

git_ro_tool!(
    GitLogTool,
    "git_log",
    "Show recent commit history of the active repository (hash, short hash, author, date, subject). Read-only.",
    {
        "type": "object",
        "properties": {
            "limit": {"type": "number", "description": "Max commits to return; default 30, clamped to [1,200]"},
            "offset": {"type": "number", "description": "Commits to skip for pagination (`git log --skip`); default 0"}
        },
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, args: Value| async move {
        let limit = opt_usize(&args, "limit").unwrap_or(30);
        let offset = opt_usize(&args, "offset").unwrap_or(0);
        ctx.service.backend().log(&repo, limit, offset).await
    }
);

git_ro_tool!(
    GitBranchTool,
    "git_branch",
    "List local and remote branches of the active repository, marking the current branch and each branch's upstream. Read-only.",
    {
        "type": "object",
        "properties": {},
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, _args: Value| async move {
        ctx.service.backend().branches(&repo).await
    }
);

git_ro_tool!(
    GitBlameTool,
    "git_blame",
    "Show per-line blame of a file in the active repository (line number, short commit, author, date, text). Path is relative to the repo root. Read-only.",
    {
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "File path relative to the repo root"}
        },
        "required": ["path"],
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, args: Value| async move {
        let path = require_str(&args, "path")?;
        ctx.service.backend().blame(&repo, &path).await
    }
);

git_ro_tool!(
    GitStashListTool,
    "git_stash_list",
    "List the stashed changes of the active repository (index and message). Read-only.",
    {
        "type": "object",
        "properties": {},
        "additionalProperties": false
    },
    |ctx: Arc<GitToolContext>, repo: GitRepo, _args: Value| async move {
        ctx.service.backend().stash_list(&repo).await
    }
);

// ──────────────────────────────────────────────
// 写工具（register Normal）
// ──────────────────────────────────────────────

pub struct GitAddTool {
    ctx: Arc<GitToolContext>,
}

impl GitAddTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitAddTool {
    fn name(&self) -> &str {
        "git_add"
    }
    fn description(&self) -> &str {
        "Stage changes in the active repository. Pass all=true to stage everything, or paths (relative to the repo root) to stage specific paths. Write operation, no confirmation required."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {"type": "array", "items": {"type": "string"}, "description": "Paths (relative to repo root) to stage"},
                "all": {"type": "boolean", "description": "true = stage all changes"}
            },
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let paths = opt_str_vec(&args, "paths").unwrap_or_default();
        let all = opt_bool(&args, "all").unwrap_or(false);
        self.ctx.service.backend().stage(&repo, &paths, all).await?;
        ok_json(json!({ "ok": true }))
    }
}

pub struct GitRestoreTool {
    ctx: Arc<GitToolContext>,
}

impl GitRestoreTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitRestoreTool {
    fn name(&self) -> &str {
        "git_restore"
    }
    fn description(&self) -> &str {
        "Discard uncommitted working-tree changes for the given paths (relative to the repo root) in the active repository. Requires user confirmation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {"type": "array", "items": {"type": "string"}, "description": "Paths (relative to repo root) to discard changes for"}
            },
            "required": ["paths"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let paths = opt_str_vec(&args, "paths").unwrap_or_default();
        if paths.is_empty() {
            return Err(AppError::InvalidInput(
                "git_restore requires at least one path".into(),
            ));
        }
        let outcome = self
            .ctx
            .service
            .confirm()
            .request_and_wait(
                GitOpKind::Checkout,
                "撤销工作区改动".into(),
                json!({ "paths": paths }),
            )
            .await?;
        apply_outcome(outcome, async {
            self.ctx.service.backend().restore(&repo, &paths).await?;
            Ok(json!({ "paths": paths }))
        })
        .await
    }
}

pub struct GitCommitTool {
    ctx: Arc<GitToolContext>,
}

impl GitCommitTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }
    fn description(&self) -> &str {
        "Create a commit from the staged changes of the active repository with the given message. Requires user confirmation showing the staged diff summary."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "Commit message"}
            },
            "required": ["message"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let message = require_str(&args, "message")?;
        let detail = match self.ctx.service.backend().diff(&repo, true, None).await {
            Ok(d) => json!({
                "staged_files": d.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                "truncated": d.truncated
            }),
            Err(_) => json!({ "staged_files": [] }),
        };
        let outcome = self
            .ctx
            .service
            .confirm()
            .request_and_wait(GitOpKind::Commit, "提交暂存区改动".into(), detail)
            .await?;
        apply_outcome(outcome, async {
            self.ctx.service.backend().commit(&repo, &message).await?;
            Ok(json!({ "message": message }))
        })
        .await
    }
}

pub struct GitResetTool {
    ctx: Arc<GitToolContext>,
}

impl GitResetTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitResetTool {
    fn name(&self) -> &str {
        "git_reset"
    }
    fn description(&self) -> &str {
        "Reset the current branch of the active repository to a target (default HEAD). mode: mixed (default) / soft / hard / keep. --hard and --keep discard working-tree changes: they are dangerous writes gated by the dangerous-writes toggle (default off) plus user confirmation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["mixed", "soft", "hard", "keep"], "description": "Reset mode"},
                "target": {"type": "string", "description": "Commit/branch to reset to; default HEAD"}
            },
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let mode = GitResetMode::parse(&require_str(&args, "mode")?)?;
        let target = opt_str(&args, "target");
        if (mode == GitResetMode::Hard || mode == GitResetMode::Keep)
            && !self.ctx.service.dangerous_writes()
        {
            return Err(AppError::InvalidInput(
                "git reset --hard/--keep 会丢弃工作区改动，属危险写操作且默认关闭；请先开启「危险写操作」开关或改用 --soft/--mixed".into(),
            ));
        }
        let detail = if mode == GitResetMode::Hard || mode == GitResetMode::Keep {
            match self.ctx.service.backend().status(&repo).await {
                Ok(s) => json!({
                    "lost": s.staged.into_iter().chain(s.unstaged).map(|e| e.path).collect::<Vec<_>>()
                }),
                Err(_) => json!({ "lost": [] }),
            }
        } else {
            json!({ "lost": [] })
        };
        let outcome = self
            .ctx
            .service
            .confirm()
            .request_and_wait(
                GitOpKind::Reset,
                format!(
                    "重置到 {}（--{}）",
                    target.as_deref().unwrap_or("HEAD"),
                    mode.as_str()
                ),
                detail,
            )
            .await?;
        apply_outcome(outcome, async {
            let preview = self
                .ctx
                .service
                .backend()
                .reset(&repo, mode, target.as_deref())
                .await?;
            Ok(preview)
        })
        .await
    }
}

pub struct GitCheckoutTool {
    ctx: Arc<GitToolContext>,
}

impl GitCheckoutTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitCheckoutTool {
    fn name(&self) -> &str {
        "git_checkout"
    }
    fn description(&self) -> &str {
        "Switch the active repository to a branch/commit (target). If the working tree has uncommitted changes that would be overwritten, the operation is gated by the dangerous-writes toggle (default off) plus user confirmation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {"type": "string", "description": "Branch or commit to check out"}
            },
            "required": ["target"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let target = require_str(&args, "target")?;
        let dirty = match self.ctx.service.backend().status(&repo).await {
            Ok(s) => !s.unstaged.is_empty() || !s.untracked.is_empty(),
            Err(_) => false,
        };
        if dirty {
            if !self.ctx.service.dangerous_writes() {
                return Err(AppError::InvalidInput(
                    "checkout 将覆盖未提交改动，属危险写操作且默认关闭；请先提交/暂存改动或开启「危险写操作」开关".into(),
                ));
            }
            let outcome = self
                .ctx
                .service
                .confirm()
                .request_and_wait(
                    GitOpKind::Checkout,
                    format!("切换到 {target}（将覆盖未提交改动）"),
                    json!({ "target": target }),
                )
                .await?;
            return apply_outcome(outcome, async {
                self.ctx.service.backend().checkout(&repo, &target).await?;
                Ok(json!({ "target": target }))
            })
            .await;
        }
        self.ctx.service.backend().checkout(&repo, &target).await?;
        ok_json(json!({ "target": target, "ok": true }))
    }
}

pub struct GitStashTool {
    ctx: Arc<GitToolContext>,
}

impl GitStashTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitStashTool {
    fn name(&self) -> &str {
        "git_stash"
    }
    fn description(&self) -> &str {
        "Stash operations in the active repository. action: push (save changes, optional message) / apply / pop (apply and remove, requires confirmation) / drop (discard newest stash, requires confirmation)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["push", "pop", "drop", "apply"], "description": "Stash action"},
                "message": {"type": "string", "description": "Message for stash push"}
            },
            "required": ["action"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let action = GitStashAction::parse(&require_str(&args, "action")?)?;
        let message = opt_str(&args, "message");
        match action {
            GitStashAction::Push | GitStashAction::Apply => {
                self.ctx
                    .service
                    .backend()
                    .stash(&repo, action, message.as_deref())
                    .await?;
                ok_json(json!({ "action": action.as_str(), "ok": true }))
            }
            GitStashAction::Pop => {
                let kind = GitOpKind::StashApply;
                let outcome = self
                    .ctx
                    .service
                    .confirm()
                    .request_and_wait(kind, "应用并移除最新 stash".into(), json!({}))
                    .await?;
                apply_outcome(outcome, async {
                    self.ctx.service.backend().stash(&repo, action, None).await?;
                    Ok(json!({ "action": "pop" }))
                })
                .await
            }
            GitStashAction::Drop => {
                let outcome = self
                    .ctx
                    .service
                    .confirm()
                    .request_and_wait(GitOpKind::StashDrop, "丢弃最新 stash".into(), json!({}))
                    .await?;
                apply_outcome(outcome, async {
                    self.ctx.service.backend().stash(&repo, action, None).await?;
                    Ok(json!({ "action": "drop" }))
                })
                .await
            }
        }
    }
}

pub struct GitPushTool {
    ctx: Arc<GitToolContext>,
}

impl GitPushTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitPushTool {
    fn name(&self) -> &str {
        "git_push"
    }
    fn description(&self) -> &str {
        "Push commits of the active repository to a remote (default: current branch's upstream). Requires user confirmation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "remote": {"type": "string", "description": "Remote name; default origin"},
                "branch": {"type": "string", "description": "Branch to push; default current branch"}
            },
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let remote = opt_str(&args, "remote");
        let branch = opt_str(&args, "branch");
        let detail = match self.ctx.service.backend().status(&repo).await {
            Ok(s) => json!({ "branch": s.branch, "ahead": s.ahead }),
            Err(_) => json!({}),
        };
        let outcome = self
            .ctx
            .service
            .confirm()
            .request_and_wait(GitOpKind::Push, "推送到远程分支".into(), detail)
            .await?;
        apply_outcome(outcome, async {
            self.ctx
                .service
                .backend()
                .push(&repo, remote.as_deref(), branch.as_deref())
                .await?;
            Ok(json!({
                "remote": remote.clone().unwrap_or_else(|| "origin".into()),
                "branch": branch.clone()
            }))
        })
        .await
    }
}

pub struct GitPullTool {
    ctx: Arc<GitToolContext>,
}

impl GitPullTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitPullTool {
    fn name(&self) -> &str {
        "git_pull"
    }
    fn description(&self) -> &str {
        "Pull and merge remote changes into the current branch of the active repository. Requires user confirmation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn execute(&self, _args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let outcome = self
            .ctx
            .service
            .confirm()
            .request_and_wait(GitOpKind::Pull, "拉取并合并远程改动".into(), json!({}))
            .await?;
        apply_outcome(outcome, async {
            self.ctx.service.backend().pull(&repo).await?;
            Ok(json!({ "ok": true }))
        })
        .await
    }
}

pub struct GitResolveConflictTool {
    ctx: Arc<GitToolContext>,
}

impl GitResolveConflictTool {
    pub fn new(ctx: Arc<GitToolContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for GitResolveConflictTool {
    fn name(&self) -> &str {
        "git_resolve_conflict"
    }
    fn description(&self) -> &str {
        "Resolve a merge conflict for a single path (relative to the repo root) in the active repository. take: ours (keep our side), theirs (keep their side), both (merge both sides with conflict markers). Write operation."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Conflicted path relative to the repo root"},
                "take": {"type": "string", "enum": ["ours", "theirs", "both"], "description": "Which side to take"}
            },
            "required": ["path", "take"],
            "additionalProperties": false
        })
    }
    async fn execute(&self, args: Value) -> AppResult<String> {
        let repo = self.ctx.service.active_repo().await?;
        let path = require_str(&args, "path")?;
        let take = ConflictTake::parse(&require_str(&args, "take")?)?;
        self.ctx
            .service
            .backend()
            .resolve_conflict(&repo, &path, take)
            .await?;
        ok_json(json!({ "path": path, "take": match take {
            ConflictTake::Ours => "ours",
            ConflictTake::Theirs => "theirs",
            ConflictTake::Both => "both",
        }, "ok": true }))
    }
}
