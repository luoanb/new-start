# Spec: 会话多窗口（会话列表 → main 区新开绑定会话的聊天窗口）

## Goal

1. 会话列表项新增「在新窗口打开」按钮（复制按钮左侧），点击后在 main 区打开一个绑定该会话的聊天窗口。
2. 新窗口**固定绑定**该会话（panel.id = `chat:${conversationId}`，会话 id 记录在 main 区 panel 上），
   窗口内不可切换其他会话；主窗口（跟随全局激活会话）行为不变。
3. 支持多个会话窗口并存（同分栏 tab 切换 / 分栏并排），各自独立滚动 / 发送 / 评分 / 流式。

## 现状问题

- chat 面板**单实例**（`insertPanel("chat")` 全局唯一，activity 入口只激活既有面板）。
- ChatArea 只渲染全局 `activeConversationId` 的消息（`state.messages` 单份），
  无法同时在 main 区查看两个不同会话的对话。
- 会话列表项 `session-actions` 已有 copy-btn / close-btn，可扩展行内按钮。

## 方案

### 1. 数据层（dataStore.svelte.ts）：会话视图缓存

- 新增 `chatViews: Record<conversationId, ChatViewState>`，`ChatViewState` =
  `{ messages, total, offset, hasMore, loadingOlder, streamingIndex }`（即原 state.messages 系列）。
- `state.messages / messagesTotal / messagesOffset / messagesHasMore / messagesLoadingOlder / streamingIndex`
  字段移除；ChatArea 主窗口与绑定窗口统一从 `chatViews[conversationId]` 读取（同一会话主/绑窗口共享一份数据，天然实时一致）。
- 写路径参数化：`refreshMessages(conversationId?)`、`loadMoreMessages(conversationId?)`、
  `sendMessage(..., conversationId?)`（缺省回退 `activeConversationId`，兼容现有调用）。
- 事件路由（handleStateChanged）按会话分发：
  - `message_delta`：`chatViews[payload.conversation_id]` 存在即更新（主窗口 + 绑定窗口统一覆盖）。
  - `conversations`：`affected` 中**有打开视图**的会话全部 `refreshMessages(cid)`（替代原先仅主会话刷新）。
- 新增导出 `chatViews`；`refreshMessages` 已导出（SessionList 打开窗口首次加载复用）。

### 2. 布局（复用 LayoutStore 多实例语义）

- 绑定窗口 = `layoutStore.insertPanel("chat", <当前激活分栏索引>, \`chat:${conversationId}\`)`：
  instanceId 多实例语义已存在（同 id 已开则激活其分栏与面板，否则新建）；插入当前激活分栏作为新 tab，
  无激活分栏（main 为空）时 `"new"` 新开一栏。
- 已存在同会话窗口：仅激活，不重复开、不重拉消息（避免打断正在看的滚动位置）。
- 主窗口（无绑定）仍 `insertPanel("chat")` 单实例，行为不变。

### 3. ChatArea.svelte：主 / 绑定双模式

- 经 `getContext("pulsar:panel")` 读 panel.id；`chat:` 前缀 → 绑定会话，否则主窗口。
- `conversationId = bound ?? activeConversationId`；消息 / 分页 / 流式 / 运行状态 / 评分 / 裁决卡 / 锚点定位
  一律按 `conversationId`（不再写死 activeConversationId）。
- 发送：绑定窗口走新命令 `commands.sendMessageTo(conversationId, text)`
  （复用 +page 的 sendingIds 防抖与模型选择；MVP 沿用全局模型，不回显会话 state.model）。

### 4. 会话列表 / 组合根 / i18n

- SessionList：copy-btn **左侧**新增 open-btn（新窗口 icon），`e.stopPropagation()` 后
  `insertPanel("chat", <当前激活分栏索引>, \`chat:${id}\`)`；首次打开（该面板不存在）先 `refreshMessages(id)` 加载视图。
- +page.svelte：
  - `ViewCommands` 增加 `sendMessageTo(conversationId, text)`（handleSendToConversation，逻辑同 handleSend，目标会话参数化）。
  - `paneTabs`：chat 面板若绑定会话，tab 标题/icon 色调用该会话摘要（preview / mode）；否则沿用主窗口逻辑。
- i18n：`sessionList.openWindow`（en: "Open in new window" / zh: "在新窗口打开"）。

## Done Contract

- `pnpm check` 0 error（2026-08-23 验证通过：0 errors / 20 warnings，warnings 均为既有无关项）。
- 会话列表项复制按钮左侧出现「在新窗口打开」按钮；点击后在当前激活分栏打开该会话窗口（新 tab）并加载其消息。
- 再次点击同会话 → 激活已开窗口（不重复开）；多个会话窗口可并存（同分栏 tab 切换），各自独立滚动 / 发送 / 评分 / 流式。
- 主窗口（跟随 activeConversationId）行为不变；同一会话在主窗口与绑定窗口间数据实时一致。
- 绑定窗口内发送 / 停止 / 评分均路由到窗口绑定的会话；关闭绑定窗口不影响其他窗口。

## 范围

- In：`dataStore.svelte.ts`、`ChatArea.svelte`、`SessionList.svelte`、`+page.svelte`、`viewContext.ts`、
  `i18n/translations.ts`、`layoutTypes.ts`（注释补充 chat 多实例）。
- Out：窗口内切换会话；绑定窗口的会话模型回显（沿用全局选择）；面板关闭时的视图缓存清理（数据小，保留复用）。

## 风险

- 中：ChatArea 主/绑定双模式分支增多——统一 conversationId 数据源，避免评分/裁决/锚点/流式遗漏
  （上述读点全部从 activeConversationId 改为 conversationId）。
- 低：`$state` 嵌套 Record 响应式（chatViews 深度代理，读写均被追踪；验证 pnpm check 与手测）。
- 低：绑定窗口沿用全局模型选择（非会话模型），MVP 可接受，后续可回显会话 `extra.session.state.model`。
