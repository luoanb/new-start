# Lifecycle / 生命周期: topic-mgmt-edge-cases

```yaml
status: completed
result: done
created_at: 2026-08-16 16:33
updated_at: 2026-08-16 16:33
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已批准（Option A），执行已完成
- 当前状态：completed（全部 6 个执行步骤落地并通过验证）
- 当前核心目标：修复课题管理两个边界情况——①需人为介入的 scope 项导致无限轮询；②工具执行后 afterhook 立即关闭课题导致 AI 无法收尾
- 下一步唯一动作：无（本迭代已闭环；如需上线验证可重启 pulsar-app 并跑通 WaitingUser / WrappingUp 两条路径）

## Execution Log / 执行记录

- 1. 2026-08-16 16:33: 创建需求迭代。已确认方案方向：边界 1 采用「全部 blocked 才暂停」粒度；边界 2 采用「延迟关闭 WrappingUp 收尾轮」；先写文档再实现。
- 2. 2026-08-16 16:33: Q1 初判——`WaitingUser` 复用 `Paused`（`extra.assistant.waiting_user` 标志区分），`WrappingUp` 新增枚举成员。需求文档已同步。
- 3. 2026-08-16 16:33: 生成 `technical-plan.md` 初版（Option A：blocked + WrappingUp，等待用户复用 Paused+flag）。
- 4. 2026-08-16 16:33: 用户审阅后变更 Q1——`WaitingUser` 改为独立 `TopicStatus` 枚举成员，不复用 `Paused`。`requirements.md` 与 `technical-plan.md` 已同步更新（PollAll 过滤需显式排除 `WaitingUser`）。
- 5. 2026-08-16 16:33: 执行 Step 1（数据模型）——`models.rs` 增加 `WaitingUser` / `WrappingUp` 枚举成员（serde snake_case 自动读写 `waiting_user` / `wrapping_up`）；`inserts/assistant.complete_scope.md` 裁决契约扩展 `blocked_item_ids`。
- 6. 2026-08-16 16:33: 执行 Step 2（存储层）——`topic_store.rs`：`normalize_scope_items` 保留 `blocked`；`derive_topic_state` 扩展 WaitingUser 推导（全 completed→Done / 无 pending 且含 blocked→WaitingUser / 无 completed→Todo / 其余 InProgress）；新增 `mark_scope_item_blocked` 与 `unblock_scope_items`（Paused 手动暂停课题保持 Paused）；`set_status` 改 `pub`。
- 7. 2026-08-16 16:33: 执行 Step 3（Hook 层）——`assistant_session.rs`：PollAll 跳过清单扩展 WaitingUser（抽纯函数 `skip_polling`）；`before_round` User 分支调用 `release_waiting_user` 解除 blocked 恢复轮询；`complete_scope` 扩展双通道裁决（completed/blocked）+ 前置收尾关闭（WrappingUp 非工具轮→Done）+ 延迟关闭（Done 且工具轮→WrappingUp，抽纯函数 `should_delay_close`）+ Paused/WaitingUser 跳过保护；`build_topic_brief` 细化 blocked 项标记与 WrappingUp 收尾指令。
- 8. 2026-08-16 16:33: 执行 Step 4（前端）——`types.ts` 增加 `waiting_user` / `wrapping_up`；`translations.ts` 中英双份补充新状态文案与 `scopeStatusBlocked`；`TopicPanel.svelte` ACTIVE 数组/状态徽标/blocked 项隐藏完成按钮/CSS；`lib.rs` 与 `net/rpc.rs` 的 `topic_status_filter` 增加两个新状态映射。
- 9. 2026-08-16 16:33: 执行 Step 5（契约回归）——确认 `call_judgement` 对缺失 `blocked_item_ids` 用 `unwrap_or_default` 容错，存量 neuron content 不迁移即可工作。
- 10. 2026-08-16 16:33: 执行 Step 6（测试与检查）——补充单测：`derive_topic_state` 推导矩阵（8 种组合）、`normalize_scope_items` 保留 blocked、`mark_scope_item_blocked`（含 Paused 拒绝）、`unblock_scope_items`（恢复轮询 / 手动 Paused 保持）、`list_unfinished` 含 waiting_user、`skip_polling` / `should_delay_close`、`build_topic_brief` blocked 标记与 WrappingUp 指令。
- 11. 2026-08-16 16:33: 验证结果——`cargo test -p pulsar-app`：**237 passed, 0 failed**（含全部新增用例）；`pnpm check`：本次改动文件无新增 error（TopicPanel 仅 1 条既有 a11y warning），仓库现存 5 个 error 均在 `vite.config.js`（与本迭代无关的既有类型问题，未擅自修复）。
