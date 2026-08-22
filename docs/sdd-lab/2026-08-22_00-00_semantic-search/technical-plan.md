# Technical Plan / 技术方案: semantic-search

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-22_00-00_semantic-search/requirements.md`
- 需求确认状态：已确认（2026-08-22，Q1–Q3 已确认，Q4 语言覆盖执行前确认）
- 本方案覆盖范围：`fileops/search/` 后端模块 + `semantic_search` AI 工具 + `fs_semantic_search` 命令/RPC + 前端搜索面板

## Current Project Facts / 当前项目事实

- **文件管理领域**（`src-tauri/src/fileops/`，注释"独立于 core，自包含工作区边界与文件操作"）：
  - `mod.rs`：`pub mod fs; pub mod fs_tools; pub mod gitops; pub mod workspace;`
  - `workspace.rs`：`WorkspaceEntry`（含 `ignore: Vec<String>`）/ `WorkspaceStore` / `resolve_in_workspace` 越界护栏。
  - `fs.rs`：`FileSystem` 文件操作层，已有 `grep()`（行级正则，`GrepMatch { path, line, column, text }`）、`glob()`、`is_ignored(rel, ignore)`、读文件分段/二进制检测/已读清单。
  - `fs_tools.rs`：`FileToolContext { store: Arc<WorkspaceStore>, fs: Arc<FileSystem> }` + `register_file_tools(registry, ctx)`（10 个工具，全部 `register_core`）+ `file_tool!` 宏（样板生成）+ `require_str/opt_usize` 等参数提取助手 + `ok_json` 序列化。
- **工具系统**（`core/`）：
  - `tool_registry.rs`：`Tool` trait（`name` / `description` / `parameters` / `async execute(args) -> AppResult<String>`）；`register_core` / `register_system`；native 注册走 `InsertCatalog::require(&name)` 门禁（缺 `inserts/<name>.md` 即 panic）。
  - `gateway.rs` `assemble_local_tools`（L1021）：`register_core(ExecuteCommandTool)` + `register_core(GetCurrentTimeTool)` + 读 `dynamic_tools.json`；文件工具经 `fileops/fs_tools.rs::register_file_tools` 追加。
- **命令/RPC**：`lib.rs` `generate_handler![...]` 集中注册（现有 `fs_grep` / `fs_glob` / `fs_info` 等 15 个 workspace/fs 命令）；`net/rpc.rs` `cmd.as_str()` 分支逐一转发（远程模式同接口）。
- **存储**：`core/storage.rs` `resolve(base) -> base/.pulsar`；Tauri 应用数据目录经 `app.path().app_data_dir()`（lib.rs L1667）。索引将放应用数据目录，不依赖 `.pulsar`。
- **依赖现状**（`src-tauri/Cargo.toml`）：已有 `rusqlite = { version = "0.32", features = ["bundled"] }`（FTS5 bundled 默认可用）、`regex`、`globset`、`walkdir`、`tokio`；**无 tree-sitter**（需新增，含 language grammar crates）。
- **前端**（Svelte 5 + TS）：`views.ts` `viewRegistry`（sidebar 可移动视图，`movableTo: "*"`）+ `mainViews`；`LayoutStore.svelte.ts` `insertPanel`（file-editor 已支持按 `instance`（文件路径）多开）；`dataStore.svelte.ts` 模块级 `$state` 单例 + `ApiClient.invoke`（tauriClient/httpClient 双实现）；`api/types.ts` `StateChangePayload`；i18n `translations.ts` 三处维护。
- **前端 file-editor 打开方式**：`FileExplorer.svelte` 点击文件 → `insertPanel("file-editor", { instance: path })`。搜索面板复用同一路径即可实现"点击结果打开文件定位"。

## Open Questions / 开放问题

- [x] Q1 v1 检索内核 → tree-sitter 分块 + FTS5（无 embedding）。
- [x] Q2 索引位置 → 应用数据目录按项目根 hash 分目录。
- [x] Q3 人调用入口 → 命令/RPC + 前端搜索面板。
- [ ] Q4 tree-sitter 语言覆盖范围 → **默认（待执行前确认）**：v1 引入 8 个常用语言 grammar（rust / typescript / javascript / go / python / java / c / cpp），未知语言回退启发式分块。若用户希望精简到 Rust-only 或减少依赖，可收窄。

## Solution Options / 方案候选

### Option A / 方案 A：`fileops/search/` 子模块 + 同步懒索引 + FTS5（推荐）

- 推荐：是
- 方案摘要：
  - 后端：新增 `fileops/search/{mod,chunk,indexer,retriever,tools}.rs`。`indexer` 用 tree-sitter 按顶层声明分块，写入 `<app_data_dir>/search/<hash>/search.db`（rusqlite：`files` 表做 mtime 增量、`chunks` + FTS5 虚表做检索）；`retriever` 做 query 预处理 → FTS5 MATCH → bm25 + 块类型加权 → 截断。
  - 工具：`semantic_search` 走 `file_tool!` 宏，注册进 `register_file_tools`（`register_core`），`inserts/semantic_search.md` 门禁。
  - 命令/RPC：`fs_semantic_search`（lib.rs + rpc.rs 双注册），返回与工具同一 `SemanticSearchResult` 形状。
  - 前端：sidebar 新视图 `search`（`viewRegistry["search"]`），输入框 + 结果列表 + 点击 `insertPanel("file-editor", { instance: path })` 打开定位；`dataStore` 增 search action；`api/types.ts` / 前端 `types.ts` 增 `SearchBlock` / `SemanticSearchResult`。
- 涉及模块：`fileops/mod.rs`、`fileops/search/*`、`fileops/fs_tools.rs`、`core/gateway.rs`、`core/mod.rs`、`lib.rs`、`net/rpc.rs`、`inserts/semantic_search.md`、前端 `views.ts` / `layoutTypes.ts` / `dataStore.svelte.ts` / `SearchPanel.svelte` / `api/types.ts` / `types.ts` / `translations.ts` / `Cargo.toml`
- 优点：改动收敛在 fileops 领域内部 + 少量装配；复用 FileToolContext / file_tool! / workspace 边界 / insert 门禁 / file-editor 多实例；rusqlite 已有，FTS5 零新存储依赖；索引不污染项目。
- 缺点：首次搜索同步构建可能秒级（超大仓库）；未知语言分块质量下降；无向量语义（限关键词泛化）。
- 风险：中。tree-sitter 解析失败回退、FTS5 代码分词适配、索引陈旧（mtime 增量兜底）。

### Option B / 方案 B：`fileops/search` + 后台异步索引 + 索引状态事件

- 推荐：否（v1）
- 方案摘要：在方案 A 基础上，索引构建放后台 task，新增 `StateChange::SearchIndex` 事件推送进度/就绪，搜索命令在索引未就绪时返回 pending 或等待。
- 优点：首次搜索不阻塞；进度可展示。
- 缺点：新增事件 kind + 前端订阅/轮询逻辑，复杂度上升；v1 需求未要求进度展示。
- 风险：中。异步状态机与工具超时语义纠缠。

## Decision / 方案决策

- Selected / 选定方案：**Option A**（推荐，待用户确认）
- Why / 选择原因：v1 目标是对齐现有文件工具族的"同步调用"语义（`grep`/`glob` 均为同步返回），同步懒索引 + mtime 增量足够；索引事件/后台异步留 v2（与 embedding 通道同批评估）。
- Decision Owner / 决策人：用户
- Decision Time / 决策时间：2026-08-22

## API Design / API 设计

> 本交付新增对外契约（AI 工具 + Tauri command + RPC），需要固化。

### Contract Scope / 契约范围

- 变更类型：新增（不破坏现有命令/事件/工具契约）
- 消费方：前端搜索面板、Agent 会话（AI 工具）、远程模式 RPC
- 真相源文件：`fileops/search/{mod,retriever,tools}.rs`、`lib.rs`、`net/rpc.rs`、前端 `types.ts` / `api/types.ts`

### Rust ↔ TS 类型（一致）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchBlock {
    pub path: String,      // 相对 workspace 根
    pub start_line: usize, // 1-based，含
    pub end_line: usize,   // 1-based，含
    pub block_type: String, // function|method|struct|class|impl|trait|enum|interface|file|unknown
    pub score: f64,        // bm25(0~N) + 块类型加权
    pub content: String,   // 截断摘要（默认 ≤ 400 字符）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    pub results: Vec<SearchBlock>,
    pub indexed_blocks: usize,     // 本次搜索所用索引的总块数
    pub index_duration_ms: u64,    // 本次索引构建/增量耗时（0 = 索引已就绪未重建）
}
```

```ts
export interface SearchBlock {
  path: string;
  start_line: number;
  end_line: number;
  block_type: string;
  score: number;
  content: string;
}
export interface SemanticSearchResult {
  results: SearchBlock[];
  indexed_blocks: number;
  index_duration_ms: number;
}
```

### AI 原生工具（1 个，native 来源 + insert 门禁）

| 工具 | 参数（JSON Schema 要点） | 返回 | 关键语义 |
|---|---|---|---|
| `semantic_search` | `query: string`（必填）/ `top_k?: number`（默认 10，上限 20）/ `path?: string`（相对 workspace 根过滤） | `SemanticSearchResult` | 对 active workspace 的块级索引做语义化关键词检索；首次调用触发索引构建；尊重 workspace ignore 规则；`path` 仅匹配该文件路径前缀 |

> 描述示例：`"Semantically search code blocks (functions, classes, structs) across the active workspace. Returns whole code units with line ranges, not single matching lines. Prefer over grep when you want to find where a concept is implemented without knowing exact identifiers."`

### Tauri command（lib.rs + net/rpc.rs 同步注册）

| command | params | returns | 说明 |
|---|---|---|---|
| `fs_semantic_search` | `{ query: string, top_k?: number, path?: string }` | `SemanticSearchResult` | 同工具语义；无 active workspace 返回可读错误 |

### 索引存储布局

```
<app_data_dir>/search/<sha256(workspace_root)[..16]>/search.db
  meta(key TEXT PRIMARY KEY, value TEXT)          -- schema_version / workspace_root
  files(path TEXT PRIMARY KEY, mtime_ms INTEGER, size INTEGER)  -- mtime 增量检测
  chunks(id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT, start_line INTEGER,
         end_line INTEGER, block_type TEXT, content TEXT)
  chunks_fts(id UNINDEXED, content)               -- FTS5 external content，unicode61
```

- v2 扩展点：`chunks` 增加 `embedding BLOB` 列 + 向量检索，不破坏 v1 表结构。
- hash 用 workspace 根 canonicalize 后 sha256 前缀 16 位，降低冲突且目录可读。

### Compatibility Notes / 兼容说明

- 纯增量：现有命令/事件/工具全部保留。
- `viewRegistry` 新增 `search`（sidebar 可移动，`movableTo: "*"`）；无 `MainPanelType` 变更（复用 file-editor 打开结果）。
- 远程模式：`fs_semantic_search` 在 `net/rpc.rs` 注册分支，复用 `ApiClient.invoke` 无需前端改造。
- 不新增 `StateChange` 变体。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本方案（Option A）+ Q4 语言覆盖确认后获批
- 若执行前需求、API、范围或交互规则变化：先回写 requirements.md / technical-plan.md

### Step 1. 后端依赖与 `fileops/search/` 模块

#### 文件：`src-tauri/Cargo.toml`

- 改动类型：修改
- 改动内容：新增 `tree-sitter = "0.22"` 及 v1 语言 grammar（`tree-sitter-rust` / `tree-sitter-typescript`（含 tsx） / `tree-sitter-javascript` / `tree-sitter-go` / `tree-sitter-python` / `tree-sitter-java` / `tree-sitter-c` / `tree-sitter-cpp`，版本取与 tree-sitter 0.22 兼容系列）；`rusqlite` 已存在无需改动
- 验收点：`cargo check` 通过

#### 文件：`src-tauri/src/fileops/search/chunk.rs`（新增）

- 改动内容：`BlockType` 枚举（Function/Method/Struct/Class/Impl/Trait/Enum/Interface/File/Unknown）+ `CodeChunk { path, start_line, end_line, block_type, content }`
- 验收点：单测覆盖序列化/行号边界

#### 文件：`src-tauri/src/fileops/search/indexer.rs`（新增）

- 改动内容：
  - 语言检测（按扩展名映射 grammar；未知扩展名 → 回退启发式分块：空行 + 缩进变化 + 大括号平衡切块，`block_type = File/Unknown`）
  - `tree-sitter` 解析 → 遍历顶层/关键节点（function/method/class/struct/impl/trait/enum/interface）提取行范围与文本 → `CodeChunk`
  - `ensure_index(ws: &WorkspaceEntry) -> IndexStats`：打开/建库 → 读 `files` 表 → `walkdir`（应用 ignore）对比 mtime/size → 变更文件重分块（先删旧 chunks）、新增插入、消失删除；元信息返回 `indexed_blocks` / `index_duration_ms`
  - 单文件大小上限（默认 512KB 超限跳过）、二进制跳过（复用 fs.rs 启发式）
- 设计约束：索引目录 `<app_data_dir>/search/<hash>/`，应用数据目录经注入（不进 `FileToolContext` 现有字段，`SearchToolContext` 或构造参数传入 `PathBuf`）
- 验收点：单测覆盖建库/增量（改文件、加文件、删文件）/ignore 过滤/未知语言回退

#### 文件：`src-tauri/src/fileops/search/retriever.rs`（新增）

- 改动内容：
  - query 预处理：小写、保留字母数字下划线、去其余符号 → 空格拆分（对齐 unicode61 拆词）
  - FTS5 `MATCH` 查询（`content:...`），`bm25()` 排序；`path` 参数追加 `path == ? OR path LIKE ?/prefix` 过滤
  - 块类型加权：`impl/trait/interface` +0.6、`function/method/struct/class/enum` +0.3、其余 0；最终 `score = bm25 + weight`
  - 截断：top_k 上限、`content` 截断（默认 ≤ 400 字符）
- 验收点：单测覆盖相关度排序/加权/path 过滤/空 query 拒绝/top_k 上限

#### 文件：`src-tauri/src/fileops/search/mod.rs`（新增）

- 改动内容：领域说明 + `pub mod chunk; pub mod indexer; pub mod retriever; pub mod tools;`

#### 文件：`src-tauri/src/fileops/mod.rs`

- 改动类型：修改
- 改动内容：追加 `pub mod search;`

### Step 2. AI 工具 + 门禁 + 装配

#### 文件：`src-tauri/src/fileops/search/tools.rs`（新增）

- 改动内容：`SemanticSearchTool`，持 `FileToolContext`（复用 `active()` 取 workspace）+ 应用数据目录路径；走 `file_tool!` 宏或手动 `Tool` impl；参数 schema `{ query: string, top_k?: number, path?: string }`；`execute` 调 `ensure_index` → `retrieve` → `ok_json`
- 设计约束：native 来源（`register_core`）；`inserts/semantic_search.md` 齐备
- 验收点：`list_tools` 可见；schema 合法；错误含可读信息

#### 文件：`src-tauri/inserts/semantic_search.md`（新增）

- 改动内容：`## 工具` 段首行一句话（"按代码块语义搜索 active workspace"）+ 用法/忌用段
- 验收点：`list_insert_catalog` 可见；`register` 不 panic

#### 文件：`src-tauri/src/fileops/fs_tools.rs` / `core/gateway.rs` / `core/mod.rs`

- 改动类型：修改
- 改动内容：`register_file_tools` 追加 `register_core(SemanticSearchTool)`（需把应用数据目录路径传入 `FileToolContext` 或单独构造）；gateway 装配处确认数据目录来源（`app_data_dir` 或复用 `storage_root` 同级）；`mod` 声明
- 验收点：`cargo test --lib` 全绿；启动装配无 panic

### Step 3. 命令 + RPC

#### 文件：`src-tauri/src/lib.rs`

- 改动类型：修改
- 改动内容：`fs_semantic_search` command（参数 `query` / `top_k?` / `path?`，校验非空 query，返回 `SemanticSearchResult`）；注册进 `generate_handler!`
- 验收点：`cargo check` 通过；命令可被前端 invoke

#### 文件：`src-tauri/src/net/rpc.rs`

- 改动类型：修改
- 改动内容：`fs_semantic_search` 分支（与 lib.rs 同一业务调用，无写操作故不广播）
- 验收点：RPC 冒烟测试/远程模式手动验证

### Step 4. 前端搜索面板

#### 文件：`src/lib/types.ts` / `src/lib/api/types.ts`

- 改动类型：修改
- 改动内容：新增 `SearchBlock` / `SemanticSearchResult`（与 Rust 契约一致）
- 验收点：`pnpm check` 通过

#### 文件：`src/lib/stores/dataStore.svelte.ts`

- 改动类型：修改
- 改动内容：新增 `semanticSearch(query, opts)` action（`api.invoke("fs_semantic_search", ...)`）；search 结果状态（可本地组件态，无需全局 store 亦可）
- 验收点：action 可调用、错误透传

#### 文件：`src/lib/components/SearchPanel.svelte`（新增）

- 改动内容：sidebar 搜索面板——输入框（Enter 触发）+ 结果列表（路径 + `start_line-end_line` + 块类型徽标 + 命中摘要）+ 点击结果 `insertPanel("file-editor", { instance: path })` 并在编辑器定位到行范围（扩展 file-editor 可选 `selection` 参数或仅打开文件）；无 active workspace 时显示可读错误
- 设计约束：复用现有组件样式/token；i18n 键补全
- 验收点：搜索/展示/点击打开可用；错误态可见

#### 文件：`src/lib/layout/views.ts` / `src/lib/i18n/translations.ts`

- 改动类型：修改
- 改动内容：`viewRegistry["search"]`（SearchPanel，`movableTo: "*"`，默认加入 sidebar）；zh/en/type 三处补 `views.search` 与 `searchPanel.*` 键
- 验收点：`pnpm check` 0 error；sidebar 显示搜索视图

### Step 5. 检查与回写

#### 命令

- 运行：`cargo test --lib`；`cargo check --all-targets`；`pnpm check`；`pnpm build`
- 修复：按失败逐项修复，涉及契约变化先回写本方案 `API Design`

#### 文件：`docs/sdd-lab/2026-08-22_00-00_semantic-search/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|---|---|
| tree-sitter grammar 版本与主 crate 兼容性/解析失败 | grammar 锁兼容版本（0.22 系列）；单文件解析失败捕获并回退启发式分块，不阻断全库 |
| 首次索引同步构建延迟（超大仓库） | 单文件大小上限跳过 + 结果返回 `index_duration_ms` 提示；v2 转后台异步（Option B 预留） |
| FTS5 对代码标识符分词不友好（`foo_bar`、`::`、泛型尖括号） | query 预处理规范化（保留字母数字下划线、去符号、小写）+ unicode61 拆词；检索词按 AND 组合 |
| 索引陈旧（外部修改、git checkout 切换分支） | `files` 表 mtime/size 增量检测，搜索时校验重建；结果附带索引统计供前端判断 |
| 未知语言分块质量低 | 回退启发式分块（`block_type = unknown`），结果仍可用但块边界粗糙；Q4 确认 v1 语言清单 |
| 多工作区索引串扰 | 按 canonicalized root 的 sha256 前缀分目录，独立 SQLite；测试覆盖双工作区隔离 |
| 索引体积/结果上下文爆炸 | top_k 默认 10 上限 20；单块 content 截断 ≤ 400 字符；AI 工具描述写明返回摘要非全文 |

## Execute Checkpoint / 执行检查点

- 当前理解：为 fileops 领域新增语义搜索——`fileops/search/`（tree-sitter 分块 + FTS5 检索）+ `semantic_search` 工具 + `fs_semantic_search` 命令/RPC + 前端搜索面板；索引存应用数据目录按项目 hash 分目录，mtime 增量；纯增量契约。
- 核心目标：打通「块级语义化检索」能力，AI 与 UI 共用同一索引与检索内核，体验对齐现有文件工具族。
- 下一步动作：用户确认本方案（含 Q4 语言覆盖范围）后进入 Step 1（依赖 + `fileops/search` 后端）。
- 风险：tree-sitter 依赖与 grammar 兼容、首次索引延迟、FTS5 代码分词、索引陈旧、前端 file-editor 定位行范围。
