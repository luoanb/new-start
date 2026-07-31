# Lifecycle / 生命周期: 神经元自举与工具契约

```yaml
status: done
result: completed
created_at: 2026-07-28 23:43
updated_at: 2026-07-29 00:20
owner: user
```

## Current Summary / 当前摘要

- 批准状态：Option A 已实现并通过验证
- 当前状态：done
- 当前核心目标：神经元前置能力已完成，可供 Assistant 迭代接入
- 下一步唯一动作：无（权重创建语义纠偏见 `2026-08-01_00-31_neuron-create-weight-zero`）

## Reverse Sync Note / 反写说明

- 2026-08-01：用户确认「创建时节点权重与边权重必须为 0，只允许后续评价 delta」。原方案中「模型 JSON weight 落库 / 创建下游可传 weight·edge_weight」作废，以新迭代为准。

## Execution Log / 执行记录

- 1. 2026-07-28 23:43: 创建“神经元自举与工具契约”迭代，作为 Assistant 运行模式的前置需求。
- 2. 2026-07-28 23:49: 用户确认 `system_type` 唯一性、直接下游选择范围和 `min_new` 自动补齐语义。
- 3. 2026-07-28 23:50: 完成需求文档初稿；本轮未修改代码，未创建技术方案。
- 4. 2026-07-28 23:57: 候选选择与补齐能力调整为 AI 和人类均可调用；新增 `system_type` 来源，和 `source_id` 同时传入时后者优先。
- 5. 2026-07-28 23:58: 参数命名统一为 `source_id`；用户确认需求并要求生成技术方案，迭代进入 planned。
- 6. 2026-07-28 23:58: 用户批准 Option A，开始代码执行。
- 7. 2026-07-29 00:20: 完成模型/迁移、NeuronManager、自举与 7 候选入口、AI 工具、TUI 管理入口和配置文档。
- 8. 2026-07-29 00:20: `cargo fmt --check`、`cargo check`、`cargo test` 通过，共 48 项测试；保留 `compactor.rs` 既有未使用 import 警告。

## Transition Log / 状态流转

- 2026-07-28 23:58:
  - 变更前：draft
  - 变更后：planned
  - 原因：需求边界已确认，技术方案已形成。
  - 依据：用户指令“改了后出技术方案”。
  - 下一步：等待用户确认技术方案并批准执行。
- 2026-07-28 23:58:
  - 变更前：planned
  - 变更后：executing
  - 原因：用户批准推荐技术方案。
  - 依据：技术方案确认问答选择“批准执行推荐方案”。
  - 下一步：实施并验证神经元前置能力。
- 2026-07-29 00:20:
  - 变更前：executing
  - 变更后：done
  - 原因：需求验收项已实现，格式、编译和测试验证通过。
  - 依据：48 项 Rust 测试全部通过，IDE lint 无新增错误。
  - 下一步：推进 Assistant 运行模式技术方案。
