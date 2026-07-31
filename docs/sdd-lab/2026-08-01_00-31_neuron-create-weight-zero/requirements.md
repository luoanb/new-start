# Requirements / 需求文档: 神经元创建权重固定为 0

## Restated Understanding / 需求复述

- 我理解当前需求是：纠正神经元创建链路的权重语义偏差——创建节点与创建边时权重必须固定为 `0`，不得由模型输出、AI 工具参数或管理入口在创建瞬间写入非零权重；权重只允许通过后续评价（delta 增减）调整。
- 当前核心目标是：所有创建路径落库时 `neuron.weight = 0` 且新建边 `connection.weight = 0`。
- 当前边界是：改 `NeuronManager` / Store 创建与连边入口、AI 工具参数、自举提示词与 draft 落库；不改满意度打分与 `adjust_*` 语义本身。
- 暂不处理：历史已有非零权重数据的批量清零或迁移；边/节点权重的绝对值覆盖 API。

## Scope / 范围

- In:
  - 节点创建（自举、补齐、下游创建、管理创建、系统根 ensure）一律落库 `weight = 0`。
  - 边创建（`create_downstream`、`link`）一律落库 `weight = 0`。
  - 忽略模型 JSON 中的 `weight`（可仍解析以免旧输出炸裂，但不得写入）。
  - AI 工具与 TUI 创建入口不得再接受创建时权重参数。
  - 创建提示词不再要求模型为新神经元打初始权重分。
  - 权重变更仅通过既有 `adjust_weight` / `adjust_connection_weight`（评价、Hook、人工）。
- Out:
  - 不重做候选选择、7 选 1、满意度打分流程。
  - 不强制重置历史库中已有非零权重。

## Acceptance Criteria / 验收标准

- [x] 任意创建路径新建神经元的 `weight` 均为 `0`。
- [x] 任意新建连接边的 `weight` 均为 `0`。
- [x] `create_downstream_neuron` 工具 schema 不再暴露 `weight` / `edge_weight`。
- [x] 模型自举即使返回非零 `weight`，落库仍为 `0`。
- [x] 创建后仍可通过 delta 调整节点权与边权。
- [x] 相关单测覆盖强制置零与忽略模型 weight。

## Constraints / 约束

- 业务约束：初始权重一律平等（0）；差异只来自后续评价。
- 技术约束：优先在 Manager/Store 写入点强制置零，避免入口漏网。
- 兼容性：不迁移历史数据；`GeneratedNeuronDraft.weight` 可保留字段但写入忽略。

## Open Questions / 开放问题

无（用户已确认：节点与边创建权重都是 0）。

## Requirement Decisions / 需求决策

- 2026-08-01 00:31:
  - 决策：创建时节点权重、边权重均固定为 `0`；只允许后续评价 delta 调整。
  - 原因：用户明确纠正当前「创建可填/模型可写权重」的偏差。
  - 关联：反向同步 `docs/sdd-lab/2026-07-28_23-43_neuron-bootstrap/` 中允许创建写入 weight 的表述。
