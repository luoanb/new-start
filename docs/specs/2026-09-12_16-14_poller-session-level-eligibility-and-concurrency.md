# 轮询资格会话化 + 全局并发额度

> 2026-09-12 · standard · 方向已获用户确认（对话内四项决策，见 §1）。**已落地**（见文末「落地结果」）。

## 1. 复述理解

两条要求：

1. **可轮询条件调整**：末轮以工具调用结束（"干到一半"）时，**即使没有可推进的关联课题、甚至完全没有课题**，也算符合进入下一轮的条件。
2. **并发解耦**：在并发限制允许的范围内，A 对话是否在进行，不应阻塞 B 对话。

对话内已确认的四项决策：

| 决策点   | 结论                                                         |
| ----- | ---------------------------------------------------------- |
| 适用范围  | 仅 `assistant` / `system` 模式会话（Chat / Agent 零介入，Agent 自带循环） |
| 硬闸优先级 | `paused` / `waiting_user` 仍是硬闸，**即使末轮有工具调用也排除**            |
| 判定信号  | 持久化会话态标记（非每 tick 实时读末条消息）                                  |
| 并发语义  | **全局**上限 = `poll_parallelism`（默认 2，硬上限 8）                  |

## 2. 现状与问题

### 问题 1：候选集完全由课题撑起

`process_step_request`（`assistant_session.rs`）的候选来自 `topics().list_unfinished()`——`topic_store.rs` 的 SQL 仅排除 `done` / `cancelled`；随后逐条要求 `topic.session_id` 存在并过滤 `skip_polling`（`Paused` / `Cancelled` / `WaitingUser`）。

后果：**没有绑定课题的会话永远进不了候选**；`done` 课题一旦不再被复盘 hook 置为 `WrappingUp` 就彻底退出轮询，末轮的"干到一半"无从续推。

### 问题 2：批次之间独占锁 + 请求丢弃

`spawn_poller_runtime`（`gateway.rs`）用独占 `tokio::sync::Mutex` 作 `step_guard`，并以 `try_lock` 判断：

```rust
let Ok(_permit) = step_guard.try_lock() else {
    // 另一个 step 在跑 → 直接丢弃本次请求（不排队）
    return;
};
```

而 `process_step_request` 末尾 `while tasks.join_next().await.is_some() {}` 要**等整批跑完**才返回。于是 A 的慢轮占住锁期间，后续每个 tick 的 PollAll 都被整批丢弃；B 即使条件已满足、额度尚有余，也只能等 A 整批结束。

批次**内部**本来就是多会话并发（信号量限流），所以阻塞点在批与批之间，不在批内。

### 问题 3：并发额度是"每批"而非"全局"

`Semaphore::new(parallelism)` 每次调用新建（`assistant_session.rs`）。当前全局并发恰好等于 `parallelism`，靠的正是问题 2 的独占锁；**只摘锁不改这里，全局并发会变成** **`parallelism × 批次数`**。

另：去重用的 `session_tracker.get` 在派发前检查，而 `register` 发生在任务内部，批次重叠后存在重复派发的窗口；且 `register` 对同 id 是 `map.insert` 覆盖，二次注册会让旧任务句柄失效（`unregister` 变 no-op → 泄漏）。

## 3. 目标语义

### 3.1 会话级可轮询资格

```
可轮询(会话) := mode ∈ {assistant, system}
              ∧ 绑定课题状态 ∉ {paused, cancelled, waiting_user}
              ∧ ( 绑定课题状态 ∈ {todo, in_progress, wrapping_up}
                  ∨ 末轮以工具调用结束 )
```

* 「末轮以工具调用结束」= 最近一轮有工具声明 / 工具结果，即 `!is_settling_round(outcome)`——复用既有纯函数（`assistant_session.rs`）。

* **无课题会话**：只看末轮标记，标记为真才入候选。

* **收敛**：末轮为收尾轮（无工具声明且无工具结果）→ 标记清零 → 退出候选。等价于把 Agent 驱动"末轮无声明即收敛"的判据，用轮询节奏跨 tick 实现。

* 纯函数 `poll_eligible(topic_status: Option<&TopicStatus>, pending_round: bool) -> bool` 承载上表，便于单测真值表。

### 3.2 标记的持久化

* **位置**：`conversation.extra.assistant.pending_round: bool`。

  * 与 `topic.extra.assistant`（`AssistantTopicState`）对称；**不写进核心** **`SessionState`**，保持核心 `extra.session.state` 只有"选型锚点 + 模型选择"语义（核心是封闭内核，不引入助手域字段）。

  * 旧数据无该键 → 回落 `false`，无需迁移。

* **写入时机**：IP-5 `assistant.round.after`，每轮完成即覆盖：
  `pending_round = !is_settling_round(outcome)`

  * 异常轮（`ctx.outcome.is_none()`）**保持原值**，失败恢复交给既有熔断 / 退避通道。

* 写入方式：`AssistantSession` 已持有 `self.store`，读改写 `Conversation.extra` 时保留其它 extra 键（参照 `set_session_state` 的写法）。

### 3.3 候选收集

两个来源取并集，再统一过滤：

1. `topics().list_unfinished()` → 有 `session_id` 且 `!skip_polling(status)` 的课题所属会话（**现状保留**）。该交集恰为 `{todo, in_progress, wrapping_up}`。
2. 新增轻量扫描 `JsonConversationStore::list_poll_candidates()` → 返回 `(id, mode, pending_round)`；取 `mode ∈ {assistant, system} ∧ pending_round` 的会话。

   * 复用既有 `ConversationLight` 反序列化（`extra` 已解析、消息体在取得首条摘要后整条跳过），**不新增全量解析开销**。

过滤（按序）：命中硬闸的会话剔除（对第 2 来源逐个 `find_by_session_id` 校验课题状态）；已在 `session_tracker` 的会话剔除。

### 3.4 派发与并发（重构）

* `AssistantSession` 新增全局在飞计数 `poll_inflight: AtomicUsize`，额度取既有共享原子 `SharedPollParallelism`（运行时可变，无需重建信号量）。

* `try_reserve() -> Option<PollPermit>`：CAS 抢占，`PollPermit::Drop` 自减。额度耗尽 → 该候选**本轮跳过，下 tick 重试**（不再丢弃请求）。

* 派发顺序：去重/占位 → 占额度 → `spawn`。**占位即注册**：新增 `SessionTracker::register_if_absent(id)`（单锁内 check + insert，原子），把注册提前到派发点，消除批次重叠后的重复派发窗口；任务收尾按既有归属句柄 `unregister`。

* 摘掉 `spawn_poller_runtime` 的独占 `step_guard`；PollAll 批次可重叠，各自 await 自己的任务并广播各自 `touched`（事件语义不变）。

### 3.5 无课题会话的轮次语义

`advance_brief` 当前无绑定课题直接报错。改为**按触发区分**：

| 触发           | 无绑定课题时                                                                                    |
| ------------ | ----------------------------------------------------------------------------------------- |
| `Poller`     | 放行：`model_input` 保持空串、`nudge_persist = false`（不落 nudge）、`reselect` 走默认；靠会话历史（末尾即上轮工具结果）续推 |
| `ManualStep` | 维持报错（文档 assistant §3「手动推进必须已绑定课题」，简报无依据）                                                  |

无课题轮的其余副作用已天然安全：`resolve_bound_topic` 留 `topic_id = None`，`round_review` / `complete_scope` 均"skip: no topic"，`tick_round_counters` 无课题直接返回。

### 3.6 文档反向同步

`docs/pulsar/assistant/index.md` §5 与 `docs/pulsar/topic/index.md` §7 的"自动轮询"一行同步为新语义（候选 = 可推进会话，而非"所有未完成课题"）。两节均**无 🔒 盖章标记**；§8 不变量第 4 条「等待用户的课题不得被轮询推进」与新语义一致，不动。

## 4. 实施步骤

1. `stores/conversation_store.rs`：新增 `list_poll_candidates()`（复用 `ConversationLight`，投影 `id / mode / extra.assistant.pending_round`）。
2. `application/session_tracker.rs`：新增 `register_if_absent()`（单锁内 check + insert，返回既有 `SessionHandle`）。
3. `application/assistant_session.rs`：

   * 新增纯函数 `poll_eligible`；

   * 新增 `collect_poll_candidates()`；

   * 新增 `poll_inflight` + `try_reserve` / `PollPermit`；

   * 重写 `process_step_request` 派发循环（去重占位 → 占额度 → spawn；额度耗尽跳过本轮）；

   * `round_after`（IP-5）写 `extra.assistant.pending_round`；

   * `advance_brief` 按触发区分无课题行为。
4. `application/gateway.rs`：`spawn_poller_runtime` 摘除独占 `step_guard`。
5. 文档同步（§3.6）。

## 5. 验证

**单测**

* `poll_eligible` 真值表：无课题 + `pending=true` → 可轮询；无课题 + `pending=false` → 否；课题 `paused` / `waiting_user` + `pending=true` → 否；课题 `in_progress` → 是；课题 `done` + `pending=false` → 否。

* `register_if_absent`：首次 `Some`、二次 `None`；`unregister` 仅清自己的句柄。

* 标记读写：`pending_round` 落库 / 旧数据无键回落 `false`；保留 `extra` 其它键。

* 无课题 Poller 轮：`advance_brief` 不报错、`model_input` 为空、无 nudge 落库；ManualStep 无课题仍报错。

* 并发：额度 = 1 时 A 占满 → B 本轮不派发；A 释放后 B 于下一 tick 被派发（而非整批丢弃）。

**集成**

* `cargo test --lib` 全绿（含既有 `skip_polling` / `WrappingUp` / 熔断用例）。

* GUI 双会话：A 长轮进行中，B 的轮询照常推进；无课题会话末轮带工具调用 → 下一 tick 续推，收尾轮后自动退出候选。

* 日志（`PHASE_POLLER_*`）核对派发 / 跳过原因（额度耗尽 / 硬闸 / 已在跑）。

## 落地结果

**代码**

| 文件 | 变更 |
|---|---|
| `stores/conversation_store.rs` | 新增 `PollCandidate` 投影 + `list_poll_candidates()` 轻量扫描 + `pending_round()` / `set_pending_round()` 标记读写（键 `extra.assistant.pending_round`） |
| `application/session_tracker.rs` | 新增 `register_if_absent()`（单锁内 check + insert，不动无条件 `register`） |
| `application/assistant_session.rs` | 新增 `poll_eligible()` / `collect_poll_candidates()` / `poll_inflight` + `try_reserve_poll_slot()` + `PollPermit`；重写 `process_step_request` 派发；`round_after` 写标记；`advance_brief` 无课题按触发放行 |
| `application/gateway.rs` | `spawn_poller_runtime` 摘除独占 `step_guard` 与 `try_lock` 丢弃分支 |
| `docs/pulsar/assistant/index.md` §5、`docs/pulsar/topic/index.md` §7 | 轮询语义反向同步 |

**测试**：`cargo test --lib` → **465 passed / 0 failed**，其中新增 7 例：

- `poll_eligible_covers_session_level_qualification`（真值表，含硬闸优先于末轮标记）
- `round_after_marks_pending_round_until_settling`（工具轮置位 → 入候选；收尾轮清零 → 退出候选）
- `advance_brief_allows_topicless_poller_round_only`（无课题轮询放行 / 手动推进报错）
- `poll_quota_is_global_and_released_on_drop`（额度占满即拒、Drop 归还）
- `test_register_if_absent_rejects_duplicate_and_respects_ownership`
- `pending_round_flag_roundtrip_preserves_other_extra_keys`（旧数据回落 + 不破坏其它 extra 键）
- `list_poll_candidates_projects_mode_and_pending_flag`

**未覆盖**：GUI 双会话长轮互不阻塞的端到端验证（需实际运行前端 + 模型调用），留给运行期确认。

