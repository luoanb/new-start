# Spec: 用户介入轮耗时（仅耗时）

## Goal

- 要解决什么问题：在会话消息区，**该用户消息所属轮次分组（`.message-round`）的底部**显示「模型处理这次输入花了多久」。
- 验收结果：助手模式下，用户介入轮结束后该分组底部显示耗时；进行中显示递增的已用时。

## Done Contract

- 什么算完成：后端在介入轮结束时把耗时写回该 user 消息；前端在该轮分组（`.message-round`）底部渲染。
- 由什么证明：`cargo test --lib` 全绿 + `pnpm --filter pulsar-app check` 0 error + 手动发一条输入看到耗时。
- 哪些情况仍算未完成：只有前端读数、重载后消失（未落库）；或已落库但前端不显示。

## Scope

- In：耗时（单值）；落库；进行中本地 tick。
- Out：结束原因；跨越轮次数；展开明细；聚合/排行；Chat / Agent 模式。

## Facts / Constraints

- `Message` 已有 `timestamp`（该 user 消息落库时刻 ≈ 介入轮起点）；**没有**「结束时刻」字段。
- assistant 单轮 = 一次模型调用 + 本轮工具执行；工具轮结束会置 `pending_round=true`，由 poller 续推下一轮 → **一个介入轮可能跨多轮**。
- `pending_round`（`conversation.extra.assistant.pending_round`）每轮 IP-5 写入；`!pending_round` = 本轮收尾。
- **Nudge 消息也是 `role=User`**（`body.kind=nudge`）；定位「用户输入」必须按 body 区分，不能只看 role。
- `JsonConversationStore::update_message_at(conversation_id, index, patch)` 可按索引改写已落库消息。
- 落库介质为会话 JSON，无迁移框架；新增字段须 `#[serde(default)]` 以兼容旧数据。
- 前端 `ChatArea` 已持有 `runningSessions`（判断该会话是否运行中）；`ChatMessage` 已按 role 分支渲染。

## Open Questions

- [x] Q1 口径：**已定 = 整个介入轮**——含 poller 续推，直到 `!pending_round` 或课题进入终态。
- [x] Q2 进行中是否递增显示：**已定 = 要 tick**——前端按 `timestamp` 本地递增，不新增推送、不新增事件。

## Restated Understanding

- 我理解当前任务是：**只要时间**——一条用户消息触发的处理，墙钟上花了多久。
- 当前核心目标是：让用户一眼看到「我这条消息之后，模型忙了多久」。
- 当前边界是：纯观测；不改轮询节奏 / `pending_round` / 课题状态机；只覆盖助手模式。
- 暂不处理：结束原因、轮次数、明细展开、聚合统计、历史回溯补算。

## 接口契约设计

- Rust 侧（`core/models.rs`）——`elapsed_ms` 三态：

  ```rust
  pub struct Message {
      pub role: MessageRole,
      pub body: MessageBody,
      pub timestamp: u128,
      pub neuron_id: Option<String>,
      /// 介入轮耗时（毫秒，仅写在该轮触发的那条用户消息上）：
      /// `0` = 进行中（前端按 `timestamp` 本地 tick）；`>0` = 已收尾定格；缺失 = 未追踪。
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub elapsed_ms: Option<u64>,
  }
  pub(crate) const USER_TURN_IN_PROGRESS: u64 = 0;
  ```

- 写入（`assistant_session.rs`，会话级公共入口）：

  ```rust
  /// 定位「最后一条真实用户输入」（仅助手模式），按 decide 决定新值；None = 不写盘。
  fn write_user_turn_elapsed(store, session_id, decide: impl FnOnce(&Message) -> Option<u64>);

  /// 未追踪 → 登记进行中（elapsed_ms = 0）。
  pub(crate) fn mark_user_turn_open(store, session_id) -> AppResult<()>;

  /// 未追踪 / 进行中 → 定格墙钟耗时；已收尾不动（幂等）。
  pub(crate) fn stamp_open_user_turn(store, session_id) -> AppResult<()>;
  ```

  - 触发点（全部「失败仅 `warn`」）：
    - IP-5 `round_after` 三支（User / ManualStep / Poller）：**收尾判据 = 轮询不再续推**（`poll_eligible(课题状态, pending_round)`，与轮询候选同一判据）——判为「还会续推」→ `mark_user_turn_open`；判为「不再续推」→ `stamp_open_user_turn`。注意 `Todo` / `InProgress` / `WrappingUp` 课题即使本轮收尾也会继续轮询，**不得**据此收尾。
    - IP-1 `round_before`（仅 User 触发）：新输入取代上一次介入轮 → `stamp_open_user_turn` 定格旧的（此刻新消息尚未落库）。
    - `stop_session` ④：`stamp_open_user_turn` 兜底。
  - **不依赖 `runningSessions`**：`runningSessions` 只在单轮执行期间注册（gateway 一轮一注册、poller 每 tick 注册后立刻注销），轮询等待空档会漏判 → 以 `elapsed_ms = 0` 显式标记进行中，进行中标记贯穿整轮（含轮询等待）。

- 前端侧（`ChatArea.svelte`）：按轮次分组渲染，**放在 `.message-round` 底部**（思考指示之后）；`>0` → 定格显示；`== 0` → 按分组首条（用户输入）的 `timestamp` 本地 tick（`nowMs` 每 1s 驱动，仅存在进行中轮时开定时器）；`缺失 且 该会话运行中 且 是最后一条用户输入` → 兜底 tick（覆盖首轮标记落库前的窗口）；其余不渲染。

## Checkpoint Summary

- 当前任务理解：消息区每条用户消息下方显示该介入轮耗时（口径 = 整个介入轮）。
- 当前核心目标：只要时间，落库 + 进行中 tick。
- 当前进度：方案已定稿（Q1/Q2 已确认），未动代码。
- 下一步 1：`core/models.rs` 加 `elapsed_ms`（27 处 `Message` 字面量补 `elapsed_ms: None`）。
- 下一步 2：`round_after`（User/ManualStep/Poller 三支）收尾时 + `stop_session` 兜底盖耗时（失败仅 warn）。
- 下一步 3：前端 `types.ts` 加字段 + `ChatMessage` 渲染 + 运行中 tick + i18n。
- 下一步 4：`cargo test --lib`、`pnpm --filter pulsar-app check`，回写本 spec。
- 涉及文件 / 模块：`core/models.rs`、`application/assistant_session.rs`、`application/gateway.rs`（`stop_session`）、`src/lib/types.ts`、`ChatMessage.svelte`、`i18n/translations.ts`。
- 风险：收尾判定要用 `pending_round` + 课题终态；Nudge 与用户输入同为 `role=User`，需按 body 区分；`Message` 字面量较多（机械改动）。
- 验证方式：`cargo test --lib`；`pnpm --filter pulsar-app check`；手动发一条带工具的输入观察耗时与收尾。
- Execution Approval: `Approved`（2026-09-13）

## Change Log

- 2026-09-13: 需求收缩为「仅耗时」；撤回此前独立表 + store + 事件 + 命令的重方案，改为在 user 消息上单一字段。
- 2026-09-13: Q1 口径定为「整个介入轮」；Q2 定为「进行中 tick」。
- 2026-09-13: 实现完成——`Message.elapsed_ms`（含 35 处字面量补齐）；`stamp_open_user_turn`（IP-5 收尾 + `stop_session` 兜底，仅助手模式，`elapsed_ms` 非空即已收尾）；前端标签 + 运行中本地 tick + i18n。
- 2026-09-13（修订）：验收发现「进行中只在 runningSessions 内才 tick」→ 轮询等待空档（会话未注册）标签会闪断。改为**显式进行中标记** `elapsed_ms = 0`（IP-5 未收尾时写入），并在 IP-1 User 轮定格被取代的旧轮；前端改为三态判定，`runningSessions` 仅作首轮兜底。
- 2026-09-13（修订）：展示位置从「用户消息气泡下方」移到**轮次分组 `.message-round` 的底部**（思考指示之后）——落到 `ChatArea` 的分组渲染层，`ChatMessage` 不再承载该标签。
- 2026-09-13（修订）：收尾判据修正为 `poll_eligible`。**实测证据**：`conv_1789234193622128355` 最后一轮 02:03:33 发出 → 02:03:46 被定格 `12284`（12s），但日志显示该会话 02:03:59→02:04:48 仍在跑 Poller 轮（真实耗时约 75s）。原因是 `InProgress` 课题 `poll_eligible` 恒为 true，而旧判据 `!pending_round` 提前收尾。新增回归测试 `round_after_keeps_user_turn_open_while_topic_keeps_polling`。

## Validation

- Self-check: 已按方案实现（字段 / IP-5 状态落账 / IP-1 取代定格 / `stop_session` 兜底 / 前端三态渲染 + tick）。
- Static checks: `cargo check --lib --tests` 无 error；`pnpm --filter pulsar-app check` → 0 errors（20 条既有 warning，非本次引入）。
- Runtime / Test: `cargo test --lib` → **487 passed / 0 failed**（新增：`stamp_open_user_turn` 单测 2 条、`round_after` 进行中/定格集成 1 条、`round_after` 课题续推保持进行中 1 条、`round_before` 取代定格 1 条）。
- Human confirmation: 待用户运行应用确认（消息区标签 + 进行中递增，尤其是轮询等待期间不闪断）。
- 结果汇总：自动化证据已齐；端到端人工确认待补。
- 核心目标是否已由证据证明完成：否（差人工运行确认）。
- 若未完成，当前剩余差距：端到端人工验证。
- 剩余风险：进程在介入轮中途崩溃 → 遗留 `elapsed_ms = 0` 会让该条持续 tick（未做启动清理）；如需覆盖再单开一轮。

## Resume / Handoff

- 当前状态：实现完成，自动化验证通过，待人工运行确认。
- 当前卡点：端到端人工确认。
- 下一步唯一动作：启动应用，发一条（最好带工具的）输入，观察该用户消息下方耗时与进行中递增。
- 下一轮核心目标：人工确认通过即收尾；若需覆盖「被抢占的旧介入轮」再单开一轮。
