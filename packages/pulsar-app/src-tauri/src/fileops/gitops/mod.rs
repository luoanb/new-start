//! Git 操作领域模块：稳定接口（`GitBackend` trait）+ 跨消费方统一数据结构。
//!
//! 消费方：
//! - GUI：Tauri commands（`lib.rs`）与前端 GitPanel / GitDiff / 文件树徽标
//! - AI：`tools.rs` 的 15 个原生 git 工具
//! - 远程模式：`net/rpc.rs` 同名命令转发
//! - 未来 TUI：复用同一 trait + 确认服务事件
//!
//! 第一版唯一实现为 `CliGitBackend`（`repo.rs`，spawn git CLI，参数数组不经 shell，
//! 行为与用户命令行 git 一致——用户 git 即唯一真相）。

pub mod confirm;
pub mod repo;
pub mod service;
pub mod tools;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::core::error::AppResult;

/// 发现的仓库（根在 workspace 内，canonicalize 校验）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitRepo {
    /// 稳定 id：由 canonicalized repo 根派生（同 workspace id_for_root 策略）。
    pub id: String,
    /// 展示名（repo 根目录名）。
    pub name: String,
    /// repo 绝对根。
    pub root: PathBuf,
    /// 是否为嵌套 repo（位于其他 repo 内）。
    pub is_nested: bool,
}

/// git status 视图（porcelain v1 解析；目录聚合由前端完成）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitStatusView {
    /// 当前分支（detached 时为 None）。
    pub branch: Option<String>,
    /// 领先远端提交数。
    pub ahead: i64,
    /// 落后远端提交数。
    pub behind: i64,
    /// 已暂存。
    pub staged: Vec<GitStatusEntry>,
    /// 未暂存（工作区）。
    pub unstaged: Vec<GitStatusEntry>,
    /// 未跟踪。
    pub untracked: Vec<GitStatusEntry>,
    /// 冲突（U）。
    pub conflicted: Vec<GitStatusEntry>,
}

/// 单条 status 项：`status` 为 X/Y 组合（如 "MM"、"A"、"??"、"UU"）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitStatusEntry {
    /// 相对 repo 根（`/` 分隔）。
    pub path: String,
    /// M/A/D/R/?/U 等单字母或组合。
    pub status: String,
    /// 前端目录聚合用。
    pub is_dir: bool,
}

/// unified diff 视图（解析 `git diff` / `git diff --cached` 输出）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitDiff {
    pub files: Vec<GitFileDiff>,
    /// 输出超限被截断。
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitFileDiff {
    pub path: String,
    /// M/A/D/R/? 等。
    pub status: String,
    /// LFS 指针 / 二进制 → 前端显示提示不渲染正文。
    pub is_binary: bool,
    pub hunks: Vec<GitHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    /// 原始头 `@@ -a,b +c,d @@ ctx`。
    pub header: String,
    pub lines: Vec<GitDiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffLine {
    pub kind: GitDiffLineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
}

/// log 条目（`git log --format=...` 解析）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitInfo {
    /// 完整 hash。
    pub hash: String,
    /// 7 位短 hash。
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

/// 某提交中单个变更文件的统计（`git show --numstat` 解析）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitShowFile {
    /// 相对 repo 根的文件路径。
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    /// 二进制 / LFS 指针文件（numstat 为 `-`）→ 前端不渲染正文。
    pub is_binary: bool,
}

/// blame 行（`git blame --porcelain` 解析，行维度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBlameLine {
    pub line_no: usize,
    /// commit 短 hash。
    pub short: String,
    pub author: String,
    pub date: String,
    pub text: String,
}

/// stash 条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStashEntry {
    /// stash@{n} 的 n。
    pub index: usize,
    pub message: String,
}

/// 分支条目（本地 + 远端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBranchItem {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
}

/// reset 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitResetMode {
    Mixed,
    Soft,
    Hard,
    Keep,
}

impl GitResetMode {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "mixed" => Ok(Self::Mixed),
            "soft" => Ok(Self::Soft),
            "hard" => Ok(Self::Hard),
            "keep" => Ok(Self::Keep),
            other => Err(crate::core::error::AppError::InvalidInput(format!(
                "invalid git reset mode: {other}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Soft => "soft",
            Self::Hard => "hard",
            Self::Keep => "keep",
        }
    }
}

/// reset dry-run 预览：hard 场景将丢失改动文件清单。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitResetPreview {
    pub lost: Vec<String>,
}

/// stash 动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitStashAction {
    Push,
    Pop,
    Drop,
    Apply,
}

impl GitStashAction {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "push" => Ok(Self::Push),
            "pop" => Ok(Self::Pop),
            "drop" => Ok(Self::Drop),
            "apply" => Ok(Self::Apply),
            other => Err(crate::core::error::AppError::InvalidInput(format!(
                "invalid git stash action: {other}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Pop => "pop",
            Self::Drop => "drop",
            Self::Apply => "apply",
        }
    }
}

/// 冲突解决取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictTake {
    Ours,
    Theirs,
    Both,
}

impl ConflictTake {
    pub fn parse(s: &str) -> AppResult<Self> {
        match s {
            "ours" => Ok(Self::Ours),
            "theirs" => Ok(Self::Theirs),
            "both" => Ok(Self::Both),
            other => Err(crate::core::error::AppError::InvalidInput(format!(
                "invalid conflict take: {other}"
            ))),
        }
    }
}

/// 稳定接口：git 操作抽象。GUI / TUI / 远程 / AI 工具共用。
/// 写操作（stage/commit/reset/...）不内置确认逻辑——确认门禁由调用方
/// （`GitConfirmService`）负责，接口保持纯操作语义。
#[async_trait::async_trait]
pub trait GitBackend: Send + Sync {
    /// 发现 workspace 内所有 repo（仅向内扫描，禁止外查）。
    async fn discover_repos(&self, ws_root: &Path, ignore: &[String]) -> AppResult<Vec<GitRepo>>;

    // ── 只读 ──
    async fn status(&self, repo: &GitRepo) -> AppResult<GitStatusView>;
    async fn diff(&self, repo: &GitRepo, cached: bool, path: Option<&str>) -> AppResult<GitDiff>;
    async fn log(&self, repo: &GitRepo, limit: usize) -> AppResult<Vec<GitCommitInfo>>;
    /// 某提交的变更文件统计列表（`git show --numstat`）。
    async fn show_files(&self, repo: &GitRepo, hash: &str) -> AppResult<Vec<GitShowFile>>;
    /// 某提交中单个文件的 unified diff（复用 `parse_diff` 结构）。
    async fn show_diff(&self, repo: &GitRepo, hash: &str, path: &str) -> AppResult<GitFileDiff>;
    async fn branches(&self, repo: &GitRepo) -> AppResult<Vec<GitBranchItem>>;
    async fn blame(&self, repo: &GitRepo, path: &str) -> AppResult<Vec<GitBlameLine>>;
    async fn stash_list(&self, repo: &GitRepo) -> AppResult<Vec<GitStashEntry>>;

    // ── 写操作（调用方负责确认门禁）──
    async fn stage(&self, repo: &GitRepo, paths: &[String], all: bool) -> AppResult<()>;
    async fn unstage(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()>;
    async fn restore(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()>;
    async fn commit(&self, repo: &GitRepo, message: &str) -> AppResult<()>;
    /// 高危写：先 dry-run 计算将丢失改动清单（preview），再执行。
    async fn reset(
        &self,
        repo: &GitRepo,
        mode: GitResetMode,
        target: Option<&str>,
    ) -> AppResult<GitResetPreview>;
    /// 高危写：checkout 分支/路径；丢弃工作区改动需确认。
    async fn checkout(&self, repo: &GitRepo, target: &str) -> AppResult<()>;
    async fn stash(
        &self,
        repo: &GitRepo,
        action: GitStashAction,
        message: Option<&str>,
    ) -> AppResult<()>;
    async fn push(&self, repo: &GitRepo, remote: Option<&str>, branch: Option<&str>)
    -> AppResult<()>;
    async fn pull(&self, repo: &GitRepo) -> AppResult<()>;
    /// 冲突解决：ours / theirs / both。
    async fn resolve_conflict(&self, repo: &GitRepo, path: &str, take: ConflictTake) -> AppResult<()>;
}

/// 校验相对 repo 根的路径参数：拒绝绝对路径与 `..` 逃逸。
pub(crate) fn validate_rel_path(path: &str) -> AppResult<String> {
    let p = path.trim();
    if p.is_empty() {
        return Err(crate::core::error::AppError::InvalidInput(
            "git path must not be empty".into(),
        ));
    }
    if Path::new(p).is_absolute() {
        return Err(crate::core::error::AppError::InvalidInput(format!(
            "git path must be relative to repo root: {p}"
        )));
    }
    for comp in p.split(['/', '\\']) {
        if comp == ".." {
            return Err(crate::core::error::AppError::InvalidInput(format!(
                "git path escapes repo root: {p}"
            )));
        }
    }
    Ok(p.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_mode_parse_roundtrip() {
        for (s, m) in [
            ("mixed", GitResetMode::Mixed),
            ("soft", GitResetMode::Soft),
            ("hard", GitResetMode::Hard),
            ("keep", GitResetMode::Keep),
        ] {
            assert_eq!(GitResetMode::parse(s).unwrap(), m);
            assert_eq!(m.as_str(), s);
        }
        assert!(GitResetMode::parse("nuke").is_err());
    }

    #[test]
    fn stash_action_parse_roundtrip() {
        for (s, a) in [
            ("push", GitStashAction::Push),
            ("pop", GitStashAction::Pop),
            ("drop", GitStashAction::Drop),
            ("apply", GitStashAction::Apply),
        ] {
            assert_eq!(GitStashAction::parse(s).unwrap(), a);
            assert_eq!(a.as_str(), s);
        }
    }

    #[test]
    fn conflict_take_parse() {
        assert_eq!(ConflictTake::parse("ours").unwrap(), ConflictTake::Ours);
        assert_eq!(ConflictTake::parse("theirs").unwrap(), ConflictTake::Theirs);
        assert_eq!(ConflictTake::parse("both").unwrap(), ConflictTake::Both);
        assert!(ConflictTake::parse("mine").is_err());
    }

    #[test]
    fn validate_rel_path_rejects_escape() {
        for bad in ["/etc/passwd", "../outside", "a/../../outside", "", "  "] {
            assert!(validate_rel_path(bad).is_err(), "{bad} should be rejected");
        }
        assert_eq!(validate_rel_path("src/main.rs").unwrap(), "src/main.rs");
        assert_eq!(validate_rel_path("a\\b").unwrap(), "a/b");
    }

    #[test]
    fn status_entry_serializes_snake_case() {
        let entry = GitStatusEntry {
            path: "a.txt".into(),
            status: "MM".into(),
            is_dir: false,
        };
        let v = serde_json::to_value(entry).unwrap();
        assert_eq!(v["path"], "a.txt");
        assert_eq!(v["is_dir"], false);
        assert!(v.get("is_dir").is_some());
    }

    #[test]
    fn diff_line_kind_serializes_string() {
        let line = GitDiffLine {
            kind: GitDiffLineKind::Add,
            old_no: None,
            new_no: Some(3),
            text: "+hello".into(),
        };
        let v = serde_json::to_value(line).unwrap();
        assert_eq!(v["kind"], "add");
    }
}
