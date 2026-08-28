//! `CliGitBackend`：GitBackend trait 的 spawn-git-CLI 实现。
//!
//! 安全模型（与 `cmd_exec` 同范式）：
//! - 所有命令 `Command::new("git").arg("-C").arg(repo_root).args(...)` 参数数组，**不经 shell**；
//! - `-C` 根为 `GitRepo.root`（本身已通过 workspace 前缀校验）；
//! - 路径参数一律加 `--` 分隔；路径经 `validate_rel_path` 拒绝绝对路径与 `..`；
//! - 超时 / 并发 / 输出截断复用 `cmd_exec` 常量（30s / 4 并发 / 64KB）。

use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

use super::super::fs::is_ignored;
use super::{
    validate_rel_path, ConflictTake, GitBackend, GitBlameLine, GitBranchItem, GitCommitInfo,
    GitDiff, GitDiffLine, GitDiffLineKind, GitFileDiff, GitHunk, GitRepo, GitResetMode,
    GitResetPreview, GitShowFile, GitStashAction, GitStashEntry, GitStatusEntry, GitStatusView,
};
use crate::core::cmd_exec::{truncate_output, MAX_CONCURRENT, MAX_OUTPUT_CHARS};
use crate::core::error::{AppError, AppResult};

/// repo 发现上限。
const MAX_DEPTH: usize = 8;
const MAX_REPOS: usize = 50;
/// diff 解析上限（超出标记 truncated，前端懒渲染）。
const MAX_DIFF_FILES: usize = 200;
const MAX_DIFF_HUNKS: usize = 500;
const MAX_DIFF_LINES: usize = 20_000;
/// blame 解析上限（大文件 blame 可达数万行，防结果撑爆上下文）。
const MAX_BLAME_LINES: usize = 5_000;

pub struct CliGitBackend {
    semaphore: Semaphore,
}

impl CliGitBackend {
    pub fn new() -> Self {
        Self {
            semaphore: Semaphore::new(MAX_CONCURRENT),
        }
    }

    /// 通用执行：`git -C <root> <args...>`，参数数组不经 shell。
    async fn run_git(&self, repo_root: &Path, args: &[&str]) -> AppResult<GitOutput> {
        let started = std::time::Instant::now();
        tracing::info!(
            target: "gitops",
            root = %repo_root.display(),
            args = %args.join(" "),
            "git run_git start"
        );
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| AppError::RuntimeError(format!("git semaphore acquire failed: {e}")))?;
        tracing::info!(
            target: "gitops",
            args = %args.join(" "),
            wait_ms = started.elapsed().as_millis(),
            "git semaphore acquired"
        );

        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        cmd.args(args);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            AppError::RuntimeError(format!("git spawn failed (is git installed?): {e}"))
        })?;
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // 先并发读取 stdout/stderr 再等进程退出：输出超过管道缓冲（约 64KB）时，
        // 子进程写满缓冲区会阻塞无法退出，若先 wait 后读会形成管道死锁直到超时。
        let wait_fut = child.wait();
        let read_stdout = async {
            match stdout_pipe {
                Some(mut r) => {
                    let mut buf = Vec::new();
                    r.read_to_end(&mut buf)
                        .await
                        .map_err(|e| AppError::RuntimeError(format!("git read stdout failed: {e}")))?;
                    Ok::<Vec<u8>, AppError>(buf)
                }
                None => Ok::<Vec<u8>, AppError>(Vec::new()),
            }
        };
        let read_stderr = async {
            match stderr_pipe {
                Some(mut r) => {
                    let mut buf = Vec::new();
                    r.read_to_end(&mut buf)
                        .await
                        .map_err(|e| AppError::RuntimeError(format!("git read stderr failed: {e}")))?;
                    Ok::<Vec<u8>, AppError>(buf)
                }
                None => Ok::<Vec<u8>, AppError>(Vec::new()),
            }
        };

        let (wait_res, stdout_res, stderr_res) = match tokio::time::timeout(
            Duration::from_millis(crate::core::cmd_exec::DEFAULT_TIMEOUT_MS),
            async { tokio::join!(wait_fut, read_stdout, read_stderr) },
        )
        .await
        {
            Ok(v) => v,
            Err(_elapsed) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                tracing::error!(
                    target: "gitops",
                    args = %args.join(" "),
                    elapsed_ms = started.elapsed().as_millis(),
                    "git run_git timed out"
                );
                return Err(AppError::RuntimeError(format!(
                    "git command timed out after {}ms",
                    crate::core::cmd_exec::DEFAULT_TIMEOUT_MS
                )));
            }
        };

        let status = wait_res
            .map_err(|e| AppError::RuntimeError(format!("git wait failed: {e}")))?;
        let stdout_bytes = stdout_res?;
        let stderr_bytes = stderr_res?;

        tracing::info!(
            target: "gitops",
            args = %args.join(" "),
            exit_code = status.code().unwrap_or(-1),
            stdout_bytes = stdout_bytes.len(),
            stderr_bytes = stderr_bytes.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "git run_git done"
        );

        Ok(GitOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: truncate_output(&stdout_bytes, MAX_OUTPUT_CHARS),
            stderr: truncate_output(&stderr_bytes, MAX_OUTPUT_CHARS),
            stdout_bytes,
        })
    }

    /// 执行并断言成功：失败返回携带 stderr 的 RuntimeError。
    async fn run_git_ok(&self, repo_root: &Path, args: &[&str]) -> AppResult<GitOutput> {
        let out = self.run_git(repo_root, args).await?;
        if out.exit_code != 0 {
            return Err(AppError::RuntimeError(format!(
                "git {} failed (exit {}): {}",
                args.first().unwrap_or(&""),
                out.exit_code,
                out.stderr.trim()
            )));
        }
        Ok(out)
    }

    fn repo_root(repo: &GitRepo) -> AppResult<PathBuf> {
        if !repo.root.is_dir() {
            return Err(AppError::RuntimeError(format!(
                "repo root not accessible: {}",
                repo.root.display()
            )));
        }
        Ok(repo.root.clone())
    }
}

struct GitOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
    stdout_bytes: Vec<u8>,
}

impl Default for CliGitBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────
// 解析器（porcelain v1 / unified diff / log / blame / branch / stash）
// ──────────────────────────────────────────────

/// porcelain v1 路径的轻量 unquote（含特殊字符时 git 输出 C 引号字符串）。
fn unquote_status_path(s: &str) -> String {
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `R  old -> new` / `C  old -> new` 取目标路径；其余原样。
fn status_path(rest: &str) -> String {
    let path = match rest.find(" -> ") {
        Some(i) => rest[i + 4..].trim().to_string(),
        None => rest.to_string(),
    };
    unquote_status_path(&path)
}

/// 解析 `git status --porcelain --branch` 输出。
fn parse_status(output: &str) -> GitStatusView {
    let mut view = GitStatusView::default();
    let mut lines = output.lines();

    if let Some(first) = lines.next() {
        if let Some(info) = first.strip_prefix("## ") {
            // `## main...origin/main [ahead 1, behind 2]` 或 `## HEAD (no branch)`
            let (branch_part, trailing) = match info.find("...") {
                Some(i) => (&info[..i], &info[i + 3..]),
                None => (info, ""),
            };
            if branch_part == "HEAD" || branch_part.contains("(no branch)") {
                view.branch = None;
            } else if let Some(idx) = branch_part.find(" on ") {
                // 空仓库：`## No commits yet on main`
                view.branch = Some(branch_part[idx + 4..].to_string());
            } else {
                view.branch = Some(branch_part.to_string());
            }
            if let Some(meta) = trailing.split('[').nth(1) {
                for part in meta.trim_end_matches(']').split(',') {
                    let part = part.trim();
                    if let Some(n) = part.strip_prefix("ahead ") {
                        view.ahead = n.trim().parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        view.behind = n.trim().parse().unwrap_or(0);
                    }
                }
            }
        } else if !first.trim().is_empty() {
            // 无 branch 行（极少见）时首行也按 entry 处理
            if let Some(entry) = parse_status_line(first) {
                push_status_entry(&mut view, entry);
            }
        }
    }

    for line in lines {
        if let Some(entry) = parse_status_line(line) {
            push_status_entry(&mut view, entry);
        }
    }
    view
}

fn parse_status_line(line: &str) -> Option<GitStatusEntry> {
    if line.len() < 4 {
        return None;
    }
    let mut chars = line.chars();
    let x = chars.next()?;
    let y = chars.next()?;
    chars.next()?; // 空格分隔
    let rest = chars.as_str().trim();
    if rest.is_empty() {
        return None;
    }
    Some(GitStatusEntry {
        path: status_path(rest),
        status: format!("{x}{y}"),
        is_dir: false,
    })
}

fn push_status_entry(view: &mut GitStatusView, entry: GitStatusEntry) {
    let x = entry.status.chars().next().unwrap_or(' ');
    let y = entry.status.chars().nth(1).unwrap_or(' ');
    if x == 'U' || y == 'U' {
        view.conflicted.push(entry);
        return;
    }
    if x == '?' && y == '?' {
        view.untracked.push(entry);
        return;
    }
    // 暂存区 / 工作区是独立维度：MM 等双状态文件同时出现在两个分组（VS Code SCM 语义）。
    if x != ' ' && x != '?' {
        view.staged.push(entry.clone());
    }
    if y != ' ' && y != '?' {
        view.unstaged.push(entry);
    }
}

/// hunk 头：`@@ -a,b +c,d @@ ctx`（b/d 可省略，缺省 1）。
fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize, String)> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, ctx) = rest.split_once(" @@")?;
    let mut it = ranges.split_whitespace();
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let old_start = old
        .split(',')
        .next()
        .and_then(|n| n.parse::<usize>().ok())?;
    let new_start = new
        .split(',')
        .next()
        .and_then(|n| n.parse::<usize>().ok())?;
    let old_lines = old
        .split_once(',')
        .and_then(|(_, n)| n.parse::<usize>().ok())
        .unwrap_or(1);
    let new_lines = new
        .split_once(',')
        .and_then(|(_, n)| n.parse::<usize>().ok())
        .unwrap_or(1);
    Some((
        old_start,
        old_lines,
        new_start,
        new_lines,
        format!("@@ {ranges} @@{ctx}"),
    ))
}

/// 解析 unified diff 输出。
fn parse_diff(output: &str, mut truncated: bool) -> GitDiff {
    let mut diff = GitDiff::default();
    diff.truncated = truncated;
    let mut current: Option<GitFileDiff> = None;
    let mut line_count = 0usize;

    let take = |diff: &mut GitDiff, current: &mut Option<GitFileDiff>| {
        if let Some(f) = current.take() {
            diff.files.push(f);
        }
    };

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            take(&mut diff, &mut current);
            if diff.files.len() >= MAX_DIFF_FILES {
                truncated = true;
                break;
            }
            // `a/x b/y` → 取 `b/` 后路径（rename 场景 b 为目标）
            let path = rest
                .rsplit_once(" b/")
                .map(|(_, p)| p.to_string())
                .unwrap_or_else(|| rest.to_string());
            current = Some(GitFileDiff {
                path: unquote_status_path(&path),
                status: "M".into(),
                is_binary: false,
                hunks: Vec::new(),
            });
            continue;
        }
        let Some(file) = current.as_mut() else {
            continue;
        };
        if line.starts_with("new file mode") {
            file.status = "A".into();
        } else if line.starts_with("deleted file mode") {
            file.status = "D".into();
        } else if line.starts_with("similarity index")
            || line.starts_with("rename from")
            || line.starts_with("rename to")
        {
            file.status = "R".into();
        } else if line.starts_with("Binary files") {
            file.is_binary = true;
        } else if line.starts_with("GIT binary patch") {
            file.is_binary = true;
        } else if let Some((os, ol, ns, nl, header)) = parse_hunk_header(line) {
            if file.hunks.len() >= MAX_DIFF_HUNKS {
                truncated = true;
                break;
            }
            file.hunks.push(GitHunk {
                old_start: os,
                old_lines: ol,
                new_start: ns,
                new_lines: nl,
                header,
                lines: Vec::new(),
            });
        } else if line.starts_with("\\ No newline") {
            // 行尾标记，跳过
        } else if line.starts_with("--- ") || line.starts_with("+++ ") || line == "---" {
            // 文件头行，跳过
        } else if let Some(hunk) = file.hunks.last_mut() {
            if line_count >= MAX_DIFF_LINES {
                truncated = true;
                break;
            }
            let (kind, old_no, new_no) = classify_diff_line(line, hunk);
            hunk.lines.push(GitDiffLine {
                kind,
                old_no,
                new_no,
                text: line.to_string(),
            });
            line_count += 1;
            // LFS 指针检测：新增行以 `version https://git-lfs` / `oid sha256:` 开头
            if kind == GitDiffLineKind::Add
                && (line.starts_with("+version https://git-lfs")
                    || line.starts_with("+oid sha256:"))
            {
                file.is_binary = true;
            }
        }
    }
    take(&mut diff, &mut current);
    diff.truncated = truncated;
    diff
}

/// 依据 hunk 内已收集行推算当前行在旧/新文件中的行号。
fn classify_diff_line(
    line: &str,
    hunk: &GitHunk,
) -> (GitDiffLineKind, Option<usize>, Option<usize>) {
    let mut old_next = hunk.old_start;
    let mut new_next = hunk.new_start;
    for l in &hunk.lines {
        match l.kind {
            GitDiffLineKind::Context => {
                old_next += 1;
                new_next += 1;
            }
            GitDiffLineKind::Add => new_next += 1,
            GitDiffLineKind::Del => old_next += 1,
        }
    }
    if line.starts_with('+') {
        (GitDiffLineKind::Add, None, Some(new_next))
    } else if line.starts_with('-') {
        (GitDiffLineKind::Del, Some(old_next), None)
    } else {
        (GitDiffLineKind::Context, Some(old_next), Some(new_next))
    }
}

/// 解析 `git log -n N --format=...`（tab 分隔 5 字段）。
fn parse_log(output: &str, limit: usize) -> Vec<GitCommitInfo> {
    output
        .lines()
        .filter(|l| !l.is_empty())
        .take(limit)
        .filter_map(|line| {
            let mut parts = line.splitn(5, '\t');
            let hash = parts.next()?.to_string();
            let short = parts.next()?.to_string();
            let author = parts.next()?.to_string();
            let date = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").to_string();
            Some(GitCommitInfo {
                hash,
                short,
                author,
                date,
                subject,
            })
        })
        .collect()
}

/// 解析 `git show --numstat --format=`（每行 `add\tdel\tpath`；二进制为 `-\t-\tpath`）。
fn parse_numstat(output: &str) -> Vec<GitShowFile> {
    let mut files = Vec::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        let (additions, deletions, is_binary) = match (add, del) {
            ("-", "-") => (0, 0, true),
            (a, d) => (a.parse().unwrap_or(0), d.parse().unwrap_or(0), false),
        };
        files.push(GitShowFile {
            path: path.to_string(),
            additions,
            deletions,
            is_binary,
        });
    }
    files
}

/// 校验提交引用参数（hash / ref 名）：拒绝空、前导 `-` 与空白（防选项注入）。
fn validate_rev(rev: &str) -> AppResult<String> {
    let r = rev.trim();
    if r.is_empty() || r.starts_with('-') || r.chars().any(char::is_whitespace) {
        return Err(AppError::InvalidInput("invalid git revision".into()));
    }
    Ok(r.to_string())
}

/// 解析 `git for-each-ref ... --format=%(refname:short)%09%(HEAD)%09%(upstream:short)`。
fn parse_branches(output: &str) -> Vec<GitBranchItem> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let head = parts.next().unwrap_or("").trim();
            let upstream = parts.next().unwrap_or("").trim();
            Some(GitBranchItem {
                name: name.to_string(),
                current: head == "*",
                upstream: if upstream.is_empty() {
                    None
                } else {
                    Some(upstream.to_string())
                },
            })
        })
        .collect()
}

fn ts_to_date(ts: &str) -> String {
    let secs: i64 = ts.trim().parse().unwrap_or(0);
    match chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0) {
        Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        None => ts.trim().to_string(),
    }
}

/// 解析 `git blame --porcelain` 输出（行维度）。
fn parse_blame(output: &str) -> Vec<GitBlameLine> {
    let mut result: Vec<GitBlameLine> = Vec::new();
    let mut sha = String::new();
    let mut author = String::new();
    let mut date = String::new();
    let mut final_line = 0usize;
    let mut code_lines: Vec<(usize, String)> = Vec::new();

    let flush = |code_lines: &mut Vec<(usize, String)>,
                     result: &mut Vec<GitBlameLine>,
                     sha: &str,
                     author: &str,
                     date: &str| {
        for (line_no, text) in code_lines.drain(..) {
            result.push(GitBlameLine {
                line_no,
                short: sha.chars().take(7).collect(),
                author: author.to_string(),
                date: date.to_string(),
                text,
            });
        }
    };

    for line in output.lines() {
        if is_commit_header(line) {
            flush(&mut code_lines, &mut result, &sha, &author, &date);
            let mut it = line.split_whitespace();
            sha = it.next().unwrap_or("").to_string();
            it.next(); // orig-line
            final_line = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            // 注意：author/date 由组第一行的 meta（`author ...` / `author-time ...`）写入，
            // 组内后续行的 header（`sha orig final`，无 meta）不得清空，否则组内行丢失归属。
        } else if let Some(v) = line.strip_prefix("author ") {
            author = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("author-time ") {
            date = ts_to_date(v);
        } else if is_blame_meta(line) {
            // 其他 header（author-mail / committer / summary / previous / filename / tab 行）跳过
        } else if let Some(code) = line.strip_prefix('\t') {
            // porcelain 代码行以 tab 前缀标记；剥离后即为真实代码内容。
            code_lines.push((final_line, code.to_string()));
            final_line += 1;
        }
    }
    flush(&mut code_lines, &mut result, &sha, &author, &date);
    result
}

fn is_commit_header(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 40 {
        return false;
    }
    // 按字节判断前 40 位是否全为 ASCII hex：blame header 行前 40 字节是 commit hash，
    // 用 `split_at(40)` 会因第 40 字节落在多字节字符（如中文代码行）中间而 panic。
    bytes[..40].iter().all(|b| b.is_ascii_hexdigit())
}

fn is_blame_meta(line: &str) -> bool {
    line.starts_with("author-mail ")
        || line.starts_with("author-tz ")
        || line.starts_with("committer")
        || line.starts_with("summary ")
        || line.starts_with("previous ")
        || line.starts_with("filename ")
}

/// 解析 `git stash list --format=%gd%x09%s`。
fn parse_stash_list(output: &str) -> Vec<GitStashEntry> {
    output
        .lines()
        .filter_map(|line| {
            let (name, message) = match line.split_once('\t') {
                Some((n, m)) => (n, m),
                None => (line, ""),
            };
            let index = name
                .trim_start_matches("stash@{")
                .trim_end_matches('}')
                .parse::<usize>()
                .ok()?;
            Some(GitStashEntry {
                index,
                message: message.to_string(),
            })
        })
        .collect()
}

fn id_for_root(root: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    format!("repo-{:x}", hasher.finish())
}

// ──────────────────────────────────────────────
// GitBackend trait 实现
// ──────────────────────────────────────────────

#[async_trait::async_trait]
impl GitBackend for CliGitBackend {
    async fn discover_repos(&self, ws_root: &Path, ignore: &[String]) -> AppResult<Vec<GitRepo>> {
        let root_c = ws_root.canonicalize().map_err(|e| {
            AppError::InvalidInput(format!("workspace root not accessible: {e}"))
        })?;
        let mut repos: Vec<GitRepo> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root_c.clone(), 0));

        while let Some((dir, depth)) = queue.pop_front() {
            if repos.len() >= MAX_REPOS {
                break;
            }
            if depth > MAX_DEPTH {
                continue;
            }

            if dir.join(".git").exists() {
                if let Ok(out) = self.run_git(&dir, &["rev-parse", "--show-toplevel"]).await {
                    if out.exit_code == 0 {
                        let top = PathBuf::from(out.stdout.trim());
                        if let Ok(top_c) = top.canonicalize() {
                            if top_c.starts_with(&root_c) && seen.insert(top_c.clone()) {
                                let is_nested = repos
                                    .iter()
                                    .any(|r| r.root != top_c && top_c.starts_with(&r.root));
                                repos.push(GitRepo {
                                    id: id_for_root(&top_c),
                                    name: top_c
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| top_c.display().to_string()),
                                    root: top_c,
                                    is_nested,
                                });
                            }
                        }
                    }
                }
            }

            // 向下 BFS：跳过 ignore / .git，符号链接不跟随。
            if depth >= MAX_DEPTH {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for item in rd.flatten() {
                let Ok(ft) = item.file_type() else {
                    continue;
                };
                if !ft.is_dir() || ft.is_symlink() {
                    continue;
                }
                let name = item.file_name().to_string_lossy().into_owned();
                if name == ".git" {
                    continue;
                }
                let rel = item
                    .path()
                    .strip_prefix(&root_c)
                    .map(|p| {
                        p.components()
                            .map(|c| c.as_os_str().to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                    .unwrap_or_else(|_| name.clone());
                if is_ignored(&rel, ignore) {
                    continue;
                }
                queue.push_back((item.path(), depth + 1));
            }
        }
        Ok(repos)
    }

    async fn status(&self, repo: &GitRepo) -> AppResult<GitStatusView> {
        let root = Self::repo_root(repo)?;
        let out = self
            .run_git_ok(
                &root,
                // -uall：未跟踪目录展开为具体文件，避免面板把 `?? docs/` 渲染成文件夹条目
                // （目录条目点不开、看不到改动内容）。
                &["status", "--porcelain", "--branch", "--untracked-files=all"],
            )
            .await?;
        Ok(parse_status(&out.stdout))
    }

    async fn diff(&self, repo: &GitRepo, cached: bool, path: Option<&str>) -> AppResult<GitDiff> {
        let root = Self::repo_root(repo)?;
        let mut args: Vec<String> = vec!["diff".into(), "--no-color".into(), "--unified=3".into()];
        if cached {
            args.push("--cached".into());
        }
        if let Some(p) = path {
            let rel = validate_rel_path(p)?;
            args.push("--".into());
            args.push(rel);
        }
        let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run_git_ok(&root, &args_refs).await?;
        Ok(parse_diff(&out.stdout, false))
    }

    async fn log(&self, repo: &GitRepo, limit: usize, offset: usize) -> AppResult<Vec<GitCommitInfo>> {
        let root = Self::repo_root(repo)?;
        let n = limit.clamp(1, 200).to_string();
        let skip = offset.to_string();
        let out = self
            .run_git_ok(
                &root,
                &["log", "-n", &n, "--skip", &skip, "--format=%H%x09%h%x09%an%x09%aI%x09%s"],
            )
            .await?;
        Ok(parse_log(&out.stdout, limit.clamp(1, 200)))
    }

    async fn show_files(&self, repo: &GitRepo, hash: &str) -> AppResult<Vec<GitShowFile>> {
        let root = Self::repo_root(repo)?;
        let rev = validate_rev(hash)?;
        let out = self
            .run_git_ok(
                &root,
                &["show", "--numstat", "--format=", "--no-renames", &rev],
            )
            .await?;
        Ok(parse_numstat(&out.stdout))
    }

    async fn show_diff(&self, repo: &GitRepo, hash: &str, path: &str) -> AppResult<GitFileDiff> {
        let root = Self::repo_root(repo)?;
        let rev = validate_rev(hash)?;
        let rel = validate_rel_path(path)?;
        let out = self
            .run_git_ok(
                &root,
                &[
                    "show", "--no-color", "--unified=3", "--no-renames", &rev, "--", &rel,
                ],
            )
            .await?;
        let diff = parse_diff(&out.stdout, false);
        diff.files.into_iter().next().ok_or_else(|| {
            AppError::InvalidInput(format!("no diff for {rel} at {rev}"))
        })
    }

    async fn branches(&self, repo: &GitRepo) -> AppResult<Vec<GitBranchItem>> {
        let root = Self::repo_root(repo)?;
        let out = self
            .run_git_ok(
                &root,
                &[
                    "for-each-ref",
                    "refs/heads",
                    "refs/remotes",
                    "--format=%(refname:short)%09%(HEAD)%09%(upstream:short)",
                ],
            )
            .await?;
        Ok(parse_branches(&out.stdout))
    }

    async fn blame(&self, repo: &GitRepo, path: &str) -> AppResult<Vec<GitBlameLine>> {
        let started = std::time::Instant::now();
        tracing::info!(target: "gitops", path = %path, "git blame start");
        let root = Self::repo_root(repo)?;
        let rel = validate_rel_path(path)?;
        let out = self
            .run_git_ok(&root, &["blame", "--porcelain", "--", &rel])
            .await?;
        // 用完整输出解析：blame porcelain 行数多，truncate 后（64KB）会丢尾部行。
        let mut lines = parse_blame(&String::from_utf8_lossy(&out.stdout_bytes));
        // 解析行数上限：数万行 blame 若全量返回会撑爆模型上下文，超出标记 truncated。
        if lines.len() > MAX_BLAME_LINES {
            lines.truncate(MAX_BLAME_LINES);
            tracing::warn!(
                target: "gitops",
                path = %path,
                limit = MAX_BLAME_LINES,
                "git blame truncated to limit lines"
            );
        }
        tracing::info!(
            target: "gitops",
            path = %path,
            lines = lines.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "git blame done"
        );
        Ok(lines)
    }

    async fn stash_list(&self, repo: &GitRepo) -> AppResult<Vec<GitStashEntry>> {
        let root = Self::repo_root(repo)?;
        let out = self
            .run_git_ok(&root, &["stash", "list", "--format=%gd%x09%s"])
            .await?;
        Ok(parse_stash_list(&out.stdout))
    }

    async fn stage(&self, repo: &GitRepo, paths: &[String], all: bool) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        if all || paths.is_empty() {
            self.run_git_ok(&root, &["add", "-A"]).await?;
            return Ok(());
        }
        let mut args: Vec<String> = vec!["add".into(), "--".into()];
        for p in paths {
            args.push(validate_rel_path(p)?);
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_git_ok(&root, &refs).await?;
        Ok(())
    }

    async fn unstage(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        let mut args: Vec<String> = vec!["restore".into(), "--staged".into(), "--".into()];
        if paths.is_empty() {
            args.push(".".into());
        } else {
            for p in paths {
                args.push(validate_rel_path(p)?);
            }
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_git_ok(&root, &refs).await?;
        Ok(())
    }

    async fn restore(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        if paths.is_empty() {
            return Err(AppError::InvalidInput(
                "git restore requires at least one path".into(),
            ));
        }
        let mut args: Vec<String> = vec!["restore".into(), "--".into()];
        for p in paths {
            args.push(validate_rel_path(p)?);
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_git_ok(&root, &refs).await?;
        Ok(())
    }

    async fn commit(&self, repo: &GitRepo, message: &str) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        if message.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "git commit message must not be empty".into(),
            ));
        }
        // message 经参数数组传入（不做 -m 拼接，防选项注入）。
        self.run_git_ok(&root, &["commit", "-m", message]).await?;
        Ok(())
    }

    async fn reset(
        &self,
        repo: &GitRepo,
        mode: GitResetMode,
        target: Option<&str>,
    ) -> AppResult<GitResetPreview> {
        let root = Self::repo_root(repo)?;
        // dry-run preview：hard 场景下 staged + unstaged 改动会丢失。
        let lost = if mode == GitResetMode::Hard {
            let out = self.run_git_ok(&root, &["status", "--porcelain"]).await?;
            let view = parse_status(&out.stdout);
            // 双状态文件（如 MM）同时出现在 staged 与 unstaged，去重。
            let mut paths: Vec<String> = view
                .staged
                .into_iter()
                .chain(view.unstaged)
                .map(|e| e.path)
                .collect();
            paths.sort();
            paths.dedup();
            paths
        } else {
            Vec::new()
        };
        let mut args: Vec<String> = vec!["reset".into()];
        args.push(format!("--{}", mode.as_str()));
        if let Some(t) = target {
            if t.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "git reset target must not be empty".into(),
                ));
            }
            args.push(t.to_string());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_git_ok(&root, &refs).await?;
        Ok(GitResetPreview { lost })
    }

    async fn checkout(&self, repo: &GitRepo, target: &str) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        if target.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "git checkout target must not be empty".into(),
            ));
        }
        self.run_git_ok(&root, &["checkout", target]).await?;
        Ok(())
    }

    async fn stash(
        &self,
        repo: &GitRepo,
        action: GitStashAction,
        message: Option<&str>,
    ) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        match action {
            GitStashAction::Push => {
                if let Some(msg) = message {
                    if !msg.trim().is_empty() {
                        self.run_git_ok(&root, &["stash", "push", "-m", msg])
                            .await?;
                        return Ok(());
                    }
                }
                self.run_git_ok(&root, &["stash", "push"]).await?;
            }
            GitStashAction::Pop => {
                self.run_git_ok(&root, &["stash", "pop"]).await?;
            }
            GitStashAction::Drop => {
                self.run_git_ok(&root, &["stash", "drop"]).await?;
            }
            GitStashAction::Apply => {
                self.run_git_ok(&root, &["stash", "apply"]).await?;
            }
        }
        Ok(())
    }

    async fn push(
        &self,
        repo: &GitRepo,
        remote: Option<&str>,
        branch: Option<&str>,
    ) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        let mut args: Vec<String> = vec!["push".into()];
        if let Some(r) = remote {
            args.push(r.to_string());
        }
        if let Some(b) = branch {
            args.push(b.to_string());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_git_ok(&root, &refs).await?;
        Ok(())
    }

    async fn pull(&self, repo: &GitRepo) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        self.run_git_ok(&root, &["pull"]).await?;
        Ok(())
    }

    async fn resolve_conflict(
        &self,
        repo: &GitRepo,
        path: &str,
        take: ConflictTake,
    ) -> AppResult<()> {
        let root = Self::repo_root(repo)?;
        let rel = validate_rel_path(path)?;
        match take {
            ConflictTake::Ours => {
                self.run_git_ok(&root, &["checkout", "--ours", "--", &rel])
                    .await?;
                self.run_git_ok(&root, &["add", "--", &rel]).await?;
            }
            ConflictTake::Theirs => {
                self.run_git_ok(&root, &["checkout", "--theirs", "--", &rel])
                    .await?;
                self.run_git_ok(&root, &["add", "--", &rel]).await?;
            }
            ConflictTake::Both => {
                // 读 stage 2（ours）/ 3（theirs）内容拼接写文件后 add。
                let ours = self
                    .run_git_ok(&root, &["show", &format!(":2:{rel}")])
                    .await?;
                let theirs = self
                    .run_git_ok(&root, &["show", &format!(":3:{rel}")])
                    .await?;
                let mut content = String::from("<<<<<<< ours\n");
                content.push_str(&String::from_utf8_lossy(&ours.stdout_bytes));
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str("=======\n");
                content.push_str(&String::from_utf8_lossy(&theirs.stdout_bytes));
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(">>>>>>> theirs\n");
                let abs = root.join(&rel);
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&abs, content)?;
                self.run_git_ok(&root, &["add", "--", &rel]).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_repo(name: &str) -> PathBuf {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let root = std::env::temp_dir().join(format!("pulsar-git-{name}-{ms}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn run_git_proc(dir: &Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git proc")
    }

    fn git_available() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn status_parses_groups() {
        let out = "\
## main...origin/main [ahead 2, behind 1]
 M modified.txt
A  added.txt
R  old.txt -> renamed.txt
?? untracked.txt
UU conflicted.txt
";
        let view = parse_status(out);
        assert_eq!(view.branch.as_deref(), Some("main"));
        assert_eq!(view.ahead, 2);
        assert_eq!(view.behind, 1);
        assert_eq!(view.staged.len(), 2);
        assert!(view.staged.iter().any(|e| e.path == "added.txt" && e.status == "A "));
        assert!(view.staged.iter().any(|e| e.path == "renamed.txt" && e.status == "R "));
        assert_eq!(view.unstaged.len(), 1);
        assert_eq!(view.unstaged[0].path, "modified.txt");
        assert_eq!(view.unstaged[0].status, " M");
        assert_eq!(view.untracked.len(), 1);
        assert_eq!(view.untracked[0].path, "untracked.txt");
        assert_eq!(view.conflicted.len(), 1);
        assert_eq!(view.conflicted[0].path, "conflicted.txt");
    }

    #[test]
    fn status_detached_head() {
        let view = parse_status("## HEAD (no branch)\n M a.txt\n");
        assert_eq!(view.branch, None);
        assert_eq!(view.unstaged.len(), 1);
    }

    #[test]
    fn status_dual_state_appears_in_both_groups() {
        // MM：同一文件暂存区与工作区都有改动，应同时出现在 staged 与 unstaged（VS Code 语义）。
        let out = "\
## main
MM dual.txt
AM added_then_modified.txt
";
        let view = parse_status(out);
        assert!(view.staged.iter().any(|e| e.path == "dual.txt" && e.status == "MM"));
        assert!(view.unstaged.iter().any(|e| e.path == "dual.txt" && e.status == "MM"));
        assert!(view.staged.iter().any(|e| e.path == "added_then_modified.txt" && e.status == "AM"));
        assert!(view.unstaged.iter().any(|e| e.path == "added_then_modified.txt" && e.status == "AM"));
    }

    #[test]
    fn status_unquotes_paths() {
        let out = "?? \"a b.txt\"\n M \"x\\t\\\"q\"\n";
        let view = parse_status(out);
        assert_eq!(view.untracked[0].path, "a b.txt");
        assert_eq!(view.unstaged[0].path, "x\t\"q");
    }

    #[test]
    fn status_untracked_files_all_expands_directory_entries() {
        // `git status --untracked-files=all`：未跟踪目录不再折叠为 `?? dir/` 目录条目，
        // 而是展开为具体文件路径，面板才能逐文件展示与打开。
        let out = "## main\n?? docs/research/ai-assistant-capabilities.md\n?? untracked.txt\n";
        let view = parse_status(out);
        assert_eq!(view.untracked.len(), 2);
        assert_eq!(view.untracked[0].path, "docs/research/ai-assistant-capabilities.md");
        assert_eq!(view.untracked[1].path, "untracked.txt");
        // 不再出现以 / 结尾的目录条目
        assert!(!view.untracked.iter().any(|e| e.path.ends_with('/')));
    }

    #[test]
    fn diff_parses_hunks_and_binary() {
        let out = "\
diff --git a/a.txt b/a.txt
index 1111111..2222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,3 +1,4 @@
 context1
-old
+new
 context2
@@ -10,2 +11,2 @@
 keep
 changed
diff --git a/logo.png b/logo.png
index 3333333..4444444 100644
Binary files a/logo.png and b/logo.png differ
";
        let diff = parse_diff(out, false);
        assert!(!diff.truncated);
        assert_eq!(diff.files.len(), 2);
        let f0 = &diff.files[0];
        assert_eq!(f0.path, "a.txt");
        assert!(!f0.is_binary);
        assert_eq!(f0.hunks.len(), 2);
        let h0 = &f0.hunks[0];
        assert_eq!(h0.old_start, 1);
        assert_eq!(h0.new_start, 1);
        assert_eq!(h0.old_lines, 3);
        assert_eq!(h0.new_lines, 4);
        assert_eq!(h0.lines.len(), 4);
        assert_eq!(h0.lines[0].kind, GitDiffLineKind::Context);
        assert_eq!(h0.lines[0].old_no, Some(1));
        assert_eq!(h0.lines[0].new_no, Some(1));
        assert_eq!(h0.lines[1].kind, GitDiffLineKind::Del);
        assert_eq!(h0.lines[1].old_no, Some(2));
        assert_eq!(h0.lines[2].kind, GitDiffLineKind::Add);
        assert_eq!(h0.lines[2].new_no, Some(2));
        let f1 = &diff.files[1];
        assert_eq!(f1.path, "logo.png");
        assert!(f1.is_binary);
    }

    #[test]
    fn diff_parses_new_file() {
        let out = "\
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..5555555
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";
        let diff = parse_diff(out, false);
        assert_eq!(diff.files.len(), 1);
        let f = &diff.files[0];
        assert_eq!(f.status, "A");
        assert_eq!(f.hunks[0].old_start, 0);
        assert_eq!(f.hunks[0].new_start, 1);
        assert_eq!(f.hunks[0].lines[0].kind, GitDiffLineKind::Add);
    }

    #[test]
    fn diff_detects_lfs_pointer() {
        let out = "\
diff --git a/big.bin b/big.bin
index 1111111..2222222 100644
--- a/big.bin
+++ b/big.bin
@@ -1,2 +1,2 @@
-version https://git-lfs.github.com/spec/v1
+version https://git-lfs.github.com/spec/v2
 oid sha256:abcdef
";
        let diff = parse_diff(out, false);
        assert!(diff.files[0].is_binary, "LFS pointer should be flagged");
    }

    #[test]
    fn log_parses_fields() {
        let out = "\
abc1234567890abcdef\tabc1234\tAlice\t2026-08-01T10:00:00Z\tInitial commit
def4567890abcdef1234\tdef4567\tBob\t2026-08-02T11:00:00Z\tSecond commit
";
        let items = parse_log(out, 10);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].hash, "abc1234567890abcdef");
        assert_eq!(items[0].subject, "Initial commit");
        assert_eq!(items[1].author, "Bob");
        assert_eq!(items[1].subject, "Second commit");
    }

    #[test]
    fn numstat_parses_fields() {
        let out = "\
12\t3\tsrc/main.rs
1\t1\tREADME.md
-\t-\tassets/logo.png
";
        let files = parse_numstat(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].additions, 12);
        assert_eq!(files[0].deletions, 3);
        assert!(!files[0].is_binary);
        assert_eq!(files[1].additions, 1);
        assert_eq!(files[1].deletions, 1);
        assert!(files[2].is_binary);
        assert_eq!(files[2].additions, 0);
        assert_eq!(files[2].deletions, 0);
    }

    #[test]
    fn rev_validation_rejects_options_and_whitespace() {
        assert!(validate_rev("abc1234").is_ok());
        assert!(validate_rev("HEAD~1").is_ok());
        assert!(validate_rev("").is_err());
        assert!(validate_rev("--all").is_err());
        assert!(validate_rev("abc 123").is_err());
    }

    #[test]
    fn branches_parse_current_and_upstream() {
        let out = "main\t*\torigin/main\nfeature\t\t\norigin/main\t\t\norigin/feature\t\t\n";
        let items = parse_branches(out);
        assert_eq!(items.len(), 4);
        assert!(items[0].current);
        assert_eq!(items[0].upstream.as_deref(), Some("origin/main"));
        assert!(!items[1].current);
        assert_eq!(items[1].upstream, None);
    }

    #[test]
    fn stash_parse_index_and_message() {
        let out = "stash@{0}\tWIP on main\nstash@{1}\ttest change\n";
        let items = parse_stash_list(out);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].index, 0);
        assert_eq!(items[0].message, "WIP on main");
        assert_eq!(items[1].index, 1);
    }

    #[test]
    fn blame_parses_lines() {
        // porcelain v1 格式：每个代码行前有 `<40hex> <orig> <final> [<num>]` header；
        // meta（author 等）只在每组第一行 header 后出现一次，组内后续行共享。
        let out = "\
abc1234567890abcdef1111111111111111111111 1 1 2
author Alice
author-mail <a@b.c>
author-time 1754000000
author-tz +0800
committer Alice
committer-mail <a@b.c>
committer-time 1754000000
committer-tz +0800
summary Initial
filename path/a.rs
\tInitial commit
abc1234567890abcdef1111111111111111111111 2 2
\tHello world
def4567890abcdef2222222222222222222222222 3 3 1
author Bob
author-mail <b@c.d>
author-time 1754100000
author-tz +0800
committer Bob
committer-mail <b@c.d>
committer-time 1754100000
committer-tz +0800
summary Second
filename path/a.rs
\tSecond
def4567890abcdef2222222222222222222222222 4 4
\tAnother line
";
        let lines = parse_blame(out);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].short, "abc1234");
        assert_eq!(lines[0].author, "Alice");
        assert_eq!(lines[0].text, "Initial commit");
        assert_eq!(lines[1].line_no, 2);
        assert_eq!(lines[1].author, "Alice");
        assert_eq!(lines[1].text, "Hello world");
        assert_eq!(lines[2].line_no, 3);
        assert_eq!(lines[2].author, "Bob");
        assert_eq!(lines[2].text, "Second");
        assert_eq!(lines[3].line_no, 4);
        assert_eq!(lines[3].author, "Bob");
        assert_eq!(lines[3].text, "Another line");
    }

    #[test]
    fn blame_handles_multibyte_code_lines_without_panicking() {
        // 回归：is_commit_header 旧实现用 split_at(40) 按字节切分，中文代码行前 40 字节
        // 会落在多字节字符中间 → `str` char boundary panic（客户端表现为 Empty reply）。
        let code_line = "中文中文中文中文中文中文中文中文中文中文中文中文中文中文注释";
        let out = format!(
            "abc1234567890abcdef1111111111111111111111 1 1 1\n\
             author Alice\n\
             author-mail <a@b.c>\n\
             author-time 1754000000\n\
             author-tz +0800\n\
             committer Alice\n\
             committer-mail <a@b.c>\n\
             committer-time 1754000000\n\
             committer-tz +0800\n\
             summary Initial\n\
             filename path/a.rs\n\
             \t{code_line}\n"
        );
        let lines = parse_blame(&out);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, code_line);
    }

    #[test]
    fn blame_skips_previous_and_filename_headers() {
        // 回归：porcelain 头部的 `previous <sha> <file>` / `filename <file>` 行此前被当成
        // 代码行输出（text 混入 header），导致前端显示一串无意义的 "previous/filename"。
        let out = "\
abc1234567890abcdef1111111111111111111111 1 1 1
author Alice
author-mail <a@b.c>
author-time 1754000000
author-tz +0800
committer Alice
committer-mail <a@b.c>
committer-time 1754000000
committer-tz +0800
summary Initial
previous eeea15be79d20a78845179003fee836c4b3cb521 path/to/file.rs
filename path/to/file.rs
\thello world
";
        let lines = parse_blame(out);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].text, "hello world");
        assert!(
            lines
                .iter()
                .all(|l| !l.text.starts_with("previous ") && !l.text.starts_with("filename "))
        );
    }

    #[test]
    fn blame_parses_real_porcelain_output_of_this_repo() {
        // 真实验证：跑真实 `git blame --porcelain` 喂给解析器，断言行数与真实文件一致，
        // 且 `previous`/`filename` 头部不再混入代码文本（此前两个 bug 均由此类真实输出暴露）。
        if !git_available() {
            return;
        }
        let rel = "src/fileops/gitops/repo.rs";
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let out = std::process::Command::new("git")
            .args(["blame", "--porcelain", "--", rel])
            .current_dir(&manifest_dir)
            .output()
            .expect("git blame runs");
        assert!(
            out.status.success(),
            "git blame failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let lines = parse_blame(&text);
        let file_lines = std::fs::read_to_string(manifest_dir.join(rel))
            .unwrap()
            .lines()
            .count();
        assert!(!lines.is_empty());
        // 硬性正确性：解析出的行数必须等于真实文件行数。porcelain 头部行（author/
        // committer/summary/previous/filename 等）若泄漏为代码行，行数必然超过文件行数。
        assert_eq!(
            lines.len(),
            file_lines,
            "parsed blame lines must equal real file lines"
        );
        // meta 只在组第一行出现，组内行共享；author 为空说明组内归属被错误清空。
        assert!(
            lines.iter().all(|l| !l.author.is_empty()),
            "every blame line must carry an author (group-shared meta)"
        );
    }

    #[test]
    fn hunk_header_omitted_counts() {
        let h = parse_hunk_header("@@ -1 +1 @@ main");
        assert!(h.is_some());
        let (os, ol, ns, nl, _) = h.unwrap();
        assert_eq!((os, ol, ns, nl), (1, 1, 1, 1));
    }

    #[tokio::test]
    async fn discover_finds_repo_within_workspace() {
        if !git_available() {
            return;
        }
        let root = setup_repo("discover");
        run_git_proc(&root, &["init", "-q", "-b", "main"]);
        let ws_root = root.clone();
        let backend = CliGitBackend::new();
        let repos = backend.discover_repos(&ws_root, &[]).await.expect("discover");
        assert!(
            repos.iter().any(|r| r.root == ws_root.canonicalize().unwrap()),
            "should find ws root repo"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn reset_hard_previews_lost_changes() {
        if !git_available() {
            return;
        }
        let root = setup_repo("reset-preview");
        run_git_proc(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "v1\n").unwrap();
        run_git_proc(&root, &["add", "a.txt"]);
        run_git_proc(
            &root,
            &["-c", "user.name=test", "-c", "user.email=t@t", "commit", "-q", "-m", "init"],
        );
        std::fs::write(root.join("a.txt"), "v2\n").unwrap();
        std::fs::write(root.join("b.txt"), "new\n").unwrap();

        let repo = GitRepo {
            id: "r".into(),
            name: "r".into(),
            root: root.clone(),
            is_nested: false,
        };
        let backend = CliGitBackend::new();
        let view = backend.status(&repo).await.expect("status");
        assert!(!view.staged.is_empty() || !view.unstaged.is_empty());
        let preview = backend
            .reset(&repo, GitResetMode::Hard, None)
            .await
            .expect("reset");
        assert!(
            preview.lost.iter().any(|p| p == "a.txt"),
            "a.txt should be listed as lost: {:?}",
            preview.lost
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn stage_commit_roundtrip() {
        if !git_available() {
            return;
        }
        let root = setup_repo("roundtrip");
        run_git_proc(&root, &["init", "-q", "-b", "main"]);
        run_git_proc(
            &root,
            &["-c", "user.name=test", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "init"],
        );
        std::fs::write(root.join("x.txt"), "hello\n").unwrap();
        let repo = GitRepo {
            id: "r".into(),
            name: "r".into(),
            root: root.clone(),
            is_nested: false,
        };
        let backend = CliGitBackend::new();
        backend
            .stage(&repo, &["x.txt".into()], false)
            .await
            .expect("stage");
        backend.commit(&repo, "feat: add x.txt").await.expect("commit");
        let logs = backend.log(&repo, 5, 0).await.expect("log");
        assert_eq!(logs[0].subject, "feat: add x.txt");
        std::fs::remove_dir_all(&root).ok();
    }
}
