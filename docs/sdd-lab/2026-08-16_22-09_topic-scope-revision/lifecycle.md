# Lifecycle / 生命周期: topic-scope-revision

```yaml
status: completed
result: done
created_at: 2026-08-16 22:09
updated_at: 2026-08-16 22:38
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已批准（Option A），执行已完成
- 当前状态：completed（全部 6 个执行步骤落地并通过验证）
- 当前核心目标：新增 `revise_topic` 步骤，让课题 scope 在推进过程中可被增删改（含契约文本），自动重算状态并留痕
- 下一步唯一动作：无（本迭代已闭环；如需上线验证可重启 pulsar-app 并跑通「用户对话中变更课题范围」路径）

## Execution Log / 执行记录

- 1. 2026-08-16 22:09: 创建迭代。需求方向经用户三轮确认：①变更通道采用 `revise_topic` afterhook 步骤（平行于 complete_scope）；②变更范围 add + remove + edit 全覆盖（completed 项保护）；③AI 可主动修订 scope（含 Poller 轮，仅限 pending 项）；④迭代命名 topic-scope-revision。
- 2. 2026-08-16 22:09: 需求文档经用户审阅批准（draft → planned）。读取代码现状（assistant_session.rs / topic_store.rs / models.rs / neuron/manager.rs / insert_catalog.rs / creation.rs / round_types.rs / conversation_runner.rs），开始生成技术方案。
- 3. 2026-08-16 22:09: 生成 `technical-plan.md`（Option A：revise_topic afterhook + update_scope_item + 触发类型门禁 + extra.revisions 留痕）。用户回答 Open Questions：Q1 编辑 completed 项自动重置 pending 重新验收；Q2 采用触发类型门禁。requirements.md 已同步（保护规则 / 职责边界 / 验收标准 / 约束 / 决策）。
- 4. 2026-08-16 22:20: 技术方案经用户批准（planned → executing）。
- 5. 2026-08-16 22:22: 执行 Step 1（裁决契约）——新增 `inserts/assistant.revise_topic.md`：结构化 diff（add_items / remove_item_ids / update_items / reason）+ 变更依据（用户显式要求优先、AI 修订仅限 pending/blocked、completed 仅 User 轮）+ 忌用（不得改状态勾选、不得为美化进度改契约）+ 注意（先于 complete_scope 执行）。
- 6. 2026-08-16 22:24: 执行 Step 2（存储层）——`topic_store.rs` 新增 `update_scope_item`：trim 校验非空、至少一个非空字段、复用 `mutate_scope`（事务 + Paused 拒绝 + 重算）、编辑 completed 项自动重置 pending。
- 7. 2026-08-16 22:30: 执行 Step 3（Hook 层）——`assistant_session.rs`：新增常量 `SYSTEM_TYPE_REVISE_TOPIC`；新增 `revise_topic` hook（守卫：无 topic / 空 scope / Paused / WaitingUser 跳过；payload 含 trigger；`call_judgement` 裁决；逐项容错应用；completed 门禁 Poller/ManualStep 跳过记 skipped_ids）；裁决解析抽纯函数 `parse_scope_revision` / `RevisionPlan`（可单测）；新增 `append_revision_log` 维护 `extra.revisions` 数组与 `now_ms`；`after_round` 中 revise 先于 complete_scope 执行，错误处理与 complete_scope 一致（User/ManualStep 传播、Poller 仅记录）。
- 8. 2026-08-16 22:32: 执行 Step 4（系统神经元登记）——`neuron/manager.rs`：`default_behavior_for_system_type` 增加 `assistant_revise_topic` → `assistant.revise_topic`；`REBOOTSTRAP_SYSTEM_TYPES` 追加 `assistant_revise_topic`。
- 9. 2026-08-16 22:36: 执行 Step 5（测试与检查）——补充单测：`topic_store.rs` `update_scope_item` 4 例（字段编辑保持进度 / completed 重置 pending / 至少一个字段 / 项不存在）；`assistant_session.rs` `parse_scope_revision` 6 例（完整 diff 字段过滤 / 空 diff 无副作用 / completed 门禁 Poller 跳过 User 放行 / pending 任意轮可改 / 全空字段仍进计划 / 脏 id 忽略）；`insert_catalog.rs` 两用例追加 `assistant.revise_topic`。修正 2 处编译错误（entry 类型标注、测试 helper 生命周期）。
- 10. 2026-08-16 22:38: 验证结果——`cargo test -p pulsar-app`：**250 passed, 0 failed**（含全部新增用例）。本次零前端改动，跳过前端检查。
