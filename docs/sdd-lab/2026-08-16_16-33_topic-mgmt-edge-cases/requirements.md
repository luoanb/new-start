# Requirements / 需求文档: topic-mgmt-edge-cases

## Restated Understanding / 需求复述

- 我理解当前需求是：优化课题（Topic）管理，修复两个边界情况：
  1. **需人为介入的 scope 项导致无限轮询**：当某项（如需要用户确认方案、提供资料）必须人工介入时，模型无法标记"等待用户"，该项永远 `pending`，课题永远 `InProgress`，Poller 每轮都推进空转，烧 token 且课题永不完成。
  2. **工具执行后课题被立即关闭，AI 无法收尾**：单轮结构为"一次模型调用 + 执行全部工具"，工具执行后模型在本轮没有机会产出最终总结；afterhook `complete_scope` 在工具轮把最后一项勾选后，`derive_topic_state` 立即置 `Done`，课题被 `list_unfinished` 排除，轮询停止，AI 永远没机会写收尾总结。
- 当前核心目标是：让"等待用户"成为 scope 项的一等状态（不无限空转），并让课题在全部完成后保留一轮收尾机会再关闭。
- 当前边界是：改动集中在 Rust 后端 hook 层（`assistant_session.rs`）、数据模型（`models.rs`）、存储状态推导（`topic_store.rs`）、裁决提示词（`inserts/assistant.complete_scope.md`）及前端课题展示（`TopicPanel.svelte` / `translations.ts`）。
- 暂不处理：无进展自动暂停兜底（用户未选择）；executor 工具后二次模型调用（改动 round 引擎核心，用户未选择）；其他课题管理功能。

## Scope / 范围

### In

1. **数据模型**
   - `ScopeInItem.status` 增加 `blocked`（等待用户）态，与现有 `pending` / `completed` 并列。
   - `TopicStatus` 增加两个状态成员：`WaitingUser`（全部非 completed 项均 blocked，等待用户介入）与 `WrappingUp`（scope 已 100% 完成但最后轮以工具调用结束，等待收尾总结）。

2. **complete_scope 裁决契约（`assistant.complete_scope.md`）**
   - 模型输出从仅 `completed_item_ids` 扩展为 `completed_item_ids` + `blocked_item_ids`（均为已有 scope 项的 id 数组，不得编造）。
   - 判定依据：`done_contract` 被本轮证据满足 → `completed_item_ids`；需要用户提供信息/确认/批准才能继续 → `blocked_item_ids`。

3. **blocked 项处理（边界 1）**
   - 有 blocked 项时：该项计入课题状态推导（`derive_topic_state`），但只有 **全部** scope 项都 blocked（即无未完成非 blocked 项）时课题状态才为 `WaitingUser`；部分 blocked 时课题保持 `InProgress` 并继续被轮询。
   - `WaitingUser` 课题被 PollAll 过滤**显式跳过**（跳过清单扩展；`list_unfinished` 的 SQL 天然包含 waiting_user，必须在此排除，否则仍会轮询空转）。
   - 用户下一次 `converse`（User 轮）时，before hook 自动解除 blocked（恢复 `pending`）并重 derive 恢复课题为可轮询状态。
   - 课题简报（`build_topic_brief`）将 blocked 项标记为"等待用户，勿选"，模型在部分 blocked 时只推进未阻塞项。

4. **延迟关闭收尾轮（边界 2）**
   - after_round 的 `complete_scope` 增加关闭判断：本轮 **以工具调用结束** 时，即使 scope 已 100% 完成，课题置 `WrappingUp` 而非 `Done`，保持被轮询（复用现有 `last_message_is_tool_result` 能力）。
   - `WrappingUp` 课题的下一轮（Poller / ManualStep）：简报变为"所有事项已完成，请输出最终总结并复核"，模型以文本收尾（无工具调用）。
   - 本轮 **非工具结束** 且 scope 100% 完成 → 课题置 `Done` 关闭。
   - `WrappingUp` 状态仅作为轮询期间过渡态，不允许永久滞留（见验收标准 4）。

5. **轮询调度（`process_step_request` PollAll 过滤）**
   - 跳过状态清单扩展为：`Paused` / `Cancelled` / `WaitingUser`；`WrappingUp` 必须被轮询。

6. **前端展示**
   - `TopicPanel.svelte` 状态筛选与状态标签支持新增的 `WaitingUser` / `WrappingUp`；scope item 支持展示 `blocked` 状态标签。
   - `translations.ts` 补充对应中英文文案。

### Out

- 连续无进展自动暂停课题的兜底机制。
- executor 单轮内工具执行后追加第二次模型调用（agent-loop 语义）。
- Topic 统计分析、批量操作等其他课题管理能力。
- TUI（ratatui）侧的状态展示改动。

## User Interaction / 用户交互

- 触发入口：现有 Topic 面板 + Assistant 轮询流程，无新入口。
- 用户操作路径（边界 1）：
  1. AI 在执行某 scope 项时判断需要用户介入 → 模型在 `complete_scope` 裁决中将其 id 放入 `blocked_item_ids` → 该项标记 `blocked`。
  2. 若全部项 blocked → 课题状态变为 `WaitingUser`，Poller 停止推进该课题，界面展示"等待用户"标签。
  3. 用户在会话中提供所需信息（正常对话）→ before hook 自动解除 blocked → 课题恢复轮询，AI 继续推进。
- 用户操作路径（边界 2）：
  1. AI 完成最后一项（通常经工具调用）→ 课题进入 `WrappingUp`，界面展示收尾中标签。
  2. 下一轮 AI 输出最终总结（无工具调用）→ 课题置 `Done` 关闭，界面展示已完成。
- 系统反馈：课题状态 / scope 状态变化经现有 `StateChange` 事件广播刷新前端。
- 状态变化：
  - scope 项：`pending` ↔ `completed`；`pending` ↔ `blocked`（blocked 只能由模型裁决写入，由用户接入解除）。
  - 课题：`InProgress` ↔ `WaitingUser`（全部 blocked / 用户接入解除）；`InProgress`/`WrappingUp` → `Done`（100% + 非工具轮收尾）。
- 异常/边界交互：
  - blocked 与 completed 同时存在：以 blocked 为准标记"等待用户"，但不进入 `WaitingUser`（仍有 pending 项），继续轮询推进其余项。
  - `WrappingUp` 状态下模型又调用了工具：保持 `WrappingUp`，下一轮继续给收尾机会，不提前关闭。
  - 用户手动 `pause_topic` 与 `WaitingUser` 互不影响：手动暂停置 `Paused`，不被自动恢复；`WaitingUser` 在用户接入后自动解除。
- 不应发生的交互：
  - `WaitingUser` 课题继续被 Poller 空转推进。
  - 工具轮结束时课题直接 `Done` 关闭（AI 无收尾机会）。
  - blocked 项被简报当作普通待办让模型继续推进。

## Acceptance Criteria / 验收标准

1. **blocked 标记与解除**
   - [ ] `complete_scope` 裁决返回 `blocked_item_ids` 后，对应 scope 项状态变为 `blocked`。
   - [ ] 用户下一次 User 轮后，所有 `blocked` 项恢复为 `pending`，课题状态从 `WaitingUser` 恢复为可轮询状态。

2. **WaitingUser 与轮询跳过**
   - [ ] 全部 scope 项均为 `blocked`（或无未完成非 blocked 项）时课题状态为 `WaitingUser`。
   - [ ] `process_step_request(PollAll)` 对 `WaitingUser` 课题显式跳过，不发起 step_poller。
   - [ ] 部分 blocked（仍有 pending 项）时课题保持 `InProgress`，轮询继续，简报中 blocked 项带"等待用户，勿选"标记。

3. **延迟关闭收尾轮**
   - [ ] scope 100% 完成且本轮以工具调用结束时，课题状态为 `WrappingUp`（而非 `Done`），继续被轮询。
   - [ ] `WrappingUp` 课题的下一轮简报包含"所有事项已完成，请输出最终总结并复核"。
   - [ ] 非工具轮（模型已正常文本收尾）且 scope 100% 时课题置 `Done`。

4. **WrappingUp 不滞留**
   - [ ] `WrappingUp` 状态在有限轮数内收敛：模型在收尾轮输出总结（无工具调用）即关闭；若模型异常持续调用工具，通过现有 round 计数或前端手动操作仍可收敛（不要求新增自动兜底，但不得无限滞留不可达）。

5. **前端展示**
   - [ ] Topic 面板状态筛选与标签支持 `WaitingUser` / `WrappingUp`；scope item 支持 `blocked` 标签展示。
   - [ ] 中英文文案（`translations.ts`）覆盖新增状态。

6. **回归**
   - [ ] 既有路径不回归：正常轮询推进、`complete_scope` 勾选完成、暂停/恢复/取消课题、用户主对话课题匹配/创建/切换。

## Constraints / 约束

- 业务约束：
  - `Spec is Truth`：文档与代码冲突时，先修正文档再修代码。
  - blocked 只能由模型裁决写入、由用户接入解除，不允许模型自行解除（避免空转自嗨）。
- 技术约束：
  - 状态推导集中在 `topic_store.rs` 的 `derive_topic_state`，课题状态由 scope 状态推导，不散落多处。推导逻辑需扩展：新增 blocked 计数，全部非 completed 项均为 blocked 时产出 `WaitingUser`（现有函数返回 `(progress, TopicStatus)`，需调整返回值或调用方处理新状态）。
  - `list_unfinished` 的 SQL（`status NOT IN ('done','cancelled')`）天然包含 `waiting_user`，必须在 `process_step_request` 的 PollAll 过滤中**显式排除** `WaitingUser`，否则等待用户课题仍被轮询。
  - 复用现有 `last_message_is_tool_result` 能力判断"以工具调用结束"，不新增 round 引擎改动。
  - 前端类型定义对齐 Rust `Topic` / `ScopeInItem` / `TopicStatus` 结构；不引入状态管理库。
  - 旧数据兼容：存量 scope 项 / 课题无新状态时行为与现状一致（serde default 回退）。
- 时间/兼容性约束：
  - 裁决提示词变更（`assistant.complete_scope.md`）需要与存量 neuron content 兼容：模型未返回新字段时按空处理（`unwrap_or_default`），不破坏现有勾选路径。

## Open Questions / 开放问题

- [x] Q1 `WaitingUser` / `WrappingUp` 是新增 `TopicStatus` 枚举成员，还是复用 `Paused` + `extra` 标志？
  - 状态：已关闭（用户 2026-08-16 最终确认）：`WaitingUser` 与 `WrappingUp` 均新增为 `TopicStatus` 枚举成员，不复用 `Paused`。

## Requirement Decisions / 需求决策

- 2026-08-16 16:33:
  - 决策：边界 1 采用「全部 blocked 才暂停课题；部分 blocked 时继续轮询，简报标记 blocked 项等待用户」；边界 2 采用「延迟关闭，引入 WrappingUp 收尾轮，工具轮不关课题」；先写文档再实现。
  - 原因：保持 AI 自动性（不因单项阻塞停摆），同时避免空转；工具轮后给 AI 补一次收尾机会，弥补单轮结构"无二次总结"的缺口。
- 2026-08-16 16:33:
  - 决策：Q1 初判——"等待用户"课题复用 `Paused` + `extra.assistant.waiting_user` 标志；`WrappingUp` 作为新增 `TopicStatus` 枚举成员。
  - 原因：避免枚举膨胀与前端筛选/存储迁移成本。
- 2026-08-16 16:33:
  - 决策：Q1 变更——`WaitingUser` 也作为独立 `TopicStatus` 枚举成员，不复用 `Paused`。
  - 原因：语义更清晰，无需 flag 区分，消除"用户手动暂停被误恢复"风险；代价是 `list_unfinished` 天然包含 waiting_user，需在 PollAll 过滤显式排除（已在约束与验收标准中固化）。
