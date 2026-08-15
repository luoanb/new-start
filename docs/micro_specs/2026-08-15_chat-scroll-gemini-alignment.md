# Spec: 对话滚动对齐 Gemini（智能自动跟随 + 问句对齐顶部）

## Goal

- 要解决什么问题：ChatArea 消息列表原为"每次 messages 变化都强制滚到底部"，用户上滑阅读历史时会被新消息/回复拽回底部，打断阅读；且缺少 Gemini 式"发送问句后对齐视口顶部、为回答预留空间"的交互。
- 验收结果：① 用户离开底部（上滑读历史）时新消息不再打断，回到底部才恢复自动跟随；② 发送问句后新问题对齐视口顶部，回答生成后（未上滑）自动跟随到底。

## Done Contract

- 什么算完成：
  1. `ChatArea.svelte` 用 `onscroll` 监听距底距离（`scrollHeight - scrollTop - clientHeight > 5` 视为离开底部），维护 `userScrolled` 锁定标记。
  2. `$effect` 跟踪 `lastMessageCount`：仅当新消息且未上滑时才 `scrollToNewest()`；列表被重置（切会话/清空）时解锁并回到底部。
  3. `onSend` 置 `pendingAlignTop`，检测到新 user 消息时将其滚动对齐容器顶部；程序化滚动用 `autoScrolling` 抑制 `onscroll` 误判为用户上滑，对齐后复位 `userScrolled=false`（发送视为主动回到最新）。
- 由什么证明：`pnpm run check` 无新增 error/warning；App 内手动验证三种行为。
- 哪些情况仍算未完成：顶部滚动自动加载历史分页（跨前后端，另行排期）；虚拟滚动（消息量极大场景）；逐 token 流式平滑跟随（当前为整批追加）。

## Scope

- In：`ChatArea.svelte`（轮次分组 + 等高容器 + 滚动逻辑 + `onscroll` 绑定）。
- Out：后端消息分页；虚拟列表；其他组件；TUI/CLI。

## Facts / Constraints

- 消息刷新为**整批追加**（`sendMessage` 乐观追加 userMsg + assistant 回复；后端 `conversations` 事件整批重拉），非逐 token 流式，故无需区分 `auto`/`smooth` 高频抖动策略。
- `isRunning` 由后端 `runningSessions` 驱动，仅控制"思考中"指示条，不参与滚动判定。
- `.messages` 容器已设 `scroll-behavior: smooth`，自动滚动自带平滑过渡。
- **根因（2026-08-15 浏览器实测确认）**：普通 DOM 列表里最后一条消息下方无内容，`scrollIntoView(block:'start')` 受 `maxScrollTop = scrollHeight - clientHeight` 限制，把目标滚到视口顶部时会越界被 clamp 回底部 → 视觉上"没吸顶、滚到底、看到多轮历史"。Virtuoso/Gemini 之所以能吸顶，是因为其虚拟列表在底部天然有预留空间，最后一条消息有足够下方空间可滚到视口顶部。**修复 = 列表末尾加 `answer-spacer`（55vh / min 240px）提供底部预留空间。**

## 接口契约设计

前端 `ChatArea.svelte`（纯内部状态，无外部接口变更）：

```ts
let userScrolled = $state(false);   // true = 用户上滑离开底部（锁定自动跟随）
let pendingAlignTop = $state(false); // 发送后待对齐顶部
let stickyRound = $state(false);    // 吸顶展示期：问题吸顶后回答到达不跳到底

function handleScroll() { /* 距底 >5px → userScrolled=true；否则 false */ }
function scrollToTopOf(target) { /* 临时覆盖 scroll-behavior:smooth，scrollIntoView block:start */ }
$effect(() => {
  // lastMessageCount 追踪：列表重置则解锁；pendingAlignTop 且新消息 → 吸顶到最后一条 user
  // （底部 spacer 提供空间）；stickyRound 期间收到新消息（回答）→ 回答 block:end 对齐视口底
  // （问题+回答整轮可见）；其余新消息且未上滑 → scrollToNewest()
});
```

模板：消息列表末尾追加 `<div class="answer-spacer">`（`height: 55vh; min-height: 240px`）。

## Open Questions

- [ ] 是否需要"滚动到顶部加载更早历史"（分页）：暂不做，另行排期。
- [ ] 是否需要流式逐 token 平滑跟随：当前后端整批追加，无此需求。

## Restated Understanding

- 我理解当前任务是：把对话列表滚动行为对齐 Gemini —— 核心是"只在用户位于底部时自动跟随，上滑读历史不被打断"，外加"发送问句后对齐视口顶部为回答预留空间"。改动仅限前端 `ChatArea.svelte`，不涉及后端与数据模型。
- 当前核心目标是：交付可用的智能自动跟随 + 问句对齐顶部，不引入依赖，不改组件接口。
- 当前边界是：不做历史分页、不做虚拟滚动、不跨组件。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：对话滚动对齐 Gemini（智能跟随 + 问句对齐顶部）。
- 当前核心目标：ChatArea 内实现锁定跟随状态机与发送对齐顶部，静态验证通过。
- 当前进度：实现完成，`pnpm run check` 无新增错误（既有 5 errors/57 warnings 均在无关文件）。
- 下一步 1：用户 App 内手动验证（上滑读历史不被拽回、发送对齐顶部、回答自动跟随）。
- 下一步 2：按反馈微调阈值/动画。
- 验证方式：`pnpm run check`；App 内手动验证待用户。
- Execution Approval: 已批准（2026-08-15，AskUserQuestion 选择"智能自动滚动 + 问句对齐顶部"）。

## Change Log

- 2026-08-15（修订 3）：**消息按轮分组**（纯前端展示层）——一轮 = 以用户输入为起点、到下一条用户输入前（不含）为止的连续消息；nudge 简报（role=user, body.kind==="nudge"）不作为轮起点并入轮内；每组包一层 `.message-round` 容器，`min-height` = 对话容器可视高度（`viewportH`，ResizeObserver 测量，CSS 百分比在滚动容器内无法解析故内联注入像素值）。移除 `.answer-spacer`（轮次容器自身等高后已提供吸顶底部空间）。吸顶目标仍为最后一条 `.message.user`，分组后实测 `userAlign≈0` 生效。
- 2026-08-15（修订 2）：回答到达时**不再额外滚动**（移除 block:end 对齐）——问题已吸顶在视口顶部，回答自然出现在下方；再滚动会把问题顶出视口、破坏吸顶。回答短则问题+回答都在视口内，回答长时用户按需自行滚动。
- 2026-08-15（修订 1）：浏览器实测确认根因（底部无预留空间 → maxScrollTop clamp → 吸顶失败），改为 `answer-spacer`（55vh/min 240px）提供底部预留 + `scrollToTopOf` 临时覆盖 smooth 后 scrollIntoView(block:'start')。实测发送后新问题 `userAlign≈0`（吸顶成功），`db≈181`（预留空间）。
- 2026-08-15: 初始 micro-spec。决策：仅前端 ChatArea 内部状态机；不引入依赖；对齐 Gemini 交互。

## Validation

- Self-check：实现完成。`ChatArea.svelte` 新增 `handleScroll`（距底判定）、`scrollToTopOf`（临时覆盖 smooth）、`$effect`（lastMessageCount 追踪 + pendingAlignTop 吸顶 + stickyRound 回答到达不滚动 + 未上滑才跟随）、`answer-spacer` 底部预留；`onscroll` 绑定容器。
- Static checks：`pnpm run check` 无 ChatArea 相关 error/warning；既有 5 errors（server 隐式 any、ErrorBanner 等）与 57 warnings 均为存量问题，非本次改动。
- Runtime / Test：Playwright 实测——发送后新问题对齐视口顶部（`userAlign=-3/-4`）、底部预留 `db≈181`、吸顶稳定不跳转；回答到达"不滚动"分支因远程后端在测试实例未返回，待 App 内手动确认。
- Human confirmation：micro-spec 已获用户批准后实现；手动 UI 验证待用户进行。
- 结果汇总：实现已完成，静态验证通过，吸顶经浏览器实测确认；运行时回答到达路径待用户确认。
- 核心目标是否已由证据证明完成：吸顶 + 智能跟随已落地并通过类型检查与浏览器实测；回答到达手感需人工确认。
- 若未完成，当前剩余差距：无代码差距；仅剩 App 内手动 UI 验证。
- 剩余风险：`answer-spacer` 高度为经验值（55vh），若回答极长需用户滚动；`scrollIntoView` 依赖目标消息有 `.user` class，结构变化需同步更新。

## Resume / Handoff

- 当前状态：实现完成，静态验证通过，待 App 内手动 UI 验证。
- 当前卡点：无。
- 下一步唯一动作：用户 App 内验证滚动三种行为与切会话回到底部。
- 下一轮核心目标：如有 UI 细节问题，按反馈微调。
