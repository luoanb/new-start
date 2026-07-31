# Lifecycle / 生命周期: 神经元创建权重固定为 0

```yaml
status: done
result: completed
created_at: 2026-08-01 00:31
updated_at: 2026-08-01 00:35
owner: user
```

## Current Summary / 当前摘要

- 批准状态：已执行并验证
- 当前状态：done
- 当前核心目标：已完成 — 创建路径节点/边权重强制 0，仅评价 delta 可改
- 下一步唯一动作：无

## Execution Log / 执行记录

- 1. 2026-08-01 00:31: Reverse Sync。用户确认：创建时节点权重与边权重都必须为 0，只允许后续评价调整。落盘需求与技术方案。
- 2. 2026-08-01 00:32: 用户批准执行。
- 3. 2026-08-01 00:35: 实现 Store/Manager/Tool/TUI/默认种子强制 0；回写 bootstrap / system-prompt-ready / storage 文档；`cargo test` 59 passed。

## Validation / 验证

- `cargo test`（`packages/agent-app/src-tauri`）：59 passed
- 关键测：`test_create_and_link_force_zero_weight`、`create_generated_ignores_model_weight_and_uses_zero`
