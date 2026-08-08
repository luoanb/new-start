# Spec: 会话运行状态单一真相源 + 发送防抖锁修复

## Goal

- 修复跨会话状态串扰 bug：A 会话"思考中"期间切换到 B，B 也显示"思考中"且无法发送。
- 架构收敛：会话运行状态（isRunning / 思考中指示器 / 输入禁用）**只**由后端 `runningSessions` 权威驱动；前端 `sendingIds` 降级为**发送按钮防抖锁**（不参与运行状态判定），消除其残留导致的永久"思考中"。

## 根因

1. **sendingIds 残留**（直接原因）：[+page.svelte handleSend](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src/routes/+page.svelte#L107-L129) 的 `finally` 清理使用响应式 `activeConversationId`（读取"当前选中"），而非发起发送时捕获的会话 id。发送期间切换会话 → 清理删错会话 → 原会话条目永久残留 → 该会话永远显示"思考中"且无法发送。
2. **runningSessions 竞态残留**（次要来源）：`register/update_step/unregister` 每个变化都广播 `sessions` 事件，前端每个事件各发一次全量 `invoke("list_running_sessions")`；响应返回乱序时旧快照覆盖新快照，已结束会话仍停留在 `runningSessions`。
3. 架构偏差：`sendingIds` 被当作"运行状态"长期存在，与后端权威 `runningSessions` 职责重叠。

## Done Contract

- `ChatArea.isRunning` 仅由 `runningSessions.find(session_id === activeConversationId)` 驱动，不再读取 `sendingIds`。
- `handleSend` 入口捕获 `conversationId`，防连点判断 / 加入 / finally 清理全部使用同一捕获值；`sendingIds` 语义改为"发送按钮防抖锁"（请求在途期间拦截同会话重复发送）。
- `refreshRunningSessions` 增加版本守卫：只有最后一次发起的刷新结果才写入 `state.runningSessions`，乱序旧响应直接丢弃。
- 不做后端改动（后端 `SessionTracker` 已完整覆盖用户发送 / assistant_step / Poller 三类执行入口，register 时机在命令入口）。
- 验证：`pnpm --filter agent-app check` / `build` 通过。

## 改动点

| 文件 | 改动 |
|---|---|
| `src/lib/components/ChatArea.svelte` | `isRunning` 收敛为纯 `runningSessions` 驱动，删除 `sendingIds` 项与相关注释 |
| `src/routes/+page.svelte` | `handleSend` 入口 `const conversationId = dataStore.state.activeConversationId`；guard/add/finally 全部改用该值；注释改为防连点锁语义 |
| `src/lib/stores/dataStore.svelte.ts` | `refreshRunningSessions` 加版本守卫（递增 seq，只应用最新响应） |
| `src/lib/layout/viewContext.ts` | `ViewUiState.sendingIds` 注释更新为"发送防抖锁，不参与运行状态" |

## 兼容性

- 后端、类型定义、事件协议均不变。
- `sendingIds` 字段保留（防抖锁仍需使用），仅语义与使用方式收敛。
- 运行中指示的毫秒级延迟：点击发送到后端 register 事件回来前，按钮仍可点，但由防连点锁拦截重复提交，不产生重复发送。

## Validation

- `pnpm --filter agent-app check`：0 errors。
- `pnpm --filter agent-app build`：构建成功。
- 手动验证（复现路径）：B 发送 → 切 A 发送 → B 先完成 → 切回 B，B 不再残留"思考中"，可正常发送。

## Change Log / Validation（2026-08-09）

- `pnpm --filter agent-app check`：0 errors（47 warnings 均为既有，与本次改动无关）。
- `pnpm --filter agent-app build`：构建成功。
- 实现摘要：
  - `+page.svelte`：`handleSend` 入口捕获 `conversationId`，防连点判断 / 加入 / finally 清理全部使用同一捕获值；`sendingIds` 语义改为发送按钮防抖锁。
  - `dataStore.svelte.ts`：`refreshRunningSessions` 加版本守卫（`runningSessionsSeq` 递增，只应用最后一次发起的刷新，丢弃乱序旧快照）。
  - `ChatArea.svelte`：`isRunning` 收敛为纯 `runningSessions.find(session_id === activeConversationId)` 驱动，移除 `sendingIds` 参与运行状态判定。
  - `viewContext.ts`：`sendingIds` 注释更新为防抖锁语义。
- 未纳入：`sendMessage` 内乐观消息追加仍读可变 `activeConversationId`（发送中切会话时 A 的回复可能短暂追加到 B 的消息列表，随后被 `conversations` 事件刷新纠正）。属同类残留风险的独立收尾项，留待后续。
