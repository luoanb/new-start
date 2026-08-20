# Technical Plan / 技术方案: Git Support

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-20_22-07_git-support/requirements.md`
- 对应设计文档：`docs/sdd-lab/2026-08-20_22-07_git-support/visual-design.md`
- 需求确认状态：已确认（2026-08-20，Q1–Q6 全部关闭）
- 本方案覆盖范围：Rust 侧 git backend 层（稳定接口）+ AI 原生 git 工具 + 写操作确认服务 + Tauri commands / RPC / 事件 + 前端 GitPanel / GitDiff / 文件树状态徽标

## Current Project Facts / 当前项目事实

- **AI 工具系统**：
  - `core/tool_registry.rs`：`Tool` trait（`name` / `description` / `parameters` / `async execute(args) -> AppResult<String>`）；`register_core` / `register` / `register_tagged`；native 注册走 `InsertCatalog::require(&name)` 门禁（缺 `inserts/<name>.md` 即 panic）；`ToolTag {Normal, System, Core}`、`ToolSource {Native, Config, Mcp}`。
  - `core/gateway.rs` `assemble_local_tools`（L1194）：`register_core(ExecuteCommandTool)` + `register_core(GetCurrentTimeTool)` + `register_file_tools(&mut registry, file_ctx)`（L1202）+ 读 `dynamic_tools.json` 注册 Http/Command 工具。git 工具在此追加注册。
  - `fileops/fs_tools.rs`：`FileToolContext`（`WorkspaceStore` + `FileSystem`）+ `file_tool!` 宏批量声明工具（模板参考）。
  - `core/cmd_exec.rs`：护栏范式（denylist / `MAX_CONCURRENT=4` / `DEFAULT_TIMEOUT_MS=30s` / `MAX_TIMEOUT_MS=120s` / `MAX_OUTPUT_CHARS=64*1024` / 截断 `truncate_output` / 日志脱敏）。
- **文件域（git 复用的边界与护栏）**：
  - `fileops/workspace.rs`：`WorkspaceStore`（`workspaces.json` 持久化、`active()`、`get(id)`）；`resolve_in_workspace(root, rel)` 越界护栏（拒绝绝对路径 / `..` / 符号链接逃逸 / 前缀校验）。
  - `fileops/fs.rs`：`FileSystem` 操作层（list/read/write/...）；`Gateway::workspace_store()` / `Gateway::file_system()`（L1009/L1014）供命令与工具共用。
  - `default_ignore()` = `[".git","node_modules","target","dist",".pulsar",".DS_Store"]`。
- **Tauri commands**：`lib.rs` `generate_handler![...]`（L1205）集中注册，workspace/fs 命令在 L1266 起；写操作成功经 `state_emit.inner()(StateChange::Workspaces)` 广播。`net/rpc.rs` `match cmd`（L303）逐命令分支（L758 起），远程模式同接口。
- **事件**：`core/events.rs` `StateChange`（`#[serde(rename_all="snake_case", tag="kind")]`）：Topics / Conversations / MessageDelta / Poller / Sessions / Neurons / Tools / Providers / Workspaces；`STATE_CHANGED_EVENT = "app://state-changed"`；`StateEmitter = Arc<dyn Fn(StateChange)+Send+Sync>`。
- **前端**：
  - `api/index.ts`：`api.invoke<T>(cmd, params)`（tauriClient / httpClient 双实现，RPC 自动转发）；`api/types.ts` `ApiClient` 接口 + `StateChangePayload`。
  - `layout/layoutTypes.ts`：`MainPanelType` 已含 `"file-editor"`（实例 key 机制，`git-diff` 复用同一多实例语义）；`DEFAULT_LAYOUT.containers.sidebar.views = ["sessions","files","topics","tools"]`。
  - `layout/views.ts`：`viewRegistry`（可移动视图）/ `mainViews`（main 区面板）/ `mainPanelMeta`（tab 图标/文案）；`LayoutStore.insertPanel` 支持实例 key。
  - `components/FileExplorer.svelte`：sidebar「files」视图（工作区下拉 + 懒加载树 + ContextMenu + ConfirmDialog）；条目行右侧已有 ⋮ 按钮，状态徽标插槽可扩展。
  - `components/` 已有：`ConfirmDialog.svelte` / `ContextMenu.svelte` / `Select.svelte` 可复用。
  - `stores/dataStore.svelte.ts`：模块级 `$state` 单例，`bootstrap()` 全量拉取 + `subscribe()` 按 kind 增量刷新。
- **依赖现状**：Cargo 无 gix / libgit2 / git2；`tokio::process::Command` 已有（cmd_exec 使用）。前端无 diff 渲染组件。

## Open Questions / 开放问题

- [x] Q1 diff 视图深度（已关闭）：UI 一期包含行内 diff 视图（unified 渲染 + hunk 导航），见 visual-design §3。
- [x] Q2 repo 边界（已关闭）：repo 发现仅向 workspace 内扫描（自 workspace 根向内），禁止外查；支持多仓库，UI 提供仓库切换器。
- [x] Q3 危险写分级（已关闭）：push 不归高危默认关；高危默认关 = reset / clean / checkout 丢弃改动；push/pull 为常规写，执行前经确认。
- [x] Q4 确认交互（已关闭）：确认弹窗仅 GUI；Rust 侧确认接口按稳定接口设计（`GitConfirmService` + 确认事件），兼容未来 TUI 接入。
- [x] Q5 工具粒度（已关闭）：`git_diff` 独立工具（`cached` 参数区分 staged/unstaged）；UI 展示与 commit 确认均基于其输出。
- [x] Q6 submodule / LFS（已关闭）：submodule 不处理；LFS 保留识别与展示（依赖系统 git-lfs，未安装时 diff 显示指针）。

## Solution Options / 方案候选

### Option A / 方案 A：gitops 独立模块 + CliGitBackend + 确认服务（推荐）

- 推荐：是
- 方案摘要：
  - 新增 `fileops/gitops/` 模块：`mod.rs`（`GitBackend` trait + `GitRepo` + 数据结构）、`repo.rs`（`CliGitBackend`：`git -C` 参数数组执行 + repo 向内发现）、`tools.rs`（15 个 AI 原生 git 工具）、`confirm.rs`（`GitConfirmService` 写操作确认）。
  - 工具分级：只读 6 个 `register_core`；写 9 个 `register`（Normal）+ 确认服务；高危写受配置开关 `git.dangerous_writes`（默认 false）控制。
  - UI：sidebar 新增「git」视图 `GitPanel.svelte` + main 区「git-diff」面板 `GitDiff.svelte`（按文件路径多实例）+ `FileExplorer` 状态徽标集成。
  - 事件：`StateChange::Git`（数据刷新）+ `StateChange::GitConfirm`（确认请求）。
- 涉及模块：`fileops/gitops/*`、`core/gateway.rs`、`core/events.rs`、`lib.rs`、`net/rpc.rs`、`inserts/*.md`（15 个）、`views.ts`、`layoutTypes.ts`、`dataStore.svelte.ts`、`GitPanel.svelte`、`GitDiff.svelte`、`FileExplorer.svelte`、`ConfirmDialog.svelte`、`translations.ts`、`api/types.ts`、`types.ts`
- 优点：Rust 侧稳定接口（GitBackend trait）一次成型，GUI/TUI/远程三消费方共用；行为与用户命令行 git 一致；零新增 Rust 依赖；沿用 fileops 护栏模式
- 缺点：CLI 输出解析需自行实现（porcelain / unified diff / blame）；写操作确认链路需新增（当前项目无确认服务）
- 风险：中。命令安全（防注入/越界）与确认链路为关键点，需充分单测

### Option B / 方案 B：gix（gitoxide）纯 Rust 实现

- 推荐：否
- 方案摘要：`GitBackend` trait 不变，实现换成 `gix::Repository`；不依赖系统 git。
- 涉及模块：同上，仅 backend 实现不同 + `Cargo.toml` 增 `gix` 依赖
- 优点：纯 Rust、无系统 git 依赖、未来可跑隔离环境
- 缺点：重依赖（编译时长↑）；行为细节与用户命令行 git 有偏差（用户以命令行 git 为真相，偏差造成困惑）；部分能力未实现（stash 等 edge case）；diff/blame 文本化仍需自写
- 风险：中高。行为一致性风险 + 编译/体积成本

### Option C / 方案 C：仅扩展动态 CommandTool（不新增 native 工具/UI）

- 推荐：否
- 方案摘要：只在 `dynamic_tools.json` 预置若干 git 命令模板（如 `git status --porcelain`）。
- 优点：改动最小
- 缺点：无结构化结果、无确认服务、无 UI，不满足需求（AI 工具 + UI 一期）
- 风险：不满足需求

## Decision / 方案决策

- Selected / 选定方案：**Option A**（需求讨论已确认技术路线倾向 spawn git CLI；选项对比见上）
- Why / 选择原因：行为一致性（用户 git 为唯一真相）、零新依赖、`GitBackend` trait 稳定接口覆盖 GUI/TUI/远程三消费方；社区共识与行业工具（VS Code / lazygit）同样基于 git CLI 语义
- Decision Owner / 决策人：用户（技术路线已确认，方案对比待用户最终确认）
- Decision Time / 决策时间：2026-08-20
- Open Questions 状态：Q1–Q6 全部关闭（用户已确认）

## API Design / API 设计

> 本交付新增对外契约（tauri commands + StateChange 变体 + AI 工具 + TS 类型），需要固化。

### Contract Scope / 契约范围

- 变更类型：新增（不破坏现有命令/事件/工具契约）
- 消费方：前端 GitPanel / GitDiff / FileExplorer、Agent 会话（AI 工具）、远程模式 RPC、未来 TUI（预留）
- 真相源文件：`fileops/gitops/mod.rs`、`lib.rs`、`net/rpc.rs`、`core/events.rs`、`api/types.ts`、`types.ts`、`views.ts`、`layoutTypes.ts`

### Rust 侧数据结构（Rust ↔ TS 一致）

```rust
// fileops/gitops/mod.rs
/// 发现的仓库（根在 workspace 内，canonicalize 校验）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRepo {
    pub id: String,        // 稳定 id（canonicalized root 派生，同 workspace.rs id 策略）
    pub name: String,      // 展示名（repo 根目录名）
    pub root: PathBuf,     // repo 绝对根
    pub is_nested: bool,   // 是否为嵌套 repo（位于其他 repo 内）
}

/// git status 视图（解析 porcelain v1 + 目录聚合由前端完成）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusView {
    pub branch: Option<String>,      // 当前分支（detached 时为 None，附 head: 摘要）
    pub ahead: i64,                  // 领先远端提交数
    pub behind: i64,                 // 落后远端提交数
    pub staged: Vec<GitStatusEntry>,     // 已暂存
    pub unstaged: Vec<GitStatusEntry>,   // 未暂存（工作区）
    pub untracked: Vec<GitStatusEntry>,  // 未跟踪
    pub conflicted: Vec<GitStatusEntry>, // 冲突（U）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusEntry {
    pub path: String,     // 相对 repo 根
    pub status: String,   // M/A/D/R/?/U 等（单字母或组合，如 "MM"）
    pub is_dir: bool,     // 前端目录聚合用
}

/// diff 视图（解析 unified diff）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiff {
    pub files: Vec<GitFileDiff>,
    pub truncated: bool,          // 输出超限被截断
}
pub struct GitFileDiff {
    pub path: String,
    pub status: String,           // M/A/D/R/? 
    pub is_binary: bool,          // LFS 指针 / 二进制 → 前端显示提示不渲染
    pub hunks: Vec<GitHunk>,
}
pub struct GitHunk {
    pub old_start: usize, pub old_lines: usize,
    pub new_start: usize, pub new_lines: usize,
    pub header: String,           // "@@ -a,b +c,d @@ ctx"
    pub lines: Vec<GitDiffLine>,
}
pub struct GitDiffLine {
    pub kind: "context" | "add" | "del",
    pub old_no: Option<usize>, pub new_no: Option<usize>,
    pub text: String,
}

/// log 条目（解析 `git log --format=...`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitInfo {
    pub hash: String,        // 完整 hash
    pub short: String,       // 7 位短 hash
    pub author: String,
    pub date: String,        // 相对/ISO 时间
    pub subject: String,
}

/// blame 行（`git blame --porcelain` 解析，行维度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBlameLine {
    pub line_no: usize,
    pub short: String,       // commit 短 hash
    pub author: String,
    pub date: String,
    pub text: String,
}

/// stash 条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStashEntry {
    pub index: usize,        // stash@{n}
    pub message: String,
}
```

```ts
// src/lib/types.ts（与上方 Rust 结构一一对应）
export interface GitRepo { id: string; name: string; root: string; is_nested: boolean }
export interface GitStatusView { branch: string | null; ahead: number; behind: number;
  staged: GitStatusEntry[]; unstaged: GitStatusEntry[]; untracked: GitStatusEntry[]; conflicted: GitStatusEntry[] }
export interface GitStatusEntry { path: string; status: string; is_dir: boolean }
export interface GitDiff { files: GitFileDiff[]; truncated: boolean }
export interface GitFileDiff { path: string; status: string; is_binary: boolean; hunks: GitHunk[] }
export interface GitHunk { old_start: number; old_lines: number; new_start: number; new_lines: number; header: string; lines: GitDiffLine[] }
export interface GitDiffLine { kind: "context" | "add" | "del"; old_no: number | null; new_no: number | null; text: string }
export interface GitCommitInfo { hash: string; short: string; author: string; date: string; subject: string }
export interface GitBlameLine { line_no: number; short: string; author: string; date: string; text: string }
export interface GitStashEntry { index: number; message: string }
```

### GitBackend trait（稳定接口，GUI / TUI / 远程共用）

```rust
/// 稳定接口：git 操作抽象。第一版唯一实现 `CliGitBackend`（spawn git CLI）；
/// 未来 TUI 消费同一接口；远程模式由 RPC 层转发同名命令。
#[async_trait]
pub trait GitBackend: Send + Sync {
    /// 发现 workspace 内所有 repo（仅向内扫描，禁止外查）。
    async fn discover_repos(&self, ws_root: &Path, ignore: &[String]) -> AppResult<Vec<GitRepo>>;

    // ── 只读 ──
    async fn status(&self, repo: &GitRepo) -> AppResult<GitStatusView>;
    async fn diff(&self, repo: &GitRepo, cached: bool, path: Option<&str>) -> AppResult<GitDiff>;
    async fn log(&self, repo: &GitRepo, limit: usize) -> AppResult<Vec<GitCommitInfo>>;
    async fn branches(&self, repo: &GitRepo) -> AppResult<Vec<GitBranchItem>>;
    async fn blame(&self, repo: &GitRepo, path: &str) -> AppResult<Vec<GitBlameLine>>;
    async fn stash_list(&self, repo: &GitRepo) -> AppResult<Vec<GitStashEntry>>;

    // ── 写操作（调用方负责确认门禁）──
    async fn stage(&self, repo: &GitRepo, paths: &[String], all: bool) -> AppResult<()>;
    async fn unstage(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()>;
    async fn restore(&self, repo: &GitRepo, paths: &[String]) -> AppResult<()>;
    async fn commit(&self, repo: &GitRepo, message: &str) -> AppResult<()>;
    /// 高危写：先 dry-run 计算将丢失改动清单（preview），再执行。
    async fn reset(&self, repo: &GitRepo, mode: GitResetMode, target: Option<&str>) -> AppResult<GitResetPreview>;
    /// 高危写：checkout 分支/路径；丢弃工作区改动需确认。
    async fn checkout(&self, repo: &GitRepo, target: &str) -> AppResult<()>;
    async fn stash(&self, repo: &GitRepo, action: GitStashAction, message: Option<&str>) -> AppResult<()>;
    async fn push(&self, repo: &GitRepo, remote: Option<&str>, branch: Option<&str>) -> AppResult<()>;
    async fn pull(&self, repo: &GitRepo) -> AppResult<()>;
    /// 冲突解决：ours / theirs / both。
    async fn resolve_conflict(&self, repo: &GitRepo, path: &str, take: ConflictTake) -> AppResult<()>;
}

pub enum GitResetMode { Mixed, Soft, Hard, Keep }
pub struct GitResetPreview { pub lost: Vec<String> }   // 将丢失改动文件清单（hard 场景）
pub enum GitStashAction { Push, Pop, Drop, Apply }
pub enum ConflictTake { Ours, Theirs, Both }
pub struct GitBranchItem { pub name: String, pub current: bool, pub upstream: Option<String> }
```

实现要点（`CliGitBackend`）：

- 所有命令经 `tokio::process::Command::new("git").args(["-C", repo_root])` 参数数组执行，**不经 shell**（防注入）；`-C` 根为 `GitRepo.root`（本身已通过 workspace 前缀校验）。
- 复用 `cmd_exec` 常量：超时 `DEFAULT_TIMEOUT_MS`、并发 `Semaphore::new(MAX_CONCURRENT)`、输出截断 `truncate_output`（diff/log 超限标记 `truncated`，diff 按文件/hunk 上限截断）。
- 路径参数（`-- path`）一律加 `--` 分隔符；commit message 经 args 传入（不用 `-m` 拼接风险场景时也走参数数组）。
- `resolve_conflict`：Ours/Theirs 用 `git checkout --ours/--theirs -- <path>` 后 `git add`；Both 读 stage 2/3 内容拼接后写文件 + `git add`（冲突块头 `<<<<<<<` 处理由前端交互决定）。
- LFS 检测：`.gitattributes` 含 `filter=lfs` 或文件头为 `version https://git-lfs` → `is_binary=true`，diff 显示指针提示（依赖系统 git-lfs，缺失降级）。

### repo 发现（repo.rs）

- 仅向 workspace 内扫描：自 `ws_root` 开始 BFS，跳过 `ignore` 匹配目录（含 `node_modules` / `.pulsar` / `target` / `dist`）与已发现的 repo 的 `.git` 内部；深度上限 8、repo 数量上限 50。
- 候选目录含 `.git`（目录或 gitfile 文件）→ 执行 `git -C <cand> rev-parse --show-toplevel` 得到真实 repo 根；canonicalize 后校验 `starts_with(ws_root_canonical)`，越界即拒绝（防御嵌套逃逸）。
- 嵌套 repo（B 在 A 内）都保留，`is_nested=true`；去重按 repo 根。
- `GitRepo.id` 由 canonicalized root 哈希派生（同 workspace `id_for_root` 策略）。

### 写操作确认服务（confirm.rs，稳定接口预留 TUI）

```rust
/// 写操作确认：任何需用户确认的写操作先入 pending 队列并广播确认请求，
/// 等待 `git_confirm { op_id, approved }`；超时（60s）自动作废。
/// GUI 为当前唯一消费方；接口形状与事件即未来 TUI 的接入点。
pub struct GitConfirmService {
    pending: RwLock<HashMap<String, PendingGitOp>>,
    timeout: Duration,
}
pub struct PendingGitOp {
    pub op_id: String,
    pub kind: GitOpKind,          // Commit | Push | Pull | Reset | Checkout | StashApply | StashDrop | Clean
    pub title: String,            // 确认弹窗标题
    pub detail: Value,            // 确认弹窗详情（如 reset dry-run 清单 / staged diff 摘要）
    pub created_ms: i64,
}
pub enum GitOpKind { Commit, Push, Pull, Reset, Checkout, StashApply, StashDrop, Clean }
```

- 流程：`git_commit { message }`（或 reset/push/...）→ 服务生成 `op_id` + 预演（commit 生成 staged diff 摘要；reset 生成 `GitResetPreview`）→ 广播 `StateChange::GitConfirm { op_id, kind, title, detail }` → 前端 ConfirmDialog → `git_confirm { op_id, approved }` → 通过则执行 `CliGitBackend` 对应写方法 → 广播 `StateChange::Git`；拒绝则丢弃；超时作废返回错误。
- 高危写（reset / clean / checkout 丢弃改动）额外受配置开关 `git.dangerous_writes`（`config.json` 顶层 `git` 节，默认 `false`）控制：开关关 → 直接拒绝（`InvalidInput`），开关开 → 仍走确认服务。
- AI 工具与 UI 走同一确认链路（工具调用写操作同样产生确认事件；TUI 未来复用 `StateChange::GitConfirm`）。

### Tauri commands（lib.rs + net/rpc.rs 同步注册）

| command | params | returns | 说明 |
|---|---|---|---|
| `git_repos` | — | `Vec<GitRepo>` | 发现 active workspace 内全部 repo（缓存于 Gateway） |
| `git_status` | — | `GitStatusView` | 当前 repo（`active_repo_id` 由前端经 `git_set_active_repo` 指定） |
| `git_diff` | `{ path?: string, cached?: bool }` | `GitDiff` | unified diff；`cached` 区分 staged/unstaged |
| `git_log` | `{ limit?: number }` | `Vec<GitCommitInfo>` | 默认 30 |
| `git_branches` | — | `Vec<GitBranchItem>` | 本地 + 远端 |
| `git_blame` | `{ path: string }` | `Vec<GitBlameLine>` | 行级 |
| `git_stash_list` | — | `Vec<GitStashEntry>` | |
| `git_set_active_repo` | `{ repo_id: string }` | `()` | 切换当前操作仓库（会话内存态） |
| `git_add` | `{ paths?: string[], all?: boolean }` | `()` | 暂存；广播 Git |
| `git_restore` | `{ paths: string[] }` | `()` | 撤销工作区改动；确认；广播 Git |
| `git_commit` | `{ message: string }` | `()` | 经确认服务；广播 Git |
| `git_reset` | `{ mode: string, target?: string }` | `GitResetPreview` | 高危（开关 + 确认）；广播 Git |
| `git_checkout` | `{ target: string }` | `()` | 分支/路径切换；丢弃改动场景确认；广播 Git |
| `git_stash` | `{ action: string, message?: string }` | `()` | push/pop/drop/apply；pop/drop 确认；广播 Git |
| `git_push` | `{ remote?: string, branch?: string }` | `()` | 确认；广播 Git |
| `git_pull` | — | `()` | 确认；广播 Git |
| `git_resolve_conflict` | `{ path: string, take: string }` | `()` | ours/theirs/both；广播 Git |
| `git_confirm` | `{ op_id: string, approved: boolean }` | `()` | 确认服务的唯一入口 |
| `git_get_confirm_config` | — | `{ dangerous_writes: boolean }` | 前端开关回显 |
| `git_set_dangerous_writes` | `{ enabled: boolean }` | `{ dangerous_writes: boolean }` | 持久化 `git.dangerous_writes` 到 config.json 并热更新内存开关；广播 Git |

> `git.dangerous_writes` 开关：`config.json` 顶层新增 `git: { dangerous_writes: bool }` 节（`ConfigStore` 扩展，`GitSection` 类型化承载，`extra` 兜底无损保留其余未建模键），`save_*` 沿用现有配置保存流程（读改写 + 原子写回）。

### StateChange 扩展（core/events.rs + api/types.ts）

```rust
// core/events.rs 新增
Git,                                   // git 数据变化（status/diff/branch/stash）→ 前端刷新 git 面板与文件树徽标
GitConfirm {                           // 写操作确认请求（GUI 弹窗；TUI 未来消费同一事件）
    op_id: String,
    kind: String,                      // Commit / Push / Pull / Reset / Checkout / StashApply / StashDrop / Clean
    title: String,
    detail: Value,
},
```

```ts
// api/types.ts StateChangePayload 新增
| { kind: "git" }
| { kind: "git_confirm"; op_id: string; kind: string; title: string; detail: unknown }
```

### AI 原生 git 工具（15 个，inserts 门禁齐备）

| 工具 | tag | 参数要点 | 返回 | 关键语义 |
|---|---|---|---|---|
| `git_status` | Core | — | `GitStatusView` | 当前 repo |
| `git_diff` | Core | `path?` / `cached?` | `GitDiff` | 只读 |
| `git_log` | Core | `limit?` | `Vec<GitCommitInfo>` | 只读 |
| `git_branch` | Core | — | `Vec<GitBranchItem>` | 只读 |
| `git_blame` | Core | `path` | `Vec<GitBlameLine>` | 只读 |
| `git_stash_list` | Core | — | `Vec<GitStashEntry>` | 只读 |
| `git_add` | Normal | `paths?` / `all?` | `()` | 写；广播 |
| `git_restore` | Normal | `paths` | `()` | 写；确认 |
| `git_commit` | Normal | `message` | `()` | 写；确认（含 staged diff 摘要） |
| `git_reset` | Normal | `mode` / `target?` | `GitResetPreview` | 高危：开关 + 确认 |
| `git_checkout` | Normal | `target` | `()` | 写；丢弃场景确认 |
| `git_stash` | Normal | `action` / `message?` | `()` | 写；pop/drop 确认 |
| `git_push` | Normal | `remote?` / `branch?` | `()` | 写；确认 |
| `git_pull` | Normal | — | `()` | 写；确认 |
| `git_resolve_conflict` | Normal | `path` / `take` | `()` | 冲突解决；写 |

> 工具统一以「当前 active workspace 内、`git_set_active_repo` 指定的 repo」为作用域；路径参数经 repo 根前缀校验（复用 `resolve_in_workspace`，repo 根须在 workspace 内）。

### Compatibility Notes / 兼容说明

- 现有命令/事件/工具全部保留，纯增量。
- `StateChange` 新增变体仅影响前端 `StateChangePayload` 联合类型（增量 union），不破坏既有消费。
- `MainPanelType` 新增 `"git-diff"`（按文件路径多实例，复用 `file-editor` 的 instance key 语义）；`viewRegistry` 新增 `git`（sidebar 可移动，`movableTo: "*"`）。
- 远程模式：git 命令在 `net/rpc.rs` 逐一注册分支；确认事件经现有 SSE 广播通道转发（`events_tx`），前端 `git_confirm` 经 RPC 返回。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本方案（Option A）+ Q1–Q6 已确认，用户批准技术方案后进入执行
- 若执行前需求、API、范围或交互规则变化：先回写 requirements.md / visual-design.md / technical-plan.md

### Step 1. gitops 模块：backend + repo 发现

#### 文件：`src-tauri/src/fileops/gitops/mod.rs`（新增）

- 改动类型：新增
- 改动内容：`GitRepo` / `GitStatusView` / `GitStatusEntry` / `GitDiff` / `GitFileDiff` / `GitHunk` / `GitDiffLine` / `GitCommitInfo` / `GitBlameLine` / `GitStashEntry` / `GitBranchItem` 数据结构；`GitBackend` trait；`GitResetMode` / `GitStashAction` / `ConflictTake`；`GitResetPreview`
- 设计约束：
  - API：遵循上方契约（Rust ↔ TS 一致）
  - 状态：纯数据结构 + trait，无状态
- 验收点：serde 序列化往返测试；trait 方法签名可被 mock 实现

#### 文件：`src-tauri/src/fileops/gitops/repo.rs`（新增）

- 改动类型：新增
- 改动内容：`CliGitBackend`（spawn `git` CLI）+ `discover_repos`（向内扫描，深度/数量上限，`rev-parse --show-toplevel` 收敛真实 repo 根 + workspace 前缀校验）；写方法实现（stage/commit/reset/... 全部参数数组 + `-C` + 超时/并发/截断）；porcelain/unified diff/blame/log 解析
- 设计约束：
  - API：`GitBackend` trait 实现；复用 `cmd_exec` 常量与 `Semaphore`
  - 状态：无持久化
- 验收点：单测覆盖 status/diff/log/branch/blame/stash 解析；reset dry-run preview；越界拒绝（repo 根逃逸）；命令注入防御（参数含 `-D`/`--` 边界）

#### 文件：`src-tauri/src/fileops/mod.rs`

- 改动类型：修改
- 改动内容：`pub mod gitops;`
- 验收点：编译通过

### Step 2. 确认服务 + AI 工具 + 装配

#### 文件：`src-tauri/src/fileops/gitops/confirm.rs`（新增）

- 改动类型：新增
- 改动内容：`GitConfirmService`（pending 队列 + 超时作废 + `request`/`resolve`）；`GitOpKind`
- 设计约束：稳定接口（GUI 当前消费，TUI 预留）；事件广播经注入的 `StateEmitter`
- 验收点：单测覆盖 request/resolve/超时/拒绝

#### 文件：`src-tauri/src/fileops/gitops/tools.rs`（新增）

- 改动类型：新增
- 改动内容：15 个 `Tool` trait 实现（复用 `GitToolContext { backend, store, confirm }`）；只读 6 个 `register_core`、写 9 个 `register`（写经确认服务 / 高危受开关）；`register_git_tools` 入口
- 设计约束：native 来源 + `inserts/<name>.md` 门禁；作用域 = active workspace + active repo
- 验收点：`list_tools` 可见 15 工具；schema 合法；写工具确认链路正确

#### 文件：`src-tauri/inserts/*.md`（新增 15 个）

- 改动类型：新增
- 改动内容：`git_status.md` / `git_diff.md` / `git_log.md` / `git_branch.md` / `git_blame.md` / `git_stash_list.md` / `git_add.md` / `git_restore.md` / `git_commit.md` / `git_reset.md` / `git_checkout.md` / `git_stash.md` / `git_push.md` / `git_pull.md` / `git_resolve_conflict.md`（`## 工具` 段首行一句话 + 写工具注明确认/危险开关）
- 验收点：`list_insert_catalog` 可见；`register` 不 panic

#### 文件：`src-tauri/src/core/gateway.rs`

- 改动类型：修改
- 改动内容：`Gateway` 持有 `git_service: Arc<GitService>`（`GitBackend` + `GitConfirmService` + active_repo 内存态 + `dangerous_writes` 开关读取）；`assemble_local_tools` 中 `register_git_tools(&mut registry, git_ctx)`；暴露 `git_service()`；`git.dangerous_writes` 从 `ConfigStore` 读取
- 设计约束：`assemble_local_tools` 保持无阻塞；状态与工具共享同一 `GitService`
- 验收点：`cargo test --lib` 全绿；启动装配无 panic

### Step 3. Tauri commands + RPC + 事件

#### 文件：`src-tauri/src/core/events.rs`

- 改动类型：修改
- 改动内容：`StateChange::Git` / `GitConfirm { op_id, kind, title, detail }` 变体 + serde 测试
- 验收点：serde 序列化测试（snake_case / tag）

#### 文件：`src-tauri/src/lib.rs`

- 改动类型：修改
- 改动内容：19 个 git commands 注册进 `generate_handler!`；写命令成功后 `StateChange::Git` 广播；`git_confirm` 命令
- 验收点：`cargo check` 通过；命令可被前端 invoke

#### 文件：`src-tauri/src/net/rpc.rs`

- 改动类型：修改
- 改动内容：19 个 cmd 分支（与 lib.rs 相同业务语义，写操作后广播 Git）
- 验收点：RPC 冒烟测试 / 远程模式手动验证

#### 文件：`src-tauri/src/core/config.rs`（或 config 结构）

- 改动类型：修改
- 改动内容：`config.json` 顶层 `git` 节（`dangerous_writes`，默认 false）+ 读写
- 验收点：默认值 / 保存往返

### Step 4. 前端：类型、事件、布局注册

#### 文件：`src/lib/api/types.ts`

- 改动类型：修改
- 改动内容：`StateChangePayload` 增 `{kind:"git"}` 与 `{kind:"git_confirm", op_id, kind, title, detail}`
- 验收点：类型检查通过

#### 文件：`src/lib/types.ts`

- 改动类型：修改
- 改动内容：`GitRepo` / `GitStatusView` / `GitDiff` 等 TS 镜像类型
- 验收点：类型检查通过

#### 文件：`src/lib/layout/layoutTypes.ts`

- 改动类型：修改
- 改动内容：`MainPanelType` 增 `"git-diff"`（复用 file-editor 实例 key 语义）；`DEFAULT_LAYOUT.containers.sidebar.views` 增 `"git"`
- 验收点：类型检查通过

#### 文件：`src/lib/layout/views.ts`

- 改动类型：修改
- 改动内容：`viewRegistry["git"]`（GitPanel，movableTo `"*"`）+ `mainViews` 增 `git-diff`（GitDiff）+ `mainPanelMeta["git-diff"]`（icon/label）
- 验收点：sidebar 显示 git tab；main 区可插入 git-diff 多实例

#### 文件：`src/lib/stores/dataStore.svelte.ts`

- 改动类型：修改
- 改动内容：`state.git`（repos/activeRepoId/status/diff 缓存）+ `gitConfirmQueue`（确认请求队列）；bootstrap 拉取 `git_repos`；`handleStateChanged` 增 `git` 与 `git_confirm` 分支；actions（loadStatus / loadDiff / setActiveRepo / confirmOp / 各写操作）
- 验收点：git 状态随事件刷新；确认队列弹窗驱动

#### 文件：`src/lib/i18n/translations.ts`

- 改动类型：修改
- 改动内容：zh/en 三处补 `views.git` / `views.gitDiff` / `gitPanel.*` / `gitDiff.*` / `gitConfirm.*` 键
- 验收点：`pnpm check` 0 error

### Step 5. 前端：GitPanel / GitDiff / 文件树徽标

#### 文件：`src/lib/components/GitPanel.svelte`（新增）

- 改动类型：新增
- 改动内容：作用域工具条（仓库下拉 + 分支下拉 + ⋯ 菜单）；变更汇总 + 刷新；暂存区/更改分组列表（勾选 + 状态徽标 + 暂存/取消暂存）；commit 输入区 + 提交按钮（经确认服务）；分支区段；Stash 区段；确认弹窗（复用 ConfirmDialog，绑定 gitConfirmQueue）
- 设计约束：遵循 visual-design §1/§4/§5；API `api.invoke` git 命令 + dataStore
- 验收点：仓库/分支切换、staging 勾选、commit（含 diff 摘要确认）、stash 创建/应用/删除、危险操作开关与确认、触屏可点

#### 文件：`src/lib/components/GitDiff.svelte`（新增）

- 改动类型：新增
- 改动内容：unified diff 渲染（hunk 头/增删行/行号/前后 hunk 导航）；staged/unstaged 范围切换；冲突 hunk 的 ours/theirs/both 操作按钮；blame 视图开关；LFS/二进制指针提示
- 设计约束：遵循 visual-design §3；`--font-mono`；多实例按文件路径
- 验收点：diff 渲染与导航、范围切换、冲突解决、blame、LFS 提示

#### 文件：`src/lib/components/FileExplorer.svelte`

- 改动类型：修改
- 改动内容：条目行右侧状态徽标区（git status 字母 + 目录聚合）；点击徽标打开 git-diff 面板；监听 `StateChange::Git` 刷新
- 设计约束：遵循 visual-design §2；不改变现有树交互
- 验收点：徽标正确显示/刷新；点击开 diff

#### 文件：`src/lib/components/ConfirmDialog.svelte`

- 改动类型：修改（可选增强）
- 改动内容：支持「将丢失改动清单 / diff 摘要」详情区渲染（若现有组件仅纯文本则扩展）
- 设计约束：不破坏现有调用方
- 验收点：现有确认弹窗回归 + git 确认详情展示

### Step 6. 检查与回写

#### 命令

- 运行：`cargo test --lib`；`cargo check --all-targets`；`pnpm check`；`pnpm build`
- 修复：按失败逐项修复，涉及契约变化先回写本方案 `API Design`

#### 文件：`docs/sdd-lab/2026-08-20_22-07_git-support/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|---|---|
| git 命令注入 / 越界逃逸 | 参数数组（不经 shell）+ `-C` 固定 repo 根 + `--` 分隔路径参数；repo 根 canonicalize 前缀校验（复用 `resolve_in_workspace`）；单测覆盖参数含 `-D`/`;`/`&&` 等用例 |
| 写操作误伤（reset/clean/checkout 丢改动） | 高危写默认开关 `git.dangerous_writes=false` + 确认服务 + reset 前 dry-run 预览；写工具 insert 文档「忌用」段说明 |
| 确认链路绕过（AI 直接调用写工具） | 写工具与 UI 走同一 `GitConfirmService`；工具侧不提供绕过确认的旁路；拒绝/超时即作废 |
| 大 diff / log / blame 输出爆量 | 复用 `MAX_OUTPUT_CHARS` 截断 + `truncated` 标记；diff 按文件/hunk 数量上限；前端懒渲染（仅可视 hunk） |
| repo 发现扫描过深/过多导致卡顿 | 深度 8 / 数量 50 上限 + ignore 目录跳过 + 缓存于 Gateway（`git_repos` 结果带版本） |
| 远程模式命令遗漏（RPC 未注册） | lib.rs 与 rpc.rs 命令清单对齐步骤化；验收含远程冒烟 |
| 前端状态刷新风暴（Git 事件频繁） | Git 事件仅写操作后广播；diff/stash 惰性拉取；dataStore 去重 |
| LFS 未安装 / 二进制 diff 渲染异常 | `is_binary` 标记 → 前端显示指针提示而非渲染；不依赖 git-lfs 完成核心功能 |
| 未来 TUI 接入破坏接口 | `GitBackend` trait + `GitConfirmService` 事件为稳定契约；GUI 实现即 TUI 复用同一后端 |

## Execute Checkpoint / 执行检查点

- 当前理解：为 pulsar-app 增加完整原生 Git 能力——Rust 侧 `gitops` 模块（GitBackend trait + CliGitBackend + 确认服务）+ 15 个 AI 原生 git 工具 + 19 个 tauri commands（RPC 同步）+ `StateChange::Git/GitConfirm` + 前端 GitPanel（sidebar）/ GitDiff（main 多实例）/ 文件树状态徽标；写操作分级护栏（高危开关 + 确认）。
- 核心目标：打通「AI 看状态 → 编辑 → UI/工具提交」闭环，同时提供完整 Git 工作台 UI（diff / conflict / blame / stash / 多仓库），Rust 侧稳定接口为 TUI 未来接入预留。
- 下一步动作：用户确认技术方案后进入 Step 1（gitops 模块）。
- 风险：命令安全（核心）、确认链路、repo 发现性能、RPC 双注册、前端状态刷新。
