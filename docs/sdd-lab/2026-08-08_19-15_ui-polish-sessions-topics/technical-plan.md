# technical-plan.md — UI 打磨：SessionList + TopicPanel

> 迭代：ui-polish-sessions-topics
> 创建：2026-08-08
> 状态：planned（待批准）

## Overview / 概述

在不改后端与数据模型的前提下，纯前端打磨两个门面面板。范围：`SessionList.svelte`、`TopicPanel.svelte`、全局 `.btn` 样式、`translations.ts` 字典补充。

## Context / 现状核实

- DESIGN tokens 定义在 `src/app.html` 的 `:root`/`[data-theme]`（OKLCH，含 `--color-error`/`--color-success`/`--color-warning`/`--color-hover`）。**没有全局 `.btn`**。
- `.btn` 在 5 个组件内各自 scoped 定义（ToolPanel/PollerPanel/NeuronDetailDrawer/TopicPanel/NeuronDetail）。Svelte scoped 选择器特异性（0-2-0）高于全局 `.btn`（0-1-0），故**抽取全局 `.btn` 不会污染其他组件的本地定义**，本次只迁移 TopicPanel。
- `Conversation = { id, mode, messages: Message[], created_at, updated_at }`；`Message = { role, body: MessageBody, timestamp }`；`MessageBody` 含 `kind: "text"` 的 `content`。会话标题可从 messages 提取。
- 现有 i18n 键：`sessionList.{title,empty,create,msgs}`、`topicPanel.*`（已较全，缺 status Record 与错误键）。

## Execution Steps / 执行步骤

### Step 1 — `src/app.html`：新增全局 `.btn` 基础样式

在 `<style>` 内（`select option` 之后）追加全局按钮词汇：

```css
/* ── 全局按钮词汇：所有面板共用（组件内 scoped 定义优先）── */
.btn {
  display: inline-flex; align-items: center; gap: var(--space-1);
  border: none; border-radius: var(--radius-sm); cursor: pointer;
  font-size: var(--fs-sm); font-weight: 500; line-height: 1;
  padding: 6px 12px;
  background: var(--color-surface); color: var(--color-text);
  border: var(--border-width) solid var(--color-border);
  transition: background var(--duration-fast) var(--ease-out),
              border-color var(--duration-fast) var(--ease-out),
              color var(--duration-fast) var(--ease-out);
}
.btn:hover:not(:disabled) { background: var(--color-hover); border-color: color-mix(in oklch, var(--color-border) 60%, var(--color-text)); }
.btn:focus-visible { outline: 2px solid var(--color-primary); outline-offset: 1px; }
.btn:active:not(:disabled) { transform: translateY(0.5px); }
.btn:disabled { opacity: 0.45; cursor: default; }
.btn-primary { background: var(--color-primary); border-color: transparent; color: var(--color-on-primary); }
.btn-primary:hover:not(:disabled) { background: var(--color-primary-dim); }
.btn-danger { color: var(--color-error); border-color: color-mix(in oklch, var(--color-error) 35%, transparent); }
.btn-danger:hover:not(:disabled) { background: color-mix(in oklch, var(--color-error) 10%, transparent); }
.btn-sm { padding: 4px 8px; font-size: var(--fs-xs); }
```

注：Motion 仅 `transform`/opacity/背景过渡，符合 DESIGN.md。

### Step 2 — `src/lib/i18n/translations.ts`：补充键（类型 + en + zh 三处同步）

`sessionList` 新增：
- `newSession: string`（en "New session" / zh "新会话"）
- `yesterday: string`（en "Yesterday" / zh "昨天"）
- `running: string`（en "Running" / zh "运行中"）
- `copyId: string`（en "Copy session ID" / zh "复制会话 ID"）
- `closeSession: string`（en "Close session" / zh "关闭会话"）
- `collapseSidebar: string`（en "Collapse sidebar" / zh "收起侧栏"）
- `expandSidebar: string`（en "Expand sidebar" / zh "展开侧栏"）
- `emptyHint: string`（en "Start a conversation to see it here" / zh "开始对话后会显示在这里"）
- `newButton: string`（en "New session" / zh "新建会话"）

`topicPanel` 新增：
- `status: Record<string, string>`：`todo`（待办/Todo）、`in_progress`（进行中/In Progress）、`paused`（已暂停/Paused）、`done`（已完成/Done）、`cancelled`（已取消/Cancelled）
- `delete: string`（en "Delete" / zh "删除"）
- `deleteConfirmTitle: string`（en "Delete topic?" / zh "删除课题？"）——确认态标题（复用原 deleteConfirm 作为确认正文）
- `createFailed` / `pauseFailed` / `resumeFailed` / `deleteFailed` / `addScopeFailed` / `completeScopeFailed` / `deleteScopeFailed`：均为 `{error}` 模板错误键

### Step 3 — `src/lib/components/SessionList.svelte`

1. **会话标题化**：新增 `sessionTitle(conv)` —— 取首条 `body.kind==="text"` 且 role 为 user/assistant 的 `content`，trim 后单行截断（CSS ellipsis）；无则 `t("sessionList.newSession")`。列表主行显示标题 + 元信息行（模式 badge、消息数、分级时间）。
2. **时间分级**：`formatTime(ts)`：今天→`HH:MM`；昨天→`t("yesterday")`；今年→`M/D`；更早→`YYYY/M/D`。
3. **active 态**：`.session-item.active` 背景改 `color-mix(in oklch, var(--color-primary) 10%, transparent)` + 已有 3px 指示条 + 标题 `font-weight:600`；hover 保持 `--color-hover`。
4. **icon-btn 词汇统一**：header 的 collapse/new 按钮与行内 copy/close 按钮改为无边框方形 26×26（对齐 ToolPanel `.icon-btn`），hover 背景 `--color-hover`（替换 `rgba(0,0,0,0.1)`）。
5. **空状态**：居中符号（现有 `+` 按钮风格的内联 SVG 消息符号）+ `emptyHint` 文案 + `.btn-primary` 主按钮。
6. **文案 i18n**：collapse/expand/copy/close/running/new/empty 全部 `t()`。
7. **collapsed 形态**：保持现状，仅统一按钮词汇。

### Step 4 — `src/lib/components/TopicPanel.svelte`

1. **状态 badge 重构**：`statusColors` 硬编码映射删除；改为 CSS 类（`.status-badge.todo/in_progress/paused/done/cancelled`），结构「圆点 + 文字」，tint 背景用 `color-mix`（语义色 12%）。移除 `--color-danger`（改 `--color-error`）。
2. **i18n 补齐**：`statusLabel(topic.status)` 走 `tMap("topicPanel.status", ...)`；过滤器按钮文案走 `tMap`；错误消息（createFailed 等）全部 `t()`。
3. **删除语义**：未确认态按钮文案 `t("topicPanel.delete")`；点击后展开确认区（`deleteConfirmTitle` + 正文 `deleteConfirm` + 确认/取消），确认按钮 `.btn-danger`。
4. **按钮统一**：删除组件内 `.btn` 样式块，改用全局 `.btn`/`.btn-primary`/`.btn-danger`/`.btn-sm`；补齐状态。
5. **过滤器紧凑化**：chips 统一 `gap: var(--space-1)`、`padding: 2px 8px`、`--fs-xs`；选中态用 primary tint。
6. **展开卡片**：展开态保留 primary 边框，加 `background: color-mix(in oklch, var(--color-primary) 4%, transparent)`；组间距微调 `--space-3`。

### Step 5 — 验证

- `pnpm check`（预期 0 errors；关注新增 `t()` 键类型强制）
- `pnpm build`
- 回写 requirements AC 勾选 + lifecycle.md 置 done

### Step 6（增量）— TopicPanel 状态筛选：chips → 三段式分段控件

**背景**：用户从产品与交互角度确认，6 个状态 chips 在窄侧栏换行、且筛选为高频常驻操作，改为「进行中 / 全部 / 已完成」三段式。聚合规则：进行中 = todo+in_progress+paused，已完成 = done+cancelled。

**改动**（纯前端，后端零改动）：
1. 状态模型：`type TopicFilter = "all" | "active" | "done"`；`filterStatus` → `filter: TopicFilter`，删除 `statusFilters` 数组。
2. `filteredTopics` derived 用 `includes()` 聚合：
   ```ts
   const ACTIVE = ["todo", "in_progress", "paused"];
   const DONE = ["done", "cancelled"];
   ```
3. 模板：`filter-bar` 渲染三段按钮（`class:active`），文案 `tMap`/`t`：`topicPanel.all` + 新增 `topicPanel.filterActive`/`filterDone`。
4. 样式：`.filter-bar` 改单行 segmented 容器（`inline-flex`、`gap:2px`、`padding:2px`、容器 `--color-surface` + border），`.filter-btn` 无边框 `flex:1`、active 段 primary 填充。移除 `flex-wrap`。
5. i18n：`topicPanel` 新增 `filterActive`（en "Active" / zh "进行中"）、`filterDone`（en "Done" / zh "已完成"）；复用 `all`。tab 顺序 `["active", "all", "done"]`（「进行中」置首）。

## Risks / 风险

- 全局 `.btn` 若与其他组件本地 `.btn` 规则冲突：scoped 特异性更高，本地胜出，仅 TopicPanel 受影响——验证阶段需确认无视觉回退。
- SessionList 标题提取依赖 messages 顺序；空会话显示占位，无异常。
- i18n 键类型强制：漏键会导致 `pnpm check` 失败，正好作为完整性校验。

## Out of Scope / 候选后续

- 迁移其余组件（ToolPanel/PollerPanel/NeuronDetail 等）的本地 `.btn` 到全局词汇。
- 会话列表虚拟滚动（消息量大时）。
