# Spec: 会话列表与消息列表分页（去掉全量拉取）

## Goal

1. 会话列表不再每次拉取全量 `Conversation`（含全部 messages）：改为「摘要 + 分页」。
2. 消息列表不再一次拉取全部历史：默认只拉最新一页，向上滚动时增量加载更早消息。
3. `status` 等轻量统计不再解析全量会话文件。

## 现状问题

- 后端 `list_conversations`（conversation_store.rs:97）读取 `sessions/*.json` 全部文件并完整反序列化
  （含每条消息正文，可能带巨型工具结果），按 `updated_at` 排序后整体返回给前端。
- 前端 `refreshConversations` 每次 `conversations` 事件都整表重拉；`bootstrap` 也只拉这一种列表。
- 前端 `history`（gateway.rs:1064）返回某会话全部消息；`refreshMessages` 每次全量拉。
- `status()`（gateway.rs:1109）用 `list_conversations().len()` 只为了数个数，却解析全部文件。

## 方案

### 后端

1. **会话摘要类型 + 分页命令**（`models.rs` / `conversation_store.rs` / `gateway.rs` / `lib.rs` / `rpc.rs`）：
   - 新类型 `ConversationSummary { id, mode, message_count, preview, created_at, updated_at, extra }`，
     `preview` = 首条 user/assistant 文本消息正文（会话列表标题源）。
   - 新命令 `list_conversation_summaries(page, page_size) -> ConversationSummaryPage { items, total, has_more }`。
   - 反序列化走轻量结构：`messages` 字段用自定义 Deserialize（只数条数 + 取首条文本，
     后续消息整条 `IgnoredAny` 跳过，不解析大体积正文）；`extra` 原样携带（会话级模型选择回显）。
   - 保留既有 `list_conversations` / `history`（TUI / CLI / 测试仍用）。
2. **消息分页命令**：
   - 新命令 `history_page(conversation_id, limit, offset) -> MessagePage { messages, total, offset, has_more }`，
     坐标系为「从最新倒推」：`offset` = 已加载条数，`limit` = 本次条数，返回最早一段切片。
   - 保留既有 `history`。
3. **轻量计数**：`ConversationStore::conversation_count()`（仅统计 `.json` 文件数），
   `status()` 改用它，不再全量解析。

### 前端

4. **dataStore**（dataStore.svelte.ts）：
   - `state.conversations` 类型改为 `ConversationSummary[]`，新增 `conversationsTotal / conversationsHasMore`。
   - 新增 `loadMoreConversations()`（追加下一页，防重入）；`refreshConversations` 重置回第 0 页。
   - 消息窗口：新增 `messagesTotal / messagesHasMore / messagesOffset`（`messagesOffset` =
     首条已加载消息在全量中的绝对下标，`= total - messages.length`），`refreshMessages` 改为拉最新页
     （`limit=100, offset=0`）；新增 `loadMoreMessages()` 拉更早页并前插（防重入）。
   - `bootstrap` / `selectConversation` / 事件刷新路径同步切换新命令。
5. **SessionList.svelte**：标题用 `preview`，条数用 `message_count`；滚动近底部触发 `loadMoreConversations`。
6. **+page.svelte**：`activeConversationTitle` 改用 `preview`；`echoSessionModel` 字段不变（extra 已携带）。
7. **ChatArea.svelte**：
   - 绝对消息下标 = `messagesOffset + round.startIndex + mi`（`data-message-index`、评分、裁决卡锚点）。
   - `streamingIndex` 保持窗口内下标语义不变。
   - 滚动近顶部触发 `loadMoreMessages`，前插后按高度差恢复滚动位置（不打断阅读）。
   - 「在会话中定位」目标不在已加载窗口时，自动续拉更早页直到覆盖或拉完。

## Done Contract

- `cargo check --all-targets` 通过；`cargo test --lib` 全绿（402 passed / 0 failed，含新增 3 个单元测试：
  `history_page_slices_from_latest`（最新倒推切片 + offset 超界空页）、
  `list_conversation_summaries_light_parse_and_paginate`（巨型工具结果跳过 + preview 提取 + 分页）、
  `conversation_count_counts_files_only`）。
- `pnpm check` 0 error（存量 20 warnings 不变，非本次改动）。
- 会话列表：bootstrap / 事件刷新只走摘要分页命令；侧栏滚动到底可加载更早会话。
- 消息：默认只加载最新页；上滑顶部可加载更早消息且滚动位置不跳变；评分/定位/裁决卡锚点下标正确。
- `status()` 不再解析全量会话。

## 范围

- In：`models.rs`、`conversation_store.rs`、`gateway.rs`、`lib.rs`、`net/rpc.rs`、`types.ts`、`contracts.ts`、
  `dataStore.svelte.ts`、`SessionList.svelte`、`ChatArea.svelte`、`+page.svelte`。
- Out：TUI/CLI 的会话/消息读取（保留全量）；存储层索引/迁移；会话列表搜索/过滤。

## 风险

- 中：消息前插后的滚动位置恢复（浏览器无原生锚点，用高度差补偿，需在 DOM 更新后 rAF 修正）。
- 低：`preview` 与旧「首条文本」规则的一致性（同一规则实现，仅位置从端上移到后端）。
- 低：分页重置时机（发送/切换会话/事件刷新回最新页，丢失已加载的更早页属预期）。
