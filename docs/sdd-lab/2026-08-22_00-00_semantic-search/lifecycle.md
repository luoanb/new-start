# Lifecycle / 生命周期: semantic-search

```yaml
status: executing
result: in-progress
created_at: 2026-08-22 00:00
updated_at: 2026-08-22 23:00
owner: user
```

## Current Summary / 当前摘要

- 批准状态：用户确认 v1 内核（tree-sitter 分块 + FTS5）、索引按项目 hash 独立存储、人调用入口（命令/RPC + 前端面板）、技术方案已批准（"开始执行"）
- 当前状态：executing（Step 0-4 完成，Step 5 验证全绿，待远程模式冒烟与构建产物人工确认）
- 当前核心目标：为 fileops 文件管理领域新增"语义搜索"子能力——AI 侧 `semantic_search` 工具 + 人侧 `fs_semantic_search` 命令/RPC + 前端搜索面板，v1 内核 = tree-sitter 语法分块 + SQLite FTS5 块级检索
- 下一步动作：远程模式 RPC 冒烟验证（fs_semantic_search）；确认后关闭迭代

## Execution Log / 执行记录

- 1. 2026-08-22 00:00: 创建迭代。前置调研：社区 IDE 上下文语义搜索策略（Cursor RAG / Continue 混合检索 / Claude Code agentic 检索）已归档。用户确认三个关键决策：
  - v1 检索内核 = tree-sitter 语法感知分块 + SQLite FTS5 块级关键词检索（embedding 向量通道留 v2）
  - 索引存储 = 应用数据目录，按项目根 hash 分目录（不污染用户项目，项目移动后重建）
  - 人调用入口 = 命令/RPC + 前端搜索面板
- 2. 2026-08-22 00:00: 生成 requirements.md 与 technical-plan.md。状态 draft → planned。待执行批准。
- 3. 2026-08-22 22:00: 用户批准（"开始执行"）。Step 0-1：Cargo.toml 新增 tree-sitter 系 + sha2 依赖（tuna 镜像探测可用版本）；新建 `fileops/search/` 模块（chunk / indexer / retriever / tools / mod），13 个单测全绿（rust 分块、TS export、未知语言回退、增量更新、ignore 排除、相关度排序、path 过滤、空 query 拒绝、工作区隔离）。
- 4. 2026-08-22 22:30: Step 2：`SemanticSearchTool` 手动实现 Tool trait（file_tool! 宏文本作用域限制），`FileToolContext` 注入 `search_index_root`，gateway 两处装配点注入索引根，`register_core` 注册；inserts/semantic_search.md 门禁落盘。
- 5. 2026-08-22 23:00: Step 3：`fs_semantic_search` Tauri command + `net/rpc.rs` `"fs_semantic_search"` 分支（FsSemanticSearchParams camelCase）。`cargo check --lib` 通过，`cargo test --lib` 364 全绿。
- 6. 2026-08-22 23:30: Step 4：前端搜索面板——`types.ts` 增 `SearchBlock`/`SemanticSearchResult`，`contracts.ts` 增 `fsSemanticSearch` 契约，`dataStore` 增 `semanticSearch` action，`SearchPanel.svelte`（结果列表 + 点击打开文件编辑器定位块区间），`views.ts` 注册 search 视图，`layoutTypes.ts` 默认 sidebar 布局加入 search，i18n en/zh 增 views.search + searchPanel。
- 7. 2026-08-22 23:40: Step 5：验证全绿——`cargo check --all-targets` ✓、`cargo test --lib` 364/364 ✓、`pnpm check` 0 errors（20 条既有 warning 与本次改动无关）✓、`pnpm build` ✓。

## Validation / 验证记录

| 检查项 | 命令 | 结果 |
| --- | --- | --- |
| 后端编译（全目标） | `cargo check --all-targets` | PASS |
| 后端单测 | `cargo test --lib` | PASS（364/364，含 13 条 search 新增） |
| 前端类型检查 | `pnpm check` | PASS（0 errors，20 条既有 warning） |
| 前端构建 | `pnpm build` | PASS |
| 远程模式 RPC 冒烟 | `curl POST /api/rpc fs_semantic_search` | PASS（ok:true，索引 16660 块，返回相关块） |

## Incident Log / 事故记录

- 2026-08-22 20:20: 前端搜索面板报 `[unknown_command] unknown command: fs_semantic_search`。根因：浏览器远程模式连接的后端进程（8891 `pulsar-server`）为旧构建，不含新命令分支（rpc.rs `_ => unknown_command` 兜底）。修复：`cargo build --bin pulsar-server --features embed-static` 重编译 + 重启 8891 服务器（原进程 kill，同参数 PULSAR_HOST=127.0.0.1 PULSAR_PORT=8891 拉起新二进制）。curl 冒烟验证通过。
- 2026-08-22 20:40: 中文关键词搜索报 `[invalid_input] query must contain at least one searchable word`。根因：`normalize_query` 用 `is_ascii_alphanumeric` 拆词，CJK 字符全被当分隔符丢弃，纯中文查询 token 为空。修复（retriever.rs）：拆词改为 Unicode-aware `is_alphanumeric`（保留汉字）；CJK token 用 FTS5 前缀匹配 `"语义"*` 弥补「短词命中长词」局限；新增单测 `cjk_query_matches_chinese_content`。验证：search 模块 14/14、全量 365/365 通过。
- 2026-08-22 20:50: 诊断「语义搜索」完整词空结果：索引 content 无连续四字串（LIKE=0），「语义」有 281 处、前缀命中 119 块。结论：非 bug，是 **CJK 无空格分词粒度限制**（v1 内核为代码语义搜索，中文分词细化留 v2）。v1 用户侧建议用核心词搜索。
- 备注：冒烟结果中混入 `build/` / `.svelte-kit/` 前端构建产物（minified JS 单行大块）。默认 ignore 不含 `build`/`.svelte-kit`；如影响检索质量，可在 workspace ignore 规则中补充，或后续在 default_ignore 中追加（另开小迭代）。

## Open Questions / 待办

- [x] 远程模式 RPC 冒烟：启动 pulsar-server 后调用 `fs_semantic_search`，验证索引构建与检索链路（2026-08-22 完成，curl ok:true）
- [ ] 确认前端面板交互正常（用户在浏览器重试搜索）后关闭迭代（status → done）
