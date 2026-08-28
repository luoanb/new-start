# 空 scope 课题自动收尾（避免轮询空转）

- 日期：2026-08-28
- 范围：`assistant_session.rs` `complete_scope`（验收 hook）
- 状态：已完成

## 背景 / 问题

课题 `scope_in` 为空（0 个范围项）时，轮询器会**无限空转烧 token**：

1. `derive_topic_state(&[])` 恒返回 `(0, Todo)`——空待办列表推导不出 `Done`（`topic_store.rs`）。
2. `list_unfinished` 仅排除 `done/cancelled`，`skip_polling` 仅跳过 `Paused/Cancelled/WaitingUser`——空 scope 的 Todo 课题每轮都被 Poller 推进。
3. 推进轮中 `revise_topic` / `complete_scope` 均因 `scope_in.is_empty()` 直接跳过，课题状态永不变化。
4. 但主对话 `run_round → call_model` 每轮照常调用模型（简报缓存只省简报生成，不省模型调用）。

空 scope 来源：`revise_topic` 模型裁决把全部项 remove 删光，或 legacy 迁移数据。

## 修复

`complete_scope`（after hook）调整检查顺序与语义：

- 先保留 `Paused / WaitingUser` 跳过（人工介入状态不受影响）。
- **新增**：`scope_in` 为空 → 直接 `set_status(Done)` 收尾并返回，不再跳过。
- 原「空 scope 直接跳过」分支删除。

语义：无待办 = 无活可推进 = 课题完成。收尾后下一轮 Poller 自然跳过（`Done` 不在 `list_unfinished`），空转终止。

调用顺序保障：`revise_topic`（after）先于 `complete_scope`（after）执行，revise 删空全部项后同轮即被验收 hook 收尾。

## 验证

- `cargo test --lib`：418 全绿（含既有状态推导 / skip_polling / WrappingUp 边界测试）。
