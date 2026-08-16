# Requirements / 需求文档: topic-scope-revision

## Restated Understanding / 需求复述

- 我理解当前需求是：课题（Topic）在推进过程中，其 scope 内容可能中途需要变更——用户补充需求（加项）、取消需求（删项）、调整验收标准（改 goal / done_contract 文本）。当前没有任何模型面入口：`complete_scope` 明确禁止修改契约文本（只勾选 / 阻塞），`match_topic` 只管切换 / 新建课题。上一迭代已闭环的 `blocked` / `WaitingUser` / `WrappingUp` 处理的是「等待用户」与「收尾轮」，不覆盖「内容变更」。
- 当前核心目标是：新增 `revise_topic` 步骤——与 `complete_scope` 平行的 afterhook，插在模型轮之后、`complete_scope` 之前，模型输出结构化 diff（`add_items` / `remove_item_ids` / `update_items` / `reason`），原子应用到 TopicStore，自动重算 progress / status，并将每次变更写入审计日志。
- 当前边界是：改动集中在 Rust 后端（`assistant_session.rs` hook 层、`topic_store.rs` 存储层、`models.rs`、新增 `inserts/assistant.revise_topic.md` 裁决契约）；不触碰 round 引擎；前端仅依赖现有 scope 列表展示（不新增 revision 历史 UI）。
- 暂不处理：已完成条目的重新打开（reopen）；scope 批量替换；revision 历史的回滚接口 / UI；TUI（ratatui）展示。

## Scope / 范围

### In

1. **变更通道：新增 `revise_topic` afterhook 步骤**
   - 与 `complete_scope` 平行的裁决步骤，插在 after_round 的模型轮之后、`complete_scope` 之前（新 scope 立即参与本轮验收）。
   - 所有触发类型（User / ManualStep / Poller）均执行；Poller 轮失败仅记录（与现有 complete_scope 容错一致）。

2. **裁决契约：`inserts/assistant.revise_topic.md`（新文件）**
   - 模型只返回 JSON diff：
     ```json
     {
       "add_items": [{"goal": "…", "done_contract": "…"}],
       "remove_item_ids": ["scope_…"],
       "update_items": [{"id": "scope_…", "goal": "…", "done_contract": "…"}],
       "reason": "用户明确要求增加 X；Y 已不需要"
     }
     ```
   - `add_items` 的 goal / done_contract 均非空；`update_items` 至少携带一个非空字段（goal 或 done_contract），缺省字段保持不变。
   - 未返回任何变更时按空处理（`unwrap_or_default`），兼容存量模型。

3. **存储层：`topic_store.rs`**
   - 复用现有 `add_scope_item` / `delete_scope_item`（`mutate_scope` 事务化、自动重算 progress / status、Paused 拒绝写入）。
   - 新增 `update_scope_item(topic_id, item_id, goal?, done_contract?)`：仅改文本，不改状态；走 `mutate_scope` 复用保护与重算。

4. **保护规则（completed 项）**
   - `pending` / `blocked` 项：AI 可直接 add / remove / edit（含 Poller 轮自主修订，用户已确认）。
   - `completed` 项：仅 **User 触发轮** 允许 edit / remove（触发类型门禁，Poller / ManualStep 轮一律跳过并记入 `skipped_ids` 留痕），且需用户显式要求（裁决提示词纪律 + reason 留痕）。

5. **职责边界：revise 管「内容」，complete_scope 管「状态」**
   - `revise_topic` 只增删改条目与文本，不直接写 `status`；条目状态仍由 `complete_scope`（completed / blocked）与用户接入 `unblock`（pending）控制。
   - **唯一例外**：编辑 completed 项时，该项自动重置为 `pending`（有限 reopen 特例，仅编辑触发——旧契约的验收结论已失效，需按新契约重新验收）；删除不重置。
   - 删除项后由 `derive_topic_state` 自然重算课题状态（如删除全部 blocked 项 → 课题脱离 WaitingUser）。

6. **审计留痕**
   - 每次成功变更将 revision 事件写入 `topic.extra`（时间、变更摘要、reason、触发轮类型），保住 Spec is Truth 与进度可追溯。

## User Interaction / 用户交互

- 触发入口：现有 Assistant 主对话 + 轮询推进流程，无新入口。
- 用户操作路径：
  1. 用户显式要求变更（「顺便把 X 也做了」「Y 不用做了」「Z 的验收标准改成 W」）→ 本轮模型响应后 `revise_topic` 应用 diff，界面 scope 列表即时更新。
  2. AI 主动修订（轮询推进中发现契约过时 / 范围错误）→ 直接修改 pending 项并留痕；completed 项不碰，如需变动转 `blocked` 提问，待用户确认后应用。
  3. 用户要求修改 / 删除某 completed 项 → 该请求作为显式依据，`revise_topic` 应用变更并留痕。
- 系统反馈：变更后经现有 `StateChange` 事件广播刷新前端课题列表与 scope 展示；revision 日志写入 `topic.extra`。
- 状态变化：
  - scope 项：内容（goal / done_contract）变更；条目增删；条目状态字段不受 revise 影响（仍由 complete_scope / unblock 控制）——**唯一例外**：编辑 completed 项时自动重置 `pending`（重新验收）。
  - 课题：progress / status 由 `derive_topic_state` 重算（增删改后分母 / 完成数变化）。
- 异常/边界交互：
  - 同一轮 revise + complete_scope：先 revise 后 complete_scope，新加项可被本轮 complete_scope 勾选。
  - Paused 课题：revise 拒绝写入（`mutate_scope` 既有保护），不产生部分写入。
  - completed 项在 Poller / ManualStep 轮被 edit / remove：一律跳过，记入 `skipped_ids` 留痕。
  - Poller 轮 revise 失败：仅记录日志，不打断轮询推进。
  - remove 导致 scope 为空：课题回退 Todo（现状 `derive_topic_state` 语义）。
- 不应发生的交互：
  - 无依据地静默改写 completed 项契约文本（造成进度与验收历史失真）。
  - revise 与 complete_scope 顺序颠倒（新项无法参与本轮验收）。
  - 变更后 progress / status 与 scope 不一致（必须重算）。

## Acceptance Criteria / 验收标准

1. **revise 契约与应用**
   - [ ] `assistant.revise_topic` 裁决返回 add / remove / update 后，对应变更原子生效；`add_items` 条目以 pending 追加，`remove_item_ids` 条目被删除，`update_items` 仅变更携带字段。
   - [ ] 变更后课题 progress / status 由 `derive_topic_state` 重算正确（增删改后分母 / 完成数变化）。

2. **completed 项保护**
   - [ ] Poller / ManualStep 轮对 completed 项的 edit / remove 一律跳过（触发类型门禁），记入 `skipped_ids` 留痕。
   - [ ] User 轮且用户显式要求时，completed 项可被 edit / remove。
   - [ ] 编辑 completed 项后，该项自动重置为 `pending` 重新验收（有限 reopen 特例）。

3. **AI 主动修订（Poller 轮）**
   - [ ] Poller / ManualStep 轮可对 pending 项执行 add / remove / edit（含契约文本），revise 失败仅记录不打断轮询。

4. **职责边界**
   - [ ] revise 不直接写 `status`；completed / blocked 仍只由 complete_scope 与用户接入 unblock 控制（唯一例外：编辑 completed 项自动重置 pending 重新验收）。

5. **审计留痕**
   - [ ] 每次成功变更写入 `topic.extra` revision 日志（时间 / 变更摘要 / reason / 触发轮类型），可追溯。

6. **顺序与回归**
   - [ ] revise 在模型轮后、complete_scope 前执行；同一轮新加项可被 complete_scope 勾选。
   - [ ] 既有路径不回归：blocked / WaitingUser / WrappingUp、暂停 / 恢复 / 取消、课题匹配 / 创建 / 切换、complete_scope 勾选。

## Constraints / 约束

- 业务约束：
  - `Spec is Truth`：文档与代码冲突时，先修正文档再修代码。
  - 变更必须留痕；completed 项受保护（触发类型门禁：仅 User 轮可动 + 需用户显式要求；编辑自动重置 pending 重新验收）。
  - AI 可主动修订 pending / blocked 项（含 Poller 轮），但不得静默改写 completed 项。
- 技术约束：
  - 状态推导单一真相源 `derive_topic_state`；revise 的写入统一走 `mutate_scope`（事务 + Paused 保护 + 重算），不新开旁路。
  - revise 与 complete_scope 同属 after_round 裁决步骤，复用现有系统神经元 + insert + `call_judgement` 机制，不新增 Tauri 命令、不改 round 引擎。
  - `assistant.revise_topic` 为新 insert，需注册进 InsertCatalog / 系统神经元 / behavior；存量神经元未返回新字段时按空处理，不破坏现有路径。
  - 前端类型无需变更（scope 列表结构与现有展示兼容）。
- 时间/兼容性约束：
  - 兼容存量 neuron content：模型不返回变更字段时行为与现状一致。
  - 旧数据兼容：存量课题无 revision 日志时 `extra` 解析回落（Option 为 None）。

## Referenced Designs / 引用设计稿

> 无。本迭代不涉及 Figma / 视觉稿。

## Open Questions / 开放问题

- [x] Q1 变更通道形态？
  - 状态：已关闭（用户 2026-08-16 22:09 确认）：采用 `revise_topic` afterhook 步骤（平行于 complete_scope）。
- [x] Q2 变更范围做到哪一层？
  - 状态：已关闭（用户 2026-08-16 22:09 确认）：add + remove + edit 全覆盖（含 completed 项保护规则）。
- [x] Q3 AI 提议的变更（用户未明确说）怎么处理？
  - 状态：已关闭（用户 2026-08-16 22:09 确认）：允许 AI 直接改（仅限 pending / blocked 项；completed 项仍需用户显式依据）。
- [x] Q4 轮询 / 手动推进轮（用户不在场）是否也执行 revise_topic？
  - 状态：已关闭（用户 2026-08-16 22:09 确认）：Poller 轮也允许 revise。

## Requirement Decisions / 需求决策

- 2026-08-16 22:09:
  - 决策：新增 `revise_topic` afterhook 裁决步骤，范围覆盖 add + remove + edit，允许 AI 主动修订（Poller 轮也执行），completed 项受保护；每次变更写入审计日志。
  - 原因：推进过程中 scope 必然需要演进；独立步骤避免把「改契约」折叠进「勾选」（防止模型悄悄改契约让进度失真）；留痕保住 Spec is Truth；AI 主动修订保证无人值守推进时范围不僵化。
- 2026-08-16 22:09:
  - 决策：Q1 编辑 completed 项后自动重置 `pending` 重新验收（有限 reopen 特例，仅编辑触发）；Q2 completed 项保护采用触发类型门禁（仅 User 轮可 edit / remove，Poller / ManualStep 一律跳过并留痕）。
  - 原因：旧契约验收结论随契约文本失效，必须重新验证；「用户显式依据」无法程序校验，门禁 + 提示词纪律 + reason 留痕是可落地的确定性边界。
