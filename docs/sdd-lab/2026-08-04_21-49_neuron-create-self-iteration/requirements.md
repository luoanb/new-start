# Requirements / 需求文档: 创建提示词自我迭代

## Restated Understanding / 需求复述

- 我理解当前需求是：让「创建提示词」节点（`system_type=create_neuron`，下称 creator）从"落库后内容永久固定"变为"可持续自我迭代"，且迭代必须是**正向、有据可依**的，不是随机改写。
- 当前核心目标是：creator 常驻 N=7 个候选变体（提示词写法变体），变体池随使用与评分持续更新（替换、重写、淘汰），创建新神经元时从池中加权选择变体作为 system prompt。
- 当前边界是：只做 creator 及其变体池的自我迭代闭环；不动普通神经元创建、满意度评分 delta 语义、候选选择整体框架。

## Background / 背景

- 现状：`ensure_creator` 三级只读（缓存→DB→种子），落库后 content 永不变；creator 无候选池子项（GUI 只走 `create_neuron_plain`，LLM 批量创建无入口）；`score_feedback` 只改介入神经元 weight，**不触及 creator/变体的 content**。
- 用户提出的两种迭代策略：
  1. 不断增加数量 + 程序按权重选池 —— 缺陷：无可靠流程评价 creator 权重（信号作用在子代，无法归因到父代变体），且池子无界膨胀。
  2. 常驻 N 个 + 持续更新 —— 关键问题：更新要参考哪些数据才能确保正向，而非随机不可靠。
- 结论：以策略 2 为主，吸收策略 1 的"数量探索"（观察位 + 加权随机选择）；二者共同地基是**归因（lineage）**。

## Scope / 范围

- In:
  - 神经元落库时记录 `lineage_parent_id`（生成所依赖的提示词来源：creator 或变体）。
  - 评分 delta 通过 `lineage_parent_id` 归因到父变体（`accumulated_delta`）。
  - 变体池常驻 N=7 正式 + 2 观察位；创建时加权随机选择（复用 `ORDER BY weight DESC, RANDOM()`）。
  - 变体更新：表现/失败/场景数据聚合 → 超门槛触发差分重写 → 一次换一个 → 退化自动回滚。
  - 用户手动编辑过的变体锁定（`manual_edited`），不自动重写。
- Out:
  - 不做 creator 整体 content 的自动改写（creator 只作为变体池的种子模板）。
  - 不重做 `score_feedback` 评分语义与 `adjust_*` delta 规则。
  - 不迁移存量历史神经元数据（新列 nullable，存量视为种子直生）。
  - 不改 GUI 手动创建（`create_neuron_plain`）路径。

## Acceptance Criteria / 验收标准

- [ ] creator 变体池常驻 7 个正式变体（+2 观察位），`select_candidates` 补足后不再无限增长。
- [ ] 任何 LLM 批量创建的子神经元都记录 `lineage_parent_id`（指向选中变体；种子直生时指向 creator）。
- [ ] `score_feedback` 的 delta 会归因到父变体（`accumulated_delta`），同时保留对介入神经元本身的既有评分。
- [ ] 变体 `use_count` / `accumulated_delta` / `last_used_at` 正确累积。
- [ ] 变体满足触发门槛（使用 ≥ 3 次 且 |delta| ≥ 2）后，触发一次差分重写；同一轮只更新 1 个。
- [ ] 新版本进入观察位；若下个评估周期 delta 为负，自动回滚旧版（`neuron_versions` 表可查历史）。
- [ ] `manual_edited=1` 的变体不参与自动重写与淘汰。
- [ ] 重写 LLM 调用失败时保留旧版，不影响创建主流程。
- [ ] 相关单测覆盖：归因、门槛触发、观察位转正、退化回滚、手动锁定。

## Constraints / 约束

- 业务约束：变体竞争靠权重与使用统计，不使用绝对内容评分。
- 技术约束：优先在 Store/Manager 写入点做迁移与归因，避免入口漏网；新增列一律 nullable 兼容存量。
- 成本约束：重写触发必须靠累积信号节流（阈值 + 一次一个），不得每次创建都调模型。
- 兼容性：存量神经元 `lineage_parent_id IS NULL` 视为种子直生，不参与变体归因。

## Open Questions / 开放问题

- 观察位是否参与 `select_candidates` 的选择？方案默认：观察位不参与（需要 store 查询加过滤），转正条件是 `use_count >= 1`。
- 是否需要一个手动触发命令（如 admin 面板"立即评估并更新变体池"）？方案默认：本轮不做，仅评分 hook 自动触发。

## Requirement Decisions / 需求决策

- 2026-08-04 21:49:
  - 决策：以"策略 2 常驻更新"为主实现，吸收策略 1 的探索思想；先建 lineage 归因地基，再建更新闭环。
  - 原因：策略 1 的"可靠权重"依赖策略 2 的"更新参考数据"来喂养；两者互补而非互斥。
  - 关联：`neuron_manager.rs` 的 `ensure_creator` / `select_candidates` / `create_neuron` / `score_feedback_hook` 均为本迭代涉及模块。
