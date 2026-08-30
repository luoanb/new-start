# Spec: Hook 干预降频（频率门控 + 合并裁决）

> 背景：Assistant 模式下每条用户消息串行触发最多 4 次裁决模型调用（IP-1 score_feedback + match_topic，IP-5 revise_topic + complete_scope），每次携带全量会话历史，首 token 延迟与 token 成本被放大数倍；且存在裁决异常误杀主轮、异常默认新建课题等激进语义。用户选定策略 2（频率门控）+ 策略 4（合并裁决）组合治理。

## Goal

- 要解决什么问题：裁决 hook 过度干预对话——①User 轮 2 次 IP-1 裁决串行阻塞主回复；②工具轮的中间产物也触发 IP-5 裁决；③已绑定课题后 match_topic 仍每轮全量跑；④4 次裁决各自喂全量历史，token 成本约为主对话 3~5 倍；⑤score_feedback 非法值经 IP-1 fail 策略中止整轮、match_topic 解析失败默认 `create`。
- 验收结果：4 次裁决调用收敛为最多 2 次（合并），且按门控跳过（未绑定课题必跑、已绑定低频复核、IP-5 仅收尾轮）；主轮不再被裁决失败中止；不改动 HookRegistry 注入点契约与核心 5 步流水线。

## Done Contract

- 什么算完成：合并裁决 `assistant_user_round_judgement`（IP-1）与 `assistant_round_review`（IP-5）落地，旧四条裁决函数删除；门控矩阵（绑定/未绑定 × user_rounds × 收尾/工具轮）单测覆盖；既有测试全量适配后 `cargo test --lib` 全绿。
- 由什么证明：单测 + 运行一轮 User 输入的日志（`calling user-round-judgement model` / skip 原因）显示裁决调用次数符合门控预期；账本出现新 `system_type` 记录且降级路径不阻断主轮。
- 哪些情况仍算未完成：裁决失败仍可能上抛 Err 中止 User 轮；工具轮仍触发 IP-5 裁决；每次裁决仍喂全量历史 4 遍。

## Scope

- In:
  - IP-1 合并：score_feedback + match_topic → `user_round_judgement` 单次裁决（User 轮），消费顺序保持 score 先、match 后
  - IP-5 合并：revise_topic + complete_scope → `round_review` 单次裁决，同一次解析先修订后验收
  - 频率门控：IP-1 按 `user_rounds` 门控（复用 `AssistantTopicState.user_rounds`，零新增状态）；IP-5 按收尾轮门控（`RoundOutcome` 的 tool_calls/tool_results 判定）
  - `hook/judgement.rs` 静态清单替换（旧四条移除 + 新两条）+ 合并 JSON Schema + neutral_fallback
  - 神经元层同步（裁决 hook 与系统神经元 1:1，换 system_type 即换神经元）：
    `SYSTEM_PROMPT_SEEDS` 新增 2 条合并种子、`default_behavior_for_system_type` 新增两 type、
    `inserts/` 新增 2 份契约段、`REBOOTSTRAP_SYSTEM_TYPES` 替换为 selector + 2 新 type
  - 前端仅 i18n 新增两个 label key；账本旧记录展示依赖既有回退，不改组件
- Out:
  - HookRegistry 注入点契约（IP-1~IP-5）、fail/ignore 策略梯度、会话切换 reload 机制（[defs.rs:208-230](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L208-L230)）
  - 选型 hook（`assistant.select-neuron`）、compaction hook、简报/轮询机制（`advance_brief` / `BRIEF_EVERY_N_ROUNDS`）
  - 策略 3（裁决旁路化/异步化）、策略 5（写操作确认闸门）、策略 6（运行时开关）——后续独立迭代
  - 账本表结构变更、`hook_judgements` 存量迁移
  - 旧 4 个裁决系统神经元的删除/归档——**选项 A 惰性遗弃**（2026-08-30 用户确认）：
    仅从 `REBOOTSTRAP_SYSTEM_TYPES` 移除，神经元资产留库不触碰；「未引用系统神经元清理」命令为后续独立迭代

## Facts / Constraints

已确认事实（代码）：

- **调用频度**：User 轮 `round_before` 无条件跑 `score_feedback` + `match_topic`（[assistant_session.rs:1145-1151](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1145-L1151)）；`round_after` 三种 trigger（User/ManualStep/Poller）均跑 `revise_topic` + `complete_scope`（[assistant_session.rs:1163-1206](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1163-L1206)）。
- **全量历史 4 遍**：四处 `call_judgement` 均传 `&ctx.messages`（[1347-1360](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1347-L1360)、[1404-1425](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1404-L1425)、[1581-1597](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1581-L1597)、complete_scope 同构）。
- **fail 误伤路径**：score 非法（0 或越界 -5..=5）返回 `Err`（[assistant_session.rs:1376-1380](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1376-L1380)），经 `round_before` 的 `?` 上抛，IP-1 是 fail 策略（[hook/defs.rs:213-230](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L213-L230)）→ **中止用户整轮**。
- **激进默认值**：match_topic action 解析失败 `unwrap_or("create")`（[assistant_session.rs:1428-1431](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L1428-L1431)）——解析坏档反而触发最重副作用。
- **纠偏机制可复用**：`call_judgement` 已内置 A/B/C 纠偏（结构化输出降级链 json_schema→json_object→无约束 + 失败重试 1 次 + 中性降级不 `?` 上抛，[assistant_session.rs:257-289](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L257-L289)）；降级态消费处按 `status == Downgraded` 跳过。
- **门控所需状态已存在**：`AssistantTopicState.user_rounds`（[assistant_session.rs:2003-2005](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L2003-L2005)）由 `apply_round_counter` 在 IP-5 计数（[2214-2223](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L2214-L2223)）；IP-1 时刻读到的是上一轮完成后的累计值（本轮尚未 tick）→ `user_rounds % N == 0` 含首轮（0）语义可直接用。
- **收尾轮判定可行**：`RoundOutcome` 含 `tool_calls: Option<Vec<ToolCall>>` 与 `tool_results: Vec<ToolResultItem>`（[round_types.rs:48-61](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/round_types.rs#L48-L61)）；无工具调用 + 无工具结果 = 收尾轮。
- **注册顺序约束**：IP-1 组内课题路由 hook 必须先于选型 hook 注册（[gateway.rs:400-450](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L400-L450)，`install_hooks` 先注册）；合并裁决替换 `assistant.round.before` 后仍保持该顺序。match_topic 现状不触碰 `ctx.reselect`（全仓 grep 仅 `advance_brief` 设置），合并后同样不碰。
- **神经元 1:1 绑定**：`call_judgement` 经 `ensure_system_neuron(def.system_type)` 懒创建/复用系统神经元（[assistant_session.rs:310-313](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L310-L313)）；神经元资产 = content（内置种子 `SYSTEM_PROMPT_SEEDS` 直落库 + config `neurons.bootstrap.system_prompts.<type>` 覆盖）+ behavior（`Fixed` + 禁工具 + `insert_id` 契约段，[manager.rs:49](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L49)）+ 自有候选池。新 system_type 必须同步种子/behavior/inserts/rebootstrap 清单，否则首次裁决回退 LLM 生成种子且缺契约段。
- **前端回退已存在**：面板 label 解析 `t(def.label)`，未知 system_type 回退 `record.hook_type` 原文（[HookJudgementPanel.svelte:154-158](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/HookJudgementPanel.svelte#L154-L158)）；`HOOK_DEFS` 移除旧条目后存量账本记录仍可展示，仅过滤下拉不再提供旧类型选项。
- **空 scope 收尾语义**（[2026-08-28 micro spec](file:///home/lab/Documents/trae_projects/new-start-wt/docs/micro_specs/2026-08-28_14-00_topic-empty-scope-close.md)）：complete_scope 遇空 scope → `set_status(Done)` 收尾防轮询空转——合并后必须保留，不得退化为「跳过」。

技术约束：

- 裁决语义 = `run_raw_round` 单入口的一种调用形态（模型调用统一收敛），合并裁决继续走 `call_judgement`。
- 真相源原则：裁决副作用（建课/切会话/scope 写库/权重）落库方式不变；合并不改写库路径。
- 既有测试 418+ 全绿为底线；旧四函数删除后其单测同步改造为合并版。

## Open Questions

- [x] Q1 合并组合：**已确认**——IP-1 score+match 合并、IP-5 revise+complete 合并（两组各自消费同一份输入，拆开无收益）。
- [x] Q2 门控状态：**已确认**——复用 `user_rounds` 计数器，不新增字段、不改 `AssistantTopicState` 结构。
- [x] Q3 旧 HOOK_DEFS 条目处置：**已确认**——直接移除（不留 deprecated），前端 label 回退已存在；过滤下拉不再提供旧类型（存量记录在「全部」视图仍可见）。
- [x] Q4 复核频率 N：**已确认默认 3**（`USER_ROUND_JUDGEMENT_EVERY_N_ROUNDS = 3`，常量一处定义可调）。
- [x] Q5 score 非法值处理：**已确认默认 skip + warn**（不 clamp、不 Err）。
- [x] Q6 旧 4 个裁决系统神经元处置：**已确认选项 A（惰性遗弃）**——不删除、不迁移；从 `REBOOTSTRAP_SYSTEM_TYPES` 移除后 rebootstrap 不再触碰；新增 2 个合并神经元经 ensure 懒创建；「未引用系统神经元清理」命令后续迭代。

## Restated Understanding

- 我理解当前任务是：把裁决 hook 从「每轮 4 次全量调用、失败可杀主轮、异常默认建课」收敛为「最多 2 次合并调用 + 频率门控 + 收尾轮门控」，在不改动注入点契约的前提下降低干预度与成本。
- 当前核心目标是：User 轮 IP-1 裁决 = 1 次合并调用且未绑定课题必跑 / 已绑定每 N 轮一次；IP-5 裁决 = 仅收尾轮 1 次合并调用。
- 当前边界是：只动裁决层（`assistant_session.rs` 裁决函数 + `hook/judgement.rs` 清单 + i18n 两个 key）；不动 hook 契约、选型、压缩、轮询、账本表结构。

## 接口契约设计

```rust
// ── 1. hook/judgement.rs：静态清单替换 ──────────────────────────────

/// 新 system_type（账本 hook_type 列取值；旧四值成为存量历史）。
pub const SYSTEM_TYPE_USER_ROUND_JUDGEMENT: &str = "assistant_user_round_judgement";
pub const SYSTEM_TYPE_ROUND_REVIEW: &str = "assistant_round_review";

/// IP-1 合并 schema（score + match 两组字段；strict 兼容：全 required，可选用 null 联合）。
pub const USER_ROUND_JUDGEMENT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "score": { "type": "integer" },                       // -5..=5 且非 0；0/越界 = 不打分
    "action": { "type": "string", "enum": ["switch", "create", "none"] },
    "topic_id": { "type": ["string", "null"] },
    "name": { "type": ["string", "null"] },
    "description": { "type": ["string", "null"] },
    "scope_in": { "type": ["array", "null"], "items": {   // 仅 action=create/switch 目标缺失时使用
      "type": "object",
      "properties": { "goal": { "type": "string" }, "done_contract": { "type": "string" } },
      "required": ["goal", "done_contract"], "additionalProperties": false
    } }
  },
  "required": ["score", "action", "topic_id", "name", "description", "scope_in"],
  "additionalProperties": false
}"#;

/// IP-5 合并 schema（revise + complete 两组字段）。
pub const ROUND_REVIEW_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "reason": { "type": "string" },
    "add_items": { "type": "array", "items": { ... 现 revise_topic add 项结构 ... } },
    "remove_item_ids": { "type": "array", "items": { "type": "string" } },
    "update_items": { "type": "array", "items": { ... 现结构 ... } },
    "completed_item_ids": { "type": "array", "items": { "type": "string" } },
    "blocked_item_ids": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["reason", "add_items", "remove_item_ids", "update_items",
               "completed_item_ids", "blocked_item_ids"],
  "additionalProperties": false
}"#;

/// 中性降级（A 方案兜底语义）：不打分、不动课题、不修订、不验收。
fn fallback_user_round_judgement() -> serde_json::Value {
    json!({ "score": 0, "action": "none", "topic_id": null, "name": null,
            "description": null, "scope_in": null })
}
fn fallback_round_review() -> serde_json::Value {
    json!({ "reason": "", "add_items": [], "remove_item_ids": [], "update_items": [],
            "completed_item_ids": [], "blocked_item_ids": [] })
}

/// HOOK_DEFS：移除旧四条（complete_scope / match_topic / revise_topic / score_feedback），
/// 新增两条（inject_point 分别为 IP-1 / IP-5）；面板下拉与账本 label 数据源自动收拢。
```

```rust
// ── 2. assistant_session.rs：门控 ──────────────────────────────────

/// IP-1 合并裁决复核频率（已绑定课题时每 N 条用户消息一次；未绑定课题必跑）。
const USER_ROUND_JUDGEMENT_EVERY_N_ROUNDS: u64 = 3;

/// IP-1 门控：未绑定课题（含首轮 user_rounds==0）必跑；已绑定按 user_rounds 低频复核。
/// user_rounds 在 IP-1 时刻是上一轮完成后的累计值（本轮未 tick），0 % N == 0 天然含首轮。
fn need_user_round_judgement(topic_bound: bool, user_rounds: u64) -> bool {
    !topic_bound || user_rounds % USER_ROUND_JUDGEMENT_EVERY_N_ROUNDS == 0
}

/// 收尾轮判定：主轮无工具声明且无工具执行结果（模型「说完了」而非「干到一半」）。
/// 工具轮的中间产物不触发 IP-5 裁决；User/ManualStep/Poller 同规则。
fn is_settling_round(outcome: &RoundOutcome) -> bool {
    outcome.tool_calls.is_none() && outcome.tool_results.is_empty()
}
```

```rust
// ── 3. assistant_session.rs：合并裁决函数（替换旧四函数）────────────

/// IP-1 合并裁决：一次模型调用产出 {score, action, ...}，按门控跳过（log + 不落账本）。
/// 消费顺序保持现语义：先 score（打上一个介入区间的神经元），后 action（路由分支原样迁移）。
async fn user_round_judgement(&self, ctx: &mut RoundContext) -> AppResult<()>;
// 流程：
//   1. 门控：topic_bound && !need → tracing::info!(skip) 返回 Ok
//   2. score 侧输入：topic 已绑定时取 interval_neuron_ids(...)，为空则仅消费 action
//   3. call_judgement（合并 schema + 中性降级），payload = { user_input, current_session_id,
//      topics(unfinished), neuron_ids, topic_bound }
//   4. 消费 score：仅 topic_bound && score ∈ 1..=5 时 apply_score_feedback；
//      score==0（含降级）或越界 → warn + skip，不 Err（消除 IP-1 fail 误伤）
//   5. 消费 action：switch/create/none 分支逻辑自现 match_topic 原样迁移
//      （含 switch 目标缺失 → create 兜底、emergency scope、会话切换 reload）；
//      仅 action 字面合法时执行，解析异常由 call_judgement 降级兜底为 none，不再 unwrap_or("create")

/// IP-5 合并裁决：一次模型调用产出 {reason, 修订..., 验收...}，先改内容后验收（同轮生效）。
async fn round_review(&self, ctx: &RoundContext) -> AppResult<()>;
// 流程：
//   1. 前置跳过（保持现语义）：无 topic / topic 缺失 → Ok；Paused | WaitingUser → 全跳
//   2. 空 scope：set_status(Done) 收尾（保留 2026-08-28 空 scope 防空转语义），不调模型
//   3. call_judgement（合并 schema），payload = { topic_id, scope_in, model_output,
//      tool_results, user_input, trigger }
//   4. 消费顺序：先修订（add/remove/update + completed 门禁仅 User 轮）后验收
//      （completed/blocked 勾选）；留痕 revision_log 合并为一条事件（含两部分摘要）
//   5. 降级（全空 diff）无副作用

// ── 4. round_before / round_after 装配（AssistantHooks）────────────
// round_before User 分支：release_waiting_user → user_round_judgement（advance_brief 分支不变）
// round_after：outcome 缺失 → Ok；!is_settling_round → log skip + 仅 tick 计数器；
//   否则 round_review；trigger 错误语义保持（User/ManualStep `?` 上抛，Poller log-only）
```

```typescript
// ── 5. 前端 i18n（translations.ts，hook 节新增两 key，en/zh）────────
hook: {
  // 旧四 key 保留（存量账本记录经 system_type 回退已可展示，key 留置无害）；
  userRoundJudgement: "User Round Judgement" / "用户轮裁决",
  roundReview: "Round Review" / "轮次复盘",
}
```

```rust
// ── 6. 神经元层同步（新 system_type 的资产齐备，缺失则首次裁决回退 LLM 生成）──

// neuron/config.rs：SYSTEM_PROMPT_SEEDS 新增 2 条合并种子
//   ("assistant_user_round_judgement", "...")   // score+match 合并文案（角色/判定准则/步骤/输出契约齐备，200–800 字）
//   ("assistant_round_review", "...")           // revise+complete 合并文案（同上）
// config 覆盖键自然生效：neurons.bootstrap.system_prompts.<新type>（旧 4 键失效，文档写明不做迁移）

// neuron/manager.rs：default_behavior_for_system_type 新增两分支
//   assistant_user_round_judgement → Fixed + ToolPolicy::None + insert_id "assistant.user_round_judgement"
//   assistant_round_review         → Fixed + ToolPolicy::None + insert_id "assistant.round_review"

// inserts/ 新增两份契约段（wire 附加，与 content 互补不重复）：
//   inserts/assistant.user_round_judgement.md
//   inserts/assistant.round_review.md

// neuron/manager.rs：REBOOTSTRAP_SYSTEM_TYPES 替换为
//   [ASSISTANT_SELECT_NEURON, "assistant_user_round_judgement", "assistant_round_review"]
// 旧 4 type 移出清单（选项 A：神经元留库，rebootstrap 不再触碰）
```

## 实现步骤

1. **清单与契约**：`hook/judgement.rs` 移除旧四条 + 新增两 schema/两 fallback/两条 HookDef；i18n 补 key；神经元层同步（种子 2 条 + 默认 behavior + inserts 契约段 2 份 + REBOOTSTRAP 清单替换）；文档回写 `docs/pulsar/neuron-init.md` / `storage.md` 的 system_type 清单。既有引用旧四函数的测试标记改造。
2. **合并裁决**：实现 `user_round_judgement` / `round_review`（门控 + call_judgement + 消费迁移），替换 `round_before` / `round_after` 装配；删除旧四函数；相关单测重写为合并版。
3. **门控矩阵单测**：未绑定必跑 / 已绑定 skip 与复核 / 首轮（user_rounds==0）/ 收尾轮与工具轮 / 降级中性 / score 非法 skip 不 Err / 空 scope 收尾 / 神经元层（新 type 种子命中零模型调用、默认 behavior 含新 insert_id、rebootstrap 清单含 3 项）。

## 测试计划

- `cargo test --lib` 全量回归（底线 418+ 全绿）。
- 新增单测：
  - `need_user_round_judgement`：`(false, _) → true`；`(true, 0/3/6) → true`；`(true, 1/2) → false`
  - `is_settling_round`：tool_calls Some → false；tool_results 非空 → false；两者皆无 → true
  - `user_round_judgement` 消费：score+switch 同时生效；score 非法仅 skip 打分、action 照常；Downgraded → 无副作用
  - `round_review` 消费：Paused/WaitingUser 全跳；空 scope → Done 不调模型；completed 门禁仅 User 轮放行；修订+验收同轮先后生效
  - 神经元层：`ensure_system_neuron(新 type)` content == 内置种子（零模型调用，fake model caller 断言）；behavior.insert_id 为新契约段；`REBOOTSTRAP_SYSTEM_TYPES` == selector + 2 新 type
- 人工验证：跑一轮 User 输入，日志确认裁决调用次数与门控一致；账本面板出现新 system_type 且旧记录展示正常。

## 风险与回滚

- 合并 schema 字段更多，解析失败率可能上升 → 既有 A/B/C 纠偏兜底（重试 1 次 + 中性降级），降级不阻断主轮；`attempts_detail` 全量留痕可观测。
- 课题切换滞后（最多 N-1 条消息）与打分降频是**预期行为变化**，需产品确认接受（见下）。
- 回滚：单点 revert 即恢复旧四函数路径；账本新 system_type 记录为增量数据，回滚后旧代码按未知类型回退展示，无迁移负担。

## 行为变化清单（批准即确认）

1. 已绑定课题的会话，跨话题输入不再立即切课题，延迟到下一次复核轮（≤ N-1 条消息）。
2. 打分降频为每 N 条用户消息一次，神经元权重更新变慢。
3. 工具轮（及一切非收尾轮）不再触发 IP-5 裁决；推进轮同样适用收尾门控。
4. score 非法值不再中止 User 轮（warn + skip）；match 解析异常默认 `none`，不再默认 `create`。
5. IP-5 留痕合并为单条 revision_log 事件（含修订与验收两部分摘要）。
6. 旧 4 个裁决系统神经元成为静止资产（画布可见、不再被调用、rebootstrap 不重建）；新合并神经元在首次裁决时懒创建（内置种子直落库）。
7. config 覆盖键 `neurons.bootstrap.system_prompts.assistant_{match_topic,complete_scope,score_feedback,revise_topic}` 失效，如有自定义提示词需按新 type 重新配置。

## Change Log

- 2026-08-30：实现完成。
  - `hook/judgement.rs`：旧四条 HookDef（score_feedback / match_topic / revise_topic / complete_scope）移除，新增 `assistant_user_round_judgement` / `assistant_round_review` 两条（inject_point 分别 IP-1 AfterLoadContext / IP-5 AfterPersistOutcome，均带 JSON Schema + neutral_fallback）。
  - `assistant_session.rs`：旧四函数删除，新增 `user_round_judgement`（门控 + score/action 消费，score 非法 skip 不 Err，action 解析异常兜底 none）与 `round_review`（收尾轮门控 + 空 scope 收尾 + WrappingUp 关闭 + 先修订后验收 + completed 仅 User 轮门禁）；`round_before` / `round_after` 装配替换。
  - 门控常量：`USER_ROUND_JUDGEMENT_EVERY_N_ROUNDS = 3`、`is_settling_round`（无 tool_calls 且无 tool_results）。
  - 神经元层：`SYSTEM_PROMPT_SEEDS` 新增 2 条种子；`default_behavior_for_system_type` 新增 2 type（含旧 4 type 兼容分支）；`REBOOTSTRAP_SYSTEM_TYPES` = selector + 2 新 type（选项 A 惰性遗弃）；`inserts/assistant.user_round_judgement.md` / `assistant.round_review.md` 新建。
  - 前端：i18n `hook.userRoundJudgement` / `hook.roundReview` 新增（en/zh）；账本 label 经既有 system_type 回退展示存量记录。
  - 活文档回写：`docs/pulsar/` 下 architecture / logging / assistant-prompt-synthesis / storage / neuron-init / model-call-sites / session-message-architecture 七篇替换为新 hook 语义（含调用次数 5 → 最多 4、rebootstrap 清单、惰性弃用注记）。历史 specs / micro_specs / sdd-lab 不回溯修改。

## Validation

- `cargo check --lib`：通过（仅 3 个既有 unused import warning，与本迭代无关）。
- `cargo test --lib`：**428 passed / 0 failed**，含门控矩阵新增单测（`need_user_round_judgement` / `is_settling_round` / 合并消费 / 降级中性 / revision 门禁 / 神经元层种子与 rebootstrap 清单）。
- 全仓 grep：源码无旧 `SYSTEM_TYPE_SCORE/MATCH/REVISE/COMPLETE` 常量引用；`docs/pulsar/` 活文档无旧 hook 语义残留（`apply_score_feedback` 为共享打分函数名，非 hook，保留）。
- 待人工验证（运行期）：跑一轮 User 输入观察日志 `calling user-round-judgement model` / `skip: judgement gated` / `skip: not a settling round`，确认调用次数符合门控；账本面板出现新 system_type。
