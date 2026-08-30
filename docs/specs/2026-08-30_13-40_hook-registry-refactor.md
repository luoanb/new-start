# Hook 注册式重构（instances/ + ACTIVE_HOOKS）

> 2026-08-30 · standard · 用户已批准方案方向（对话内明确：「按这个方案开始重构」）。
> 上游迭代：`2026-08-30_10-30_hook-gating-merged-judgement.md`（合并裁决已落地）。

## 复述理解

用户两点诉求：① hook 调整应是**注册式**——旧 hook 代码保留、只是不注册（与神经元「惰性弃用」同一哲学）；② hook 定义**单独管理**——目前一个 hook 散在 judgement.rs / assistant_session.rs / inserts / config.rs / i18n 五处。

## 目标结构

```
core/hook/
  registry.rs               # HookInstance{def,run} + ACTIVE_HOOKS（启用清单）+ LEGACY_HOOKS（休眠清单）
  instances/
    mod.rs                  # 模块声明 + SYSTEM_TYPE_* 常量 re-export
    user_round_judgement.rs # 内聚：常量+schema+fallback+DEF+run 实现
    round_review.rs
    score_feedback.rs       # 休眠（#[allow(dead_code)]，不在 ACTIVE_HOOKS）
    match_topic.rs          # 休眠
    revise_topic.rs         # 休眠
    complete_scope.rs       # 休眠
  judgement.rs              # 收缩为共享类型：HookDef/JudgementStatus/JudgementOutcome/hook_def()/hook_defs_meta()
```

- `HookRunFn = for<'a> fn(&'a AssistantHooks<'a>, &'a mut RoundContext) -> BoxFuture<'a, AppResult<()>>`（复用 `hook::defs::BoxFuture`）。
- `hook_def()` / `hook_defs_meta()` 只查 `ACTIVE_HOOKS`（裁决执行与面板语义不变）。
- `round_before` / `round_after` 编排**中立化**：模式门控 + resolve/release/advance_brief/tick 计数留在编排层，hook 本体按 `def.inject_point` 遍历 ACTIVE_HOOKS 执行；**门控下沉**（`is_settling_round` 门移入 round_review 的 run），保证把 legacy 实例移回 ACTIVE 时语义忠实（旧行为每轮跑 revise/complete）。

## 旧 4 hook 完整体眠单元

- inserts `assistant.{score_feedback,match_topic,revise_topic,complete_scope}.md`：**已在盘上**（此前未删），无需恢复。
- 神经元种子（config.rs）与 `default_behavior_for_system_type` legacy 分支：**已在**。
- 需从 HEAD 恢复：4 个 run 实现（`git show HEAD` 已导出 /tmp/old_hooks.rs）+ 4 份 schema/fallback/DEF 元数据（HEAD judgement.rs）。
- 回切方式 = 把实例从 `LEGACY_HOOKS` 移入 `ACTIVE_HOOKS`（一行），不动门控/测试/文档。

## 可见性调整（assistant_session.rs）

`pub(crate)`：`AssistantHooks` + `assistant` 字段；`AssistantSession::{store, topic_store, runner}` 字段与 `topics()`、`call_judgement()` 方法；自由函数 `need_user_round_judgement / is_settling_round / read_assistant_state / interval_neuron_ids / emergency_scope_in / parse_scope_revision / append_revision_log / should_delay_close`；`AssistantHooks::create_bound_topic_from_decision / create_bound_topic_with_scope`。

## 涉及文件

新增：`hook/registry.rs`、`hook/instances/{mod,user_round_judgement,round_review,score_feedback,match_topic,revise_topic,complete_scope}.rs`。
修改：`hook/mod.rs`、`hook/judgement.rs`、`assistant_session.rs`。
不改：inserts、neuron/config.rs、manager.rs、i18n、前端。

## 风险

- HRTB fn 指针（`for<'a> fn(&'a AssistantHooks<'a>, …)`）生命周期推导失败 → 备选：调用点显式构造局部 `AssistantHooks` 再传引用。
- 测试大量依赖 `hook_def` / `call_judgement` 路径 → 保持两者签名不变，路径兼容。

## Done Contract

1. `cargo test --lib` 全绿（现 428 例，数量只增不减）。
2. `ACTIVE_HOOKS` 恰含合并 2 条；`LEGACY_HOOKS` 恰含旧 4 条；各自 system_type 互异。
3. `hook_def()` 对 legacy type 返回 None（面板/执行语义不变），全仓 grep 无已删符号残留。

## Change Log / Validation（2026-08-30 执行后回写）

**已落地，与目标结构的差异（实现期决策）：**

1. `HookRun` 为枚举而非单一 fn 类型：`Before`（IP-1，`&mut RoundContext`）/ `After`
   （IP-5，`&RoundContext`）。原因：round_after 编排链只有 `&RoundContext`
   （`defs::HookHandler::AfterPersistOutcome` 公共契约不改），统一 `&mut` 需动 5 个注入点签名。
2. 可见性超出 spec 清单的补充项（instances 消费所需）：`AssistantTopicState` /
   `RevisionPlan` struct 与字段 `pub(crate)`、`topic_store::now_ms` `pub(crate)`；
   `registry` 模块整体 `pub(crate)`（`HookRun` 签名引用 crate 内部类型，不对外泄露）。
3. `assistant_session.rs` 中两个迁移走的方法体（user_round_judgement / round_review，约 470 行）
   与孤儿 `now_ms()` 已删除；`SYSTEM_TYPE_{USER_ROUND_JUDGEMENT,ROUND_REVIEW}` 常量唯一来源
   迁至 `instances/`，`assistant_session.rs` 顶层 `pub use` 保持旧引用路径不变。

**编排语义（中立化 + 回切忠实）：**

- `round_before`：模式门控 / `resolve_bound_topic` / `release_waiting_user` / `advance_brief`
  留编排层；IP-1 hooks 仅 User 轮遍历执行（与旧 4 hook / 合并裁决的 trigger 语义一致，
  Manual/Poller 轮不会误触裁决）。频率门控保留在 `user_round_judgement::run` 内。
- `round_after`：`outcome` 缺失早退（不 tick，保持旧行为）；IP-5 hooks 每轮遍历执行
  （User/Manual 上抛、Poller 吞错）+ 每轮无条件 tick。`is_settling_round` 门在
  `round_review::run` 内 → 合并版等价原门控行为；回切 legacy revise/complete（无门）
  等价 HEAD 每轮跑的旧行为。

**验证（Done Contract 逐条）：**

1. `cargo test --lib`：**434 passed; 0 failed**（428 → 434，新增 registry 6 例，只增不减）✓
2. `registry` 测试断言：ACTIVE 恰 2 / LEGACY 恰 4 / 全表 system_type 互异 /
   Before↔IP-1、After↔IP-5 一一对应 ✓
3. `hook_def()` legacy 四类返回 None（judgement 测试）；全仓 grep `HOOK_DEFS` 零残留
   （仅 lib.rs 注释措辞已同步）✓
4. `cargo check --lib --tests` 无新增警告（剩余 3 条为 gateway.rs/providers.rs 预存项）✓

**回切方式（备忘）**：把 `instances/<hook>.rs::INSTANCE` 从 `LEGACY_HOOKS` 数组移入
`ACTIVE_HOOKS`（一行），inserts 契约 / 神经元种子 / 测试 / 文档均无需改动。

## Resume or Handoff

无未完成项。后续可选项：前端「hook 类型过滤下拉」如需展示 legacy 类型（当前语义不变，仅展示启用 2 条），另行迭代。
