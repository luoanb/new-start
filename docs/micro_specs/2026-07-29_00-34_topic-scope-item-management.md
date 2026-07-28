# Spec: 课题 Scope 条目管理

## Goal

- 要解决什么问题：禁止课题创建后整体覆盖 `scope_in`，改为按稳定条目 ID 单独新增、删除和完成。
- 验收结果：条目变更始终自动同步课题 `progress` 和活动状态；课题为 `paused` 时不能变更条目。

## Done Contract

- 创建课题时可一次性提供完整 `scope_in`；创建后通用更新入口不再接受 `scope_in` 或 `progress`。
- 新增、删除、完成均按条目稳定 `id` 操作，并在一次业务动作内重算课题进度和状态。
- `paused` 课题的条目变更必须失败，且不得产生部分写入。

## Scope

- In:
  - `ScopeInItem` 增加稳定 `id`。
  - 创建时接收完整条目集合，并为缺少 ID 的条目生成 ID。
  - 创建后的单条新增、删除、完成能力。
  - 条目每次变化后自动重算 `progress/status`。
  - `paused` 状态的写入保护。
- Out:
  - 条目批量替换、批量删除或批量完成。
  - 已完成条目的重新打开。
  - `cancelled` 状态语义调整。
  - 拆分 `topic_scope_items` 关联表。

## Facts / Constraints

- 当前 `ScopeInItem` 只有 `goal/done_contract/status`，没有稳定 ID。
- 当前 `update_topic` 可整体覆盖 `scope_in`，也可直接设置 `progress/status`。
- 目标条目状态只保留 `pending/completed`；新增条目默认为 `pending`。
- 重复完成已完成条目是幂等操作；删除或完成不存在的条目返回明确错误。
- `progress` 采用整数百分比：`completed_count * 100 / total_count`。
- 重算活动状态：
  - 总条目为 `0`，或完成数为 `0`：`todo`。
  - 部分完成：`in_progress`。
  - 全部完成且总数大于 `0`：`done`。
- 新增、删除、完成后均重算，避免分母变化造成进度过期。
- `paused` 是人工覆盖状态：暂停期间禁止新增、删除和完成；恢复时根据当前条目重新计算活动状态。
- `scope_in` 继续保存在 `topics.scope_in` JSON 文本列，不新增关联表；单条管理是业务 API 粒度。
- 当前代码没有课题/条目的显式字符数上限；本次保持不设置应用层硬上限，不在 JSON Schema 增加 `maxLength`，实际容量由 SQLite `TEXT` 与调用链上下文共同约束。

## Restated Understanding

- 我理解当前任务是：把 `scope_in` 从 Topic 的普通可覆盖字段改为受控子资源。
- 当前核心目标是：条目状态成为 `progress/status` 的唯一活动状态来源。
- 当前边界是：使用现有 JSON 列完成最小实现，不引入关联表。
- 暂不处理：条目检索索引、历史版本和独立条目表。

## 接口契约设计

```text
create_topic(input_with_full_scope_in) -> Topic
add_scope_item(topic_id, goal, done_contract) -> Topic
delete_scope_item(topic_id, item_id) -> Topic
complete_scope_item(topic_id, item_id) -> Topic
pause_topic(topic_id) -> Topic
resume_topic(topic_id) -> Topic  // 根据条目重算活动状态
```

- `update_topic` 继续负责名称、描述和扩展信息，不再接受 `scope_in/progress`。
- `todo/in_progress/done` 不允许调用方直接设置，由条目重算。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，只记录条目管理和派生状态契约。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：小范围课题条目契约调整。
- 当前核心目标：禁止创建后全量覆盖，并保持 Topic 派生状态一致。
- 当前进度：实现与验证完成。
- 下一步 1：交付结果。
- 下一步 2：后续按需处理 `cancelled` 或条目重新打开。
- 涉及文件 / 模块：后续预计涉及 `models.rs`、`topic_store.rs`、`topic_manager.rs` 和 TUI。
- 风险：已有 SQLite JSON 条目缺少 ID，需要兼容迁移。
- 验证方式：迁移、暂停保护、单条操作和状态重算测试。
- Execution Approval: `Approved`

## Change Log

- 2026-07-29 00:34：创建轻量 spec；确认稳定条目 ID，并在每次单条变更后重算进度和状态。
- 2026-07-29 00:38：确认继续使用 JSON 存储、不设置应用层字符硬上限；用户批准执行。
- 2026-07-29 00:53：完成 JSON 兼容迁移、单条管理、派生状态、暂停保护、AI Tool 与 TUI 入口。

## Validation

- Self-check：通用更新已移除 `scope_in/progress/status` 的直接写入；单条操作统一重算。
- Static checks：`cargo fmt --check`、`cargo check` 通过；IDE lint 无新增错误。
- Runtime / Test：`cargo test` 通过，共 49 项；包含旧 JSON 迁移、暂停保护、状态重算和 5 万字符条目测试。
- Human confirmation：已确认使用 JSON、稳定 ID、每次变更重算并放宽字符限制。
- 结果汇总：实现与验证完成。
- 核心目标是否已由证据证明完成：是。
- 当前剩余差距：无。
- 剩余风险：SQLite TEXT 无应用层硬上限，但实际可用大小仍受调用模型上下文与内存影响。

## Resume / Handoff

- 当前状态：已完成。
- 当前卡点：无。
- 下一步唯一动作：交付用户验收。
- 下一轮核心目标：无；如新增条目查询或历史需求，再评估关联表。
