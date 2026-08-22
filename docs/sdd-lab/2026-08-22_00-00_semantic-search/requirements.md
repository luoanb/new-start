# Requirements / 需求文档: semantic-search

## Restated Understanding / 需求复述

- 我理解当前需求是：为 pulsar-app 的 `fileops/` 文件管理领域新增「语义搜索」子能力，面向两个调用方：
  - **面向 AI（工具形）**：新增 `semantic_search` 原生工具，注册进 `register_file_tools`（native + Core 标签，任何对话都带上），与现有 `grep` / `glob` 同族，以 active workspace 为根。工具返回「代码块」而非「行」——即按函数/结构体/类等语义单元返回，含路径、行范围、块类型与内容摘要。
  - **面向人（命令/RPC + 前端）**：新增 `fs_semantic_search` Tauri command + `net/rpc.rs` 同步分支（远程模式同接口），前端提供搜索面板（输入 → 结果列表 → 点击跳转文件编辑器）。
- v1 检索内核（用户确认）：**tree-sitter 语法感知分块 + SQLite FTS5 块级关键词检索**。embedding 向量通道明确不在 v1，作为 v2 增量（方案中保留扩展点）。
- 索引按项目独立存储（用户确认）：存**应用数据目录**（`app_data_dir`），按项目根路径 hash 分目录，不污染用户项目；项目移动/重命名后索引自然失效重建。
- 与现有 `grep` 的关系：`grep` 是行级关键词检索（正则），`semantic_search` 是块级/语义化检索（按代码结构单元召回）。两者互补，`semantic_search` 的定位是"不记得确切关键词时的检索入口"。
- 暂不处理：embedding/向量通道、跨项目联合搜索、cross-encoder 精排、LLM 生成式摘要、索引热更新监听（文件系统 watch）。

## Scope / 范围

- In:
  - 后端：`fileops/search/` 子模块——`chunk`（块数据模型）、`indexer`（tree-sitter 分块 + FTS5 索引写入 + 文件 mtime 增量检测）、`retriever`（FTS5 块级检索 + 排序 + 结果截断）。
  - 索引生命周期：首次搜索懒构建；再次搜索按文件 mtime/size 增量更新；workspace 根 hash 变化即独立索引，天然隔离。
  - AI 工具：`semantic_search`（参数：`query` / `top_k?` / `path?`），native 来源 + `inserts/semantic_search.md` 门禁。
  - 命令/RPC：`fs_semantic_search`（Tauri command + `net/rpc.rs` 分支），返回与 AI 工具同一形状。
  - 前端：sidebar「搜索」视图（搜索面板：输入框 + 工作区指示 + 结果列表，点击结果打开 file-editor 面板）。
  - 类型：Rust ↔ TS 的 `SearchResult` 等契约对齐。
- Out:
  - 不做 embedding / 向量检索（v2）。
  - 不做跨工作区联合搜索、不做仓库级代码图谱/调用图。
  - 不做 cross-encoder 精排（v1 用 FTS5 bm25 + 块类型加权）。
  - 不做文件系统 watch 热更新（v1 靠 mtime 增量检测，搜索时校验）。
  - 不改动现有 chat / topic / neuron / 文件工具行为。
  - 不引入 LLM 摘要、不新增 StateChange 事件（v1 同步返回，无需事件推送）。

## User Interaction / 用户交互

- 触发入口：
  - 人：sidebar「搜索」视图 → 输入查询 → 结果列表 → 点击条目在 main 区打开 file-editor（复用现有多实例面板）。
  - AI：Agent 会话中模型自主调用 `semantic_search` 工具。
- 用户操作路径：
  1. sidebar 打开「搜索」视图（需已配置 active workspace）。
  2. 输入自然语言/关键词查询，回车触发 `fs_semantic_search`。
  3. 结果列表展示块摘要（路径 + 行范围 + 块类型 + 命中内容）；点击 → 打开对应文件定位到该行范围。
- 系统反馈：
  - 首次搜索触发索引构建：返回结果附带 `indexed_blocks` / `index_duration_ms` 等元信息；构建耗时提示（v1 同步构建，超大仓库首次可能秒级）。
  - 无 active workspace：错误提示（与现有文件工具一致的 InvalidInput 语义）。
  - AI 工具执行结果以 JSON 返回（与现有 tool 返回格式一致），错误含可读 message。
- 状态变化：
  - 索引状态不引入事件推送（v1 简化）；前端面板每次搜索拿到最新结果。
- 异常/边界交互：
  - 项目被删除/移动 → 索引目录失效，下次搜索按新 hash 重建（旧索引文件残留可接受，v1 不做清理）。
  - 查询空串/过短 → 后端返回空结果或 InvalidInput。
  - 仓库文件被外部修改 → mtime 增量检测触发该文件重建分块。
- 不应发生的交互：
  - 搜索越出 active workspace 根（索引只覆盖 workspace 内文件，遵循 ignore 规则）。
  - 结果内容包含二进制/超长块导致上下文爆炸（块内容截断 + top_k 上限）。

## Acceptance Criteria / 验收标准

- [ ] 后端：`fileops/search/` 可对 active workspace 构建索引（tree-sitter 分块，尊重 workspace ignore 规则）；同一工作区二次搜索走 mtime 增量，不重复全量构建。
- [ ] 后端：`semantic_search` 与 `fs_semantic_search` 返回块级结果（path / start_line / end_line / block_type / score / content 摘要），结果按相关度排序、有 top_k 上限与内容截断。
- [ ] AI 工具：`semantic_search` 注册进 `register_file_tools`（native + Core 标签），`list_tools` 可见，`inserts/semantic_search.md` 齐备，schema 合法。
- [ ] 命令/RPC：`fs_semantic_search` 在 `lib.rs` 与 `net/rpc.rs` 同步注册，远程模式同接口。
- [ ] 前端：sidebar「搜索」视图可用（输入、结果列表、点击打开 file-editor）；无 active workspace 时提示可读错误。
- [ ] 索引隔离：两个不同 workspace 根互不串扰，各自独立索引目录。
- [ ] `cargo test --lib` 全绿；`cargo check --all-targets` 通过；`pnpm check` 0 error；`pnpm build` 通过。

## Constraints / 约束

- 业务约束：
  - 遵循现有「active workspace 边界」：索引只覆盖 workspace 内文件，沿用 workspace ignore 规则过滤。
  - 工具语义对齐现有文件工具族（`file_tool!` 宏样板、insert 门禁、JSON 返回、`FileToolContext::active()` 取工作区）。
- 技术约束：
  - Rust 后端：新增 `fileops/search/` 子模块（对齐 `gitops/` 先例子目录组织）；复用现有 `rusqlite`（bundled，FTS5）；新增 `tree-sitter` 及少量语言 grammar 依赖（记录版本）。
  - 索引存储：应用数据目录 `<app_data_dir>/search/<workspace_root_hash>/`，独立 SQLite 库，不写入项目目录。
  - 前端：Svelte 5，搜索视图走现有 `views.ts` / `layoutStore` 注册机制；复用 file-editor 多实例打开。
  - 结果上限：top_k 默认 10、上限 20；单块内容摘要默认 ≤ 400 字符。
- 时间/兼容性约束：
  - 纯增量契约：不破坏现有命令/事件/工具。
  - 前端现有视图行为不变。

## Referenced Designs / 引用设计稿

> 本迭代无 Figma/视觉稿，前端搜索面板为新增轻量组件，交互形态在本文档 `User Interaction` 定义；不创建 `visual-design.md`。

## Open Questions / 开放问题

- [x] Q1 v1 检索内核范围？→ **tree-sitter 分块 + SQLite FTS5 块级关键词检索**（2026-08-22 已确认）
  - embedding 向量通道留 v2；方案保留嵌入扩展点（chunk 持久化 + 向量列可增量添加）。
- [x] Q2 索引按项目独立存储，放哪里？→ **应用数据目录按项目根 hash 分目录**（2026-08-22 已确认）
  - `<app_data_dir>/search/<hash(workspace_root)>/search.db`，不污染项目，项目移动后重建。
- [x] Q3 面向人的调用 v1 形态？→ **命令/RPC + 前端搜索面板**（2026-08-22 已确认）
  - `fs_semantic_search` command + RPC 分支；前端 sidebar「搜索」视图。
- [ ] Q4 tree-sitter 语言覆盖范围？→ 技术方案默认：v1 引入常用语言 grammar（rust/ts/js/go/python/java/c/cpp），未知语言回退启发式分块（空行/缩进/大括号）；精确列表执行前确认。

## Requirement Decisions / 需求决策

- 2026-08-22 00:00:
  - 决策：v1 = tree-sitter 语法感知分块 + SQLite FTS5 块级检索（无 embedding）；索引存应用数据目录按项目 hash 分目录；人调用 = 命令/RPC + 前端搜索面板。
  - 原因：用户在方案对齐中明确选择；三件套中"分块 + 关键词"已比现有行级 grep 强一个量级，embedding 作为 v2 增量控制复杂度；项目独立索引保证多工作区隔离与可重建。
