# Technical Plan / 技术方案: workspace-file-management

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-16_23-45_workspace-file-management/requirements.md`
- 需求确认状态：已确认（2026-08-16，Q1–Q6 全部确认）
- 本方案覆盖范围：后端文件操作层 + 工作区存储 + AI 原生文件工具 + 前端文件树/编辑器 + 事件/RPC 接入

## Current Project Facts / 当前项目事实

- **AI 工具系统**：
  - `core/tool_registry.rs`：`Tool` trait（`name` / `description` / `parameters` / `async execute(args) -> AppResult<String>`）；`register_core` / `register_system` / `register_source`；native 注册走 `InsertCatalog::require(&name)` 门禁（缺 `inserts/<name>.md` 即 panic）；`get_tool` / `list_definitions`。
  - `core/gateway.rs` `assemble_local_tools`（L1021）：`register_core(ExecuteCommandTool)` + `register_core(GetCurrentTimeTool)` + 读 `dynamic_tools.json` 注册 Http/Command 工具。新文件工具应在此处追加注册。
  - `core/insert_catalog.rs`：`inserts/*.md` 由 rust-embed 内嵌；`## 工具` 段首行作为 `hint`。
  - `core/cmd_exec.rs`：护栏范式参照（denylist / 并发 / 超时 / 截断 / 日志脱敏）。
  - `core/models.rs`：`ToolSource {Native, Config, Mcp}`、`ToolTag {Normal, System, Core}`、`ToolDefinition`。
- **Tauri commands**：`lib.rs` `generate_handler![...]` 集中注册；`net/rpc.rs` `cmd.as_str()` 分支逐命令转发（远程模式同接口）。
- **事件**：`core/events.rs` `StateChange`（Topics / Conversations / Poller / Sessions / Neurons / Tools / Providers）；前端 `api/types.ts` `StateChangePayload` 同步维护。
- **前端**：
  - `views.ts`：`viewRegistry`（sidebar/info/panel 可移动视图）+ `mainViews`（main 区面板，`chat` / `neurons` / `tool-editor` / `provider-manager`）+ `mainPanelMeta`（tab 图标/文案）+ `activityItems`。
  - `layoutTypes.ts`：`MainPanelType` 联合类型；`DEFAULT_LAYOUT.containers.sidebar.views = ["sessions", "topics", "tools"]`。
  - `LayoutStore.svelte.ts`：`insertPanel(type, target?)`（同一类型全局唯一）/ `closePanel` / `setActivePanel`。
  - `dataStore.svelte.ts`：模块级 `$state` 单例，`bootstrap()` 全量拉取 + `subscribe()` 按 kind 增量刷新；actions 内 `api.invoke`。
  - `api/`：`ApiClient` 抽象（tauriClient / httpClient 双实现），`api.invoke<T>(cmd, params)` 统一通道。
  - i18n：`translations.ts` 三处（zh/en/type 接口）维护 `views.*` 等键。
- **存储**：`storage::resolve` → 仓库根 `.pulsar/`；已有 `config.json` / `mcp_servers.json` / `dynamic_tools.json` 并列，均为根目录下 JSON。
- **依赖现状**：Cargo 无 regex / walkdir / globset / ignore；前端无 CodeMirror。

## Open Questions / 开放问题

- [x] Q1 文件树默认过滤（ignore）规则存哪里、可配置到什么程度？→ **基于工作目录配置**（2026-08-16 已确认）
  - 每个工作区条目携带独立 ignore 列表（写入 workspaces.json），FileExplorer 提供编辑入口。
- [x] Q2 工作区目录选择交互？→ **系统对话框 + 输入回退**（2026-08-16 已确认）
  - 桌面端引入 `tauri-plugin-dialog` 系统目录选择器；远程模式（浏览器访问）无桌面对话框，回退为输入路径字符串。
- [x] Q3 文件编辑器是否支持多文件 tab？→ **多实例 tab**（2026-08-16 已确认）
  - 点文件开新 tab（按文件路径区分实例），多文件并存切换、各自独立未保存状态；需扩展布局面板模型，兼容现有 chat/neurons 单实例语义。

## Solution Options / 方案候选

### Option A / 方案 A：单实例文件编辑器 + 内置过滤 + 输入路径添加（最简推进）

- 推荐：是
- 方案摘要：
  - 后端：新增 `core/workspace.rs`（WorkspaceStore，管理 `workspaces.json`：条目含 id/name/root/active）+ `core/fs.rs`（文件操作层：list/read/write/create_dir/delete/rename/move/glob/grep/info，统一路径越界护栏 + 大小/数量限制 + 二进制检测）+ `core/fs_tools.rs`（10 个 AI 原生工具实现，复用 fs.rs 语义）。
  - 工具注册：`assemble_local_tools` 中 `register_core(...)` 追加；新增 `inserts/` 下 10 个 `.md` 门禁文档。
  - 命令：`list_workspaces` / `add_workspace` / `remove_workspace` / `set_active_workspace` / `fs_list` / `fs_read` / `fs_write` / `fs_create_dir` / `fs_delete` / `fs_rename` / `fs_move` / `fs_glob` / `fs_grep` / `fs_info`。
  - 事件：`StateChange::Workspaces` 新增 kind，写操作后广播；`fs_*` 写命令成功后广播（前端刷新树）。
  - 前端：`FileExplorer.svelte`（sidebar files 视图：工作区下拉 + 添加/删除/切换 + 懒加载树 + 刷新）；`FileEditor.svelte`（main file-editor 面板：CodeMirror 6 语法高亮 + 保存 + 外部修改检测 + 未保存标记）；`views.ts` / `layoutTypes.ts` / `mainPanelMeta` 注册；`dataStore` 增 workspaces 状态与 actions；i18n 补键；`api/types.ts` 增 `workspaces` kind。
  - 编辑器依赖：`codemirror`（主包）+ `@codemirror/language-data`（常见语言按需加载，避免逐语言包手选）。
- 涉及模块：`core/workspace.rs`、`core/fs.rs`、`core/fs_tools.rs`、`core/events.rs`、`core/gateway.rs`、`core/mod.rs`、`lib.rs`、`net/rpc.rs`、`inserts/*`、`views.ts`、`layoutTypes.ts`、`dataStore.svelte.ts`、`FileExplorer.svelte`、`FileEditor.svelte`、`translations.ts`、`api/types.ts`、`package.json`
- 优点：改动面收敛、复用现有面板/事件/通道模型、依赖增量小（后端零新依赖，前端仅 CodeMirror）
- 缺点：单实例编辑器同时只能看一个文件；无系统目录选择器；过滤不可配置
- 风险：低。路径护栏是核心安全点，需充分单测

### Option B / 方案 B：多实例文件 tab + 系统目录选择器（体验更全）

- 推荐：是
- 方案摘要：在方案 A 基础上：main 区编辑器支持多实例（文件维度 tab）；引入 `tauri-plugin-dialog` 系统选目录；过滤规则按工作区可配置（写入 workspaces.json）。
- 涉及模块：方案 A 全部 + `LayoutStore`（多实例面板语义）、`layoutTypes`（MainPanel 扩展）、`EditorTabs`（多实例）、Cargo/前端 dialog 插件依赖
- 优点：对齐 VS Code 多标签体验、目录选择更友好、过滤可配置
- 缺点：布局系统需扩展多实例面板（现有「同一类型全局唯一」约束要改，波及 chat/neurons 语义）、依赖与复杂度显著上升
- 风险：中。布局改造可能影响现有面板行为

## Decision / 方案决策

- Selected / 选定方案：**Option B**（2026-08-16 用户确认）
- Why / 选择原因：用户确认多文件 tab（多实例）、系统对话框 + 输入回退、过滤基于工作目录配置；对齐 VS Code 体验
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：2026-08-16
- Open Questions 状态：Q1–Q3 全部关闭（用户已确认）

## API Design / API 设计

> 本交付新增对外契约（tauri commands + StateChange kind + AI 工具），需要固化。

### Contract Scope / 契约范围

- 变更类型：新增（不破坏现有命令/事件/工具契约）
- 消费方：前端文件树/编辑器、Agent 会话（AI 工具）、远程模式 RPC
- 真相源文件：`core/workspace.rs`、`core/fs.rs`、`lib.rs`、`api/types.ts`、`views.ts`、`layoutTypes.ts`

### Workspace 相关类型（Rust ↔ TS 一致）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub id: String,        // 稳定 id（uuid），跨重启保留
    pub name: String,      // 展示名（默认取根目录名）
    pub root: PathBuf,     // 规范化后的绝对路径（canonicalize）
    pub ignore: Vec<String>, // 该工作区文件树过滤规则（glob/前缀），v1 可编辑
    pub created_at: i64,   // 毫秒时间戳
}

pub struct WorkspaceView {            // 前端读取/写入形状
    pub workspaces: Vec<WorkspaceEntry>,
    pub active_id: Option<String>,
}
```

```ts
export interface WorkspaceEntry {
  id: string;
  name: string;
  root: string;
  ignore: string[];
  created_at: number;
}
export interface WorkspaceView {
  workspaces: WorkspaceEntry[];
  active_id: string | null;
}
```

### Tauri commands（lib.rs + net/rpc.rs 同步注册）

| command | params | returns | 说明 |
|---|---|---|---|
| `list_workspaces` | — | `WorkspaceView` | 读全部 + active |
| `add_workspace` | `{ root: string }` | `WorkspaceView` | 校验目录存在且未重复；canonicalize；广播 Workspaces |
| `remove_workspace` | `{ id: string }` | `WorkspaceView` | 移除条目（不删目录）；active 失效则清除；广播 |
| `set_active_workspace` | `{ id: string }` | `WorkspaceView` | 设置 active；校验存在；广播 |
| `fs_list` | `{ path?: string, ignore?: string[] }` | `Vec<FsEntry>` | 列目录（相对 active workspace 根）；应用内置/传入过滤 |
| `fs_read` | `{ path: string, offset?: number, limit?: number }` | `FsReadResult` | 读文件（行号分段）；记录「已读」清单 |
| `fs_write` | `{ path: string, content: string }` | `FsWriteResult` | 写文件（覆盖已存在需已读）；外部修改检测；广播 |
| `fs_create_dir` | `{ path: string }` | `()` | 建目录；广播 |
| `fs_delete` | `{ paths: string[] }` | `()` | 删文件/目录（须在 workspace 内）；广播 |
| `fs_rename` | `{ from: string, to: string }` | `()` | 重命名；广播 |
| `fs_move` | `{ from: string, to: string }` | `()` | 移动；广播 |
| `fs_glob` | `{ pattern: string, cwd?: string }` | `Vec<FsMatch>` | glob 模式查找，按修改时间排序 |
| `fs_grep` | `{ pattern: string, path?: string, case_sensitive?: bool, multiline?: bool, glob?: string, context?: number }` | `Vec<GrepMatch>` | 内容搜索 |
| `fs_info` | `{ path: string }` | `FsInfo` | 文件/目录元信息 |

```rust
pub struct FsEntry {
    pub name: String,
    pub path: String,          // 相对 workspace 根
    pub is_dir: bool,
    pub size: Option<u64>,     // 文件有；目录 None（或递归大小，v1 不递归）
    pub modified_ms: Option<i64>,
}
pub struct FsReadResult {
    pub content: String,
    pub total_lines: usize,
    pub total_chars: usize,
    pub mtime_ms: i64,         // 供保存冲突检测
    pub truncated: bool,
}
pub struct FsWriteResult {
    pub mtime_ms: i64,
}
pub struct FsMatch { pub path: String, pub modified_ms: i64 }
pub struct GrepMatch { pub path: String, pub line: usize, pub column: usize, pub text: String }
pub struct FsInfo {
    pub exists: bool, pub is_dir: bool, pub size: u64,
    pub modified_ms: Option<i64>, pub is_binary: bool,
}
```

### AI 原生工具（10 个，均 native 来源 + insert 门禁）

| 工具 | 参数（JSON Schema 要点） | 返回 | 关键语义 |
|---|---|---|---|
| `list_directory`(LS) | `path?` / `ignore?` | `Vec<FsEntry>` | 默认 active workspace 根；过滤 |
| `read_file`(Read) | `path` / `offset?` / `limit?` | `FsReadResult` | 行号分段；标记已读 |
| `write_file`(Write) | `path` / `content` | `FsWriteResult` | 覆盖已存在需先已读（内存清单）；外部修改检测 |
| `search_replace` | `path` / `search` / `replace` | `{matched: bool}` | SEARCH/REPLACE 首处匹配；需先已读 |
| `delete_file` | `paths: string[]` | `()` | 一次多文件；须存在；须在 workspace 内 |
| `glob` | `pattern` / `cwd?` | `Vec<FsMatch>` | glob 模式；修改时间排序 |
| `grep` | `pattern` / `path?` / 开关/上下文 | `Vec<GrepMatch>` | 正则/大小写/多行/类型过滤/计数 |
| `file_info` | `path` | `FsInfo` | 元信息 |
| `create_directory` | `path` | `()` | 递归创建 |
| `rename` / `move` | `from` / `to` | `()` | 同 workspace 内 |

> 注：AI 工具统一以 active workspace 为根（绝对路径校验前缀），与前端同一护栏函数。`rename` 与 `move` 语义差异：rename 同目录改名，move 跨目录移动；实现可合并为同一底层 fs 调用。

### StateChange 扩展

```rust
// core/events.rs 新增
Workspaces,   // 工作区列表/active 变化 → 前端刷新文件树与选择器
// fs_* 写操作成功后复用 Workspaces 广播（前端树刷新）。
```

```ts
// api/types.ts StateChangePayload 新增
| { kind: "workspaces" }
```

### Compatibility Notes / 兼容说明

- 现有命令/事件/工具全部保留，纯增量。
- `MainPanelType` 新增 `"file-editor"`；`viewRegistry` 新增 `files`（sidebar 可移动，`movableTo: "*"`）。
- 远程模式：`fs_*` / workspace 命令在 `net/rpc.rs` 逐一注册分支，复用 `ApiClient.invoke` 无需前端改造。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本方案（Option B）+ Q1–Q3 用户确认后获批（均已确认）
- 若执行前需求、API、范围或交互规则变化：先回写 requirements.md / technical-plan.md

### Step 1. 后端文件操作层与工作区存储

#### 文件：`src-tauri/src/core/workspace.rs`（新增）

- 改动类型：新增
- 改动内容：`WorkspaceEntry`（含 `ignore: Vec<String>` 字段）/ `WorkspaceView` / `WorkspaceStore`（`Arc<RwLock<WorkspaceStore>>`，读 workspaces.json、原子写回、增删、active 管理、ignore 更新、canonicalize 校验）；`resolve_in_workspace(root, path) -> AppResult<PathBuf>`（越界拒绝）；内置默认 ignore（.git / node_modules / target / dist / .pulsar 等，作为新工作区初始值，用户可改）
- 设计约束：
  - API：遵循上方 `WorkspaceView` 契约
  - 状态：workspaces.json 持久化，写操作后广播
- 验收点：单测覆盖增删/切换/ignore 更新/持久化往返/越界拒绝/重复添加

#### 文件：`src-tauri/src/core/fs.rs`（新增）

- 改动类型：新增
- 改动内容：文件操作层（list/read/write/create_dir/delete/rename/move/glob/grep/info）；list 接受工作区 ignore 规则过滤；大小上限（读默认 256KB 分页阈值、写上限）；二进制检测（NUL 字节启发式）；「已读清单」内存结构（workspace 内 path → mtime）供 write/search_replace 校验
- 设计约束：
  - API：遵循上方命令契约；错误经 `AppError`（可读 message）
  - 状态：已读清单为进程内存态，重启清空（v1 可接受）
- 验收点：单测覆盖读写往返/分段读/二进制拒绝/越界拒绝/已读校验/外部修改检测/glob/grep 基础用例

#### 文件：`src-tauri/src/core/fs_tools.rs`（新增）

- 改动类型：新增
- 改动内容：10 个 `Tool` trait 实现（复用 fs.rs）；各自 `parameters()` JSON Schema；描述含工作区边界说明
- 设计约束：native 来源（`register_core` 或 `register`）；`inserts/<name>.md` 齐备
- 验收点：`list_tools` 可见 10 工具；schema 合法；错误含可读信息

#### 文件：`src-tauri/inserts/*.md`（新增 10 个）

- 改动类型：新增
- 改动内容：`list_directory.md` / `read_file.md` / `write_file.md` / `search_replace.md` / `delete_file.md` / `glob.md` / `grep.md` / `file_info.md` / `create_directory.md` / `rename.md`（`## 工具` 段首行一句话）
- 验收点：`list_insert_catalog` 可见；`register` 不 panic

#### 文件：`src-tauri/src/core/mod.rs` / `gateway.rs`

- 改动类型：修改
- 改动内容：mod 声明；`Gateway` 持有 `Arc<RwLock<WorkspaceStore>>`（setup 构造，manage）；`assemble_local_tools` 注册 fs 工具（工具共享 workspace store）；`list_workspaces` / `add_workspace` 等 gateway 方法
- 验收点：`cargo test --lib` 全绿；启动装配无 panic

### Step 2. Tauri commands + RPC + 事件

#### 文件：`src-tauri/src/core/events.rs`

- 改动类型：修改
- 改动内容：`StateChange::Workspaces` 变体 + 序列化测试
- 验收点：serde 测试通过

#### 文件：`src-tauri/src/lib.rs`

- 改动类型：修改
- 改动内容：14 个 commands（workspace ×4 + fs ×10）注册进 `generate_handler!`
- 验收点：`cargo check` 通过；命令可被前端 invoke

#### 文件：`src-tauri/src/net/rpc.rs`

- 改动类型：修改
- 改动内容：14 个 cmd 分支（与 lib.rs 相同业务语义，写操作后广播 Workspaces）
- 验收点：RPC 冒烟测试/远程模式手动验证

### Step 3. 前端：类型、事件、布局注册

#### 文件：`src/lib/api/types.ts`

- 改动类型：修改
- 改动内容：`StateChangePayload` 增 `{ kind: "workspaces" }`
- 验收点：类型检查通过

#### 文件：`src/lib/types.ts`

- 改动类型：修改
- 改动内容：`WorkspaceEntry`（含 ignore）/ `WorkspaceView` / `FsEntry` / `FsReadResult` / `FsWriteResult` / `FsMatch` / `GrepMatch` / `FsInfo`
- 验收点：类型检查通过

#### 文件：`src/lib/layout/layoutTypes.ts`

- 改动类型：修改
- 改动内容：`MainPanelType` 增 `"file-editor"`；`DEFAULT_LAYOUT.containers.sidebar.views` 增 `"files"`；`MainPanel` 增可选实例 key（`instance?: string`，file-editor 用文件路径区分多开）
- 验收点：类型检查通过

#### 文件：`src/lib/layout/LayoutStore.svelte.ts`

- 改动类型：修改
- 改动内容：`insertPanel` 兼容实例语义：file-editor 按 `instance`（文件路径）区分，同路径已开则激活、新路径开新 tab；chat/neurons 等保持单实例不变
- 验收点：多文件 tab 可开可切换；现有面板行为回归不变

#### 文件：`src/lib/layout/views.ts`

- 改动类型：修改
- 改动内容：`viewRegistry["files"]`（FileExplorer，movableTo `"*"`）+ `mainViews` 增 `file-editor`（FileEditor）+ `mainPanelMeta["file-editor"]`（icon/label）
- 验收点：sidebar 显示 files tab；main 区可插入 file-editor 多实例

### Step 4. 前端：文件树与编辑器

#### 文件：`src/lib/components/FileExplorer.svelte`（新增）

- 改动类型：新增
- 改动内容：
  - 工作区选择器（下拉 + 添加「系统对话框/输入回退」+ 删除 + 切换 active + 编辑 ignore 过滤规则）
  - 懒加载树（展开按需 `fs_list`，按工作区 ignore 过滤）；刷新按钮；节点折叠/展开/错误态
  - 点击文件 → 按文件路径开/激活 file-editor 面板
  - **编辑操作交互（用户确认）**：
    - 触发：右键菜单 + 快捷键（F2 重命名 / Delete 删除）+ 顶部工具条（新建文件/新建文件夹/刷新）
    - 菜单按目标类型区分：空白/根（新建文件、新建文件夹、刷新、添加工作区）；目录（+重命名、删除、移动）；文件（+打开、复制路径）
    - 新建/重命名走节点 inline 输入框：Enter 确认、Esc 取消；后端校验重名/越界，失败提示并回滚
    - 删除：直接删无确认（用户确认），后端护栏保证不越界
    - 移动：支持拖拽（拖到目标目录上放），拖拽命中 + 嵌套/循环校验 + 跨工作区拒绝；同时右键菜单提供「移动」入口作为备选
    - 刷新：顶部按钮 + 右键菜单项
    - 复制路径：文件右键菜单（写剪贴板）
  - **pad/移动端适配（用户补充）**：上下文触发按钮（⋮）放在条目右侧，点击弹同一右键菜单；无 hover/右键场景下可用
- 设计约束：
  - API：`api.invoke` workspace/fs 命令；桌面端 `@tauri-apps/plugin-dialog` 选目录（非 Tauri 环境回退输入框）
  - 状态：监听 `workspaces` 事件刷新
  - 交互：对齐 ToolPanel 面板风格；新增 `ContextMenu.svelte`（轻量右键菜单，目前前端无现成组件）+ 拖拽实现（HTML5 DnD）
- 验收点：树加载/展开/刷新/错误态可用；添加工作区（对话框/输入两路）可用；ignore 编辑生效；新建/重命名/删除/拖拽移动/复制路径可用；pad 端条目右侧 ⋮ 按钮可用；点击文件打开编辑器

#### 文件：`src/lib/components/FileEditor.svelte`（新增）

- 改动类型：新增
- 改动内容：CodeMirror 6 编辑器（`codemirror` + `@codemirror/language-data` 按扩展名/内容嗅探）；打开文件（`fs_read`，支持 offset/limit 懒加载长文件）；保存（Ctrl+S/按钮 → `fs_write`，携带打开时 mtime 做冲突检测，冲突弹确认）；未保存标记（tab 标题 ●）；保存后广播刷新树
- 设计约束：
  - API：`fs_read` / `fs_write`
  - 状态：dirty 标记、mtime 快照、当前文件路径（多实例各自独立）
  - 交互：对齐现有 EditorTabs 标题展示（文件路径为实例区分）
- 验收点：打开/编辑/保存/冲突提示/未保存标记可用；常见语言语法高亮

#### 文件：`src/lib/stores/dataStore.svelte.ts`

- 改动类型：修改
- 改动内容：`state.workspaces: WorkspaceView`；bootstrap 拉取；`handleStateChanged` 增 `workspaces` 分支；actions（addWorkspace / removeWorkspace / setActiveWorkspace / updateWorkspaceIgnore）
- 验收点：工作区状态随事件刷新

#### 文件：`src/lib/i18n/translations.ts`

- 改动类型：修改
- 改动内容：zh/en 三处补 `views.files` / `views.fileEditor` / `fileExplorer.*` / `fileEditor.*` 键
- 验收点：`pnpm check` 0 error

#### 文件：`package.json` / `Cargo.toml` / `lib.rs`

- 改动类型：修改
- 改动内容：前端 dependencies 增 `codemirror`（^6）、`@codemirror/language-data`、`@codemirror/state` / `@codemirror/view`、`@tauri-apps/plugin-dialog`；后端 Cargo 增 `tauri-plugin-dialog = "2"`；`lib.rs` 注册 `.plugin(tauri_plugin_dialog::init())`；`capabilities/default.json` 增 dialog 权限
- 验收点：`pnpm install` / `cargo check` 成功；桌面目录选择可用

### Step 5. 检查与回写

#### 命令

- 运行：`cargo test --lib`；`cargo check --all-targets`；`pnpm check`；`pnpm build`
- 修复：按失败逐项修复，涉及契约变化先回写本方案 `API Design`

#### 文件：`docs/sdd-lab/2026-08-16_23-45_workspace-file-management/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|---|---|
| 路径越界逃逸（`../`、符号链接指向 workspace 外） | 统一 `resolve_in_workspace`：canonicalize 后前缀校验 + 组件级遍历拒绝；覆盖工具/命令两入口；单测覆盖 |
| 大文件/深目录导致 UI 卡顿或内存膨胀 | 读支持行分段（offset/limit，默认阈值）；树懒加载；glob/grep 结果上限 |
| 覆盖已读校验与前端编辑冲突 | 已读清单按「路径→mtime」校验；前端保存携带打开时 mtime，外部改动提示确认（需求已确认） |
| 布局多实例改造波及 chat/neurons 单实例语义 | 仅 file-editor 走实例 key 分支，现有类型默认保持「全局唯一」；回归验收含现有面板行为 |
| 远程模式命令遗漏（RPC 未注册 → 前端调用失败） | lib.rs 与 rpc.rs 命令清单对齐步骤化；验收含远程冒烟 |
| 新增 CodeMirror / dialog 插件依赖体积/兼容 | 锁 ^6 主版本；`language-data` 按需加载避免全量语言包；dialog 仅桌面路径启用，远程回退输入 |
| AI 工具破坏性操作 | 全部经 workspace 边界护栏；delete/rename/move 同栏；insert 文档写明「忌用」段 |

## Execute Checkpoint / 执行检查点

- 当前理解：新增可配置工作区文件管理——后端 fs 操作层 + workspace 存储（含 per-workspace ignore）+ 10 个 AI 原生工具 + 前端文件树/CodeMirror 编辑器（多实例 tab）；纯增量契约，布局做多实例扩展。
- 核心目标：打通 AI 与 UI 共用同一套工作区边界下的文件读写链路，体验对齐 VS Code（多 tab / 系统选目录 / 过滤可配置）。
- 下一步动作：用户确认技术方案后进入 Step 1（后端 workspace + fs 层）。
- 风险：路径护栏（核心安全点）、已读校验、布局多实例改造、RPC 双注册、CodeMirror/dialog 依赖。
