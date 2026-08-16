# Spec: topic-panel-conversation

## Goal

- 要解决什么问题：课题面板（TopicPanel）缺少「从课题直达其对话」的入口；状态徽标与进度条分占两行浪费高度；操作按钮常驻标题行造成视觉噪声。
- 验收结果：课题卡片可一键切到对应会话对话；状态徽标与进度条同行；按钮 hover 整卡才展示。

## Done Contract

- 什么算完成：TopicPanel.svelte 完成三点交互调整（打开对话按钮 / 状态+进度同行 / 按钮聚合标题右侧 hover 展示），i18n 新增 `topicPanel.openConversation` 文案（en/zh + 类型）。
- 由什么证明：前端类型检查 / build 通过；人工在界面核对三种状态的布局与交互。
- 哪些情况仍算未完成：点击按钮未切换/未弹出对应会话对话；无会话课题仍渲染可点按钮；按钮在非 hover 下泄漏显示。

## Scope

- In:
  1. 打开对话按钮：`topic.session_id` 存在时渲染「打开对话」按钮，点击调用 `useViewContext().commands.selectConversation(session_id)`（与 SessionList 一致：切换会话 + 插入/激活 main 区 chat 面板）。
  2. 状态徽标 + 进度条同行：`progress-row` 内 进度条左（flex:1 占满）、状态徽标右；徽标从 `topic-header-actions` 移除。
  3. 按钮聚合与 hover：标题右侧仅保留按钮组（打开对话 / 暂停恢复 / 删除）；整卡 hover 时展示，默认隐藏；删除确认态（confirm/cancel）始终可见；无 hover 能力的触屏设备按钮始终可见。
- Out:
  - 后端 / Tauri 无改动；TopicPanel 其他区域（过滤、新建、scope 列表等）不动。
  - 不做 hover 之外的动画、主题、无障碍专项改动（沿用现有 icon-btn 词汇）。

## Facts / Constraints

- 已确认事实：
  - `Topic.session_id` 类型为 `string | null | undefined`；无会话课题隐藏按钮（用户确认）。
  - 布局方案：进度条左、徽标右（用户确认）；hover 作用范围为整张卡片（用户确认）。
  - `useViewContext()` 已由组合根注入，`commands.selectConversation(id)` = `dataStore.selectConversation` + `layoutStore.insertPanel("chat")` + 关闭 drawer。
  - i18n 结构：`translations.ts` 中 `topicPanel` 类型定义（en L586 / zh L1016）三处同步新增 key。
- 技术/业务约束：
  - 现有删除确认态由 `deleteConfirmId === topic.id` 控制，hover 隐藏不得遮蔽确认按钮组。
  - 暂停/恢复按钮仅 `todo | in_progress | paused` 状态显示（保持现状）。
  - CSS hover 展示用 `@media (hover: hover)` 包一层，触屏设备回退为始终可见。
- 已知风险：
  - 按钮默认隐藏但保留布局空间（opacity 而非 display:none），避免 hover 时卡片抖动。
  - 删除确认态必须脱离 hover 控制，否则无法完成二次确认。

## Open Questions

- [x] 无会话课题：隐藏按钮（用户确认）
- [x] 状态与进度布局：进度条左、徽标右（用户确认）
- [x] hover 范围：整卡（用户确认）

## Restated Understanding

- 我理解当前任务是：对课题面板卡片做三点交互/布局调整——① 新增「打开对话」按钮直达绑定会话；② 状态徽标与进度条合并到一行；③ 操作按钮聚合到标题右侧、hover 整卡再展示。
- 当前核心目标是：TopicPanel 单组件 + i18n 文案的最小改动，交付三个已确认的交互行为。
- 当前边界是：纯前端；复用现有 `selectConversation` 命令与 icon-btn 样式词汇；不动后端与其余视图。
- 暂不处理：会话不存在时的兜底提示、按钮排序自定义、非 hover 设备的专门手势、视觉稿文档化。

## 接口契约设计

```ts
// TopicPanel.svelte（新增消费既有命令，无新 API）
const { commands } = useViewContext();
function handleOpenConversation(sessionId: string): void {
  commands.selectConversation(sessionId); // 切换会话 + 插入/激活 chat 面板 + 关闭 drawer
}

// i18n（translations.ts 三处同步）
// type topicPanel.openConversation: string
// en: "Open conversation"
// zh: "打开对话"
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是（探索阶段已确认现有组件、ViewContext、i18n 结构）。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：三点 UI 交互调整，方案与落点已由用户确认。
- 当前核心目标：TopicPanel 打开对话按钮 + 状态/进度同行 + 按钮 hover 聚合。
- 当前进度：探索完成（组件结构 / viewContext / i18n / 布局机制已读）。
- 下一步 1: 修改 `TopicPanel.svelte`（script + 模板 + 样式）。
- 下一步 2: 更新 `translations.ts`（类型 + en + zh）。
- 下一步 3: 前端类型检查/build 验证，人工视觉核对。
- 涉及文件 / 模块：`src/lib/components/TopicPanel.svelte`、`src/lib/i18n/translations.ts`。
- 风险：hover 隐藏与删除确认态冲突（已用「确认态始终可见」化解）；触屏无 hover（用 `@media (hover:hover)` 兜底）。
- 验证方式：`pnpm` 前端类型检查或 dev build；人工核对三点行为。
- Execution Approval: `Approved`

## Change Log

- 2026-08-16: 创建 spec；三点交互细节经 AskUserQuestion 确认（隐藏无会话按钮 / 进度条左徽标右 / hover 整卡）。
- 2026-08-16: 执行完成。TopicPanel.svelte：新增 `handleOpenConversation`（复用 `commands.selectConversation`）+ 标题右侧「打开对话」按钮（仅 `session_id` 存在时渲染）；`status-badge` 从标题行移至 `progress-row`（进度条左、徽标右）；按钮组聚合标题右侧，`@media (hover:hover)` 整卡 hover/focus-within 展示、默认 visibility 隐藏（保留空间不抖动），删除确认态脱离 hover 始终可见，触屏无 hover 时按钮始终可见。translations.ts 三处新增 `topicPanel.openConversation`（类型 + en + zh）。

## Validation

- Self-check: 模板结构（header-actions / progress-row）、hover 规则（visibility 保空间 + 不参与点击）、删除确认态脱离 hover 均复查通过。
- Static checks: `pnpm check` 5 个错误全部来自既有 `vite.config.js`（隐式 any），与本次改动无关；TopicPanel.svelte / translations.ts 无错误。
- Runtime / Test: `pnpm build` 通过（vite build 7.59s，adapter-static 产出成功）。
- Human confirmation: 待用户核对三点交互（打开对话跳转 / 状态+进度同行 / hover 展示按钮）。
- 结果汇总：编译与构建证据通过；交互视觉待人工确认。
- 核心目标是否已由证据证明完成：代码与构建层面已完成；交互验收待用户核对后定论。
- 若未完成，当前剩余差距：无代码差距，仅剩人工视觉确认。
- 剩余风险：低。hover 隐藏按钮在窄卡片下由 flex 收缩 + nowrap 控制；删除确认态已保证可见。

## Resume / Handoff

- 当前状态：执行完成，代码与构建验证通过。
- 当前卡点：无（人工核对属验收环节，非卡点）。
- 下一步唯一动作：用户在界面核对三点交互，如有偏差按 Reverse Sync 回写本 spec 后调整。
- 下一轮核心目标：按人工反馈收尾或调整。
