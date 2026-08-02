# Requirements / 需求文档: 神经元面板 — 移除 tag 筛选 + 创建/权重

## Restated Understanding / 需求复述

- 我理解当前需求是：对 `packages/agent-app` 的神经元管理面板做三件事——(1) 移除神经元左侧列表顶部的 tag 筛选；(2) 新增「创建神经元」功能，支持孤立 / 某神经元的下游神经元两种模式；(3) 新增「调整权重」功能，支持调整神经元自身权重与关联关系边权重。
- 当前核心目标是：降低面板噪音（去 tag 筛选）、补齐人工创建与权重编辑能力，且新建神经元权重恒为 0（符合 `neuron-create-weight-zero` 约束）。
- 当前边界是：只改前端面板交互与后端暴露的命令；不触动 assistant 运行态对权重的消费。
- 暂不处理：LLM 驱动的自动创建流程（`neuron_manager.create_neuron` 统一流程）、神经元删除、tool_ids 编辑等。

## Scope / 范围

- In:
  - 移除 `NeuronManager.svelte` 工具栏基于 `system_type` 的 tag 筛选 chips 及过滤逻辑。
  - 新增「创建神经元」入口，支持「孤立」「下游」两种模式：下游需选择上游神经元，创建后自动建边（边权重 0）。
  - 新增「调整权重」入口：神经元自身权重步进 + 关联边权重步进。
  - 后端暴露对应 Tauri 命令（当前未暴露：`create_downstream_neuron` / `adjust_weight` / `adjust_edge_weight` 已被 `neuron_manager` 持有）。
- Out:
  - 不改 `NeuronIndex` 左侧按 `system_type` 分组浏览逻辑（仅去顶部筛选，保留分组）。
  - 不改动 LLM 统一创建流程 `neuron_manager.create_neuron`。
  - 不涉及神经元删除、tool_ids 编辑、assistant 运行态权重消费逻辑。

## User Interaction / 用户交互

- 触发入口：神经元管理面板（`NeuronManager` 工具栏、「创建神经元」按钮；详情抽屉 `NeuronDetailDrawer` 权重区）。
- 用户操作路径：
  - 移除筛选：打开面板即不再出现 system_type tag chips；列表恢复纯浏览（按类型分组、按权重排序）。
  - 创建：点击「创建神经元」→ 弹窗选模式（孤立/下游）→ 下游时选上游神经元 + 填 desc/content → 确认 → 列表刷新并选中新神经元、打开抽屉。
  - 调权重：在抽屉「权重」字段点 `−/+` 调整自身权重；在「关联」每条边点 `−/+` 调整边权重。
- 系统反馈：创建/调整成功后列表与抽屉即时刷新；失败给出可见错误提示（来自 `AppError.payload()`）。
- 状态变化：工具栏从「含 tag 筛选」变为「纯浏览 + 创建按钮」；新神经元 state 进入列表；权重数值在 store 持久化。
- 异常/边界交互：下游模式所选上游 id 不存在 → 创建失败提示；权重调整并发重复点击 → 加 saving 锁。
- 不应发生的交互：不应静默失败；不应因去筛选而破坏左侧分组展示。

## Acceptance Criteria / 验收标准

- [ ] AC-1：面板顶部不再出现 `system_type` 的 tag 筛选 chips；左侧列表仍按类型分组、按权重排序正常展示。
- [ ] AC-2：点击「创建神经元」可创建孤立神经元，列表即时出现该神经元（`weight=0`、无连接）。
- [ ] AC-3：选择某上游神经元后创建下游神经元，新神经元存在且其 connections 含 `上游 -> 新神经元` 边（边权重 0）。
- [ ] AC-4：在抽屉中调整神经元权重，数值变化并持久化（刷新后仍生效）。
- [ ] AC-5：在抽屉中调整某条边权重，数值变化并持久化。
- [ ] AC-6：所有失败路径（上游不存在、调整失败）均有可见错误提示，不出现未捕获异常。

## Constraints / 约束

- 业务约束：
  - 新建神经元 `weight` 恒为 `0.0`、`system_type = None`、`tool_ids = []`（遵循 `neuron-create-weight-zero`）。
  - 下游模式的边权重恒为 `0.0`（由 store 直持久化逻辑保证）。
- 技术约束：
  - 新增命令必须在 `lib.rs` 注册并加入 `generate_handler!` 调用列表，与既有 `list_neurons` 等保持一致。
  - 本次创建走 store 直持久化（用户输入 desc/content），不触发 LLM 草稿生成。
  - 权重可为正/负（与后端 `adjust_weight` 数值累加实现一致，不做非负限制）。
- 时间/兼容性约束：
  - 不破坏既有 `update_neuron` / `get_connections` 调用方。

## Referenced Designs / 引用设计稿

> 本需求不涉及 Figma / 视觉稿，无链接。

## Open Questions / 开放问题

- [ ] Q1：顶部 tag 筛选移除后，`NeuronManager` 工具栏上是否还保留 depth / edge-type 等其它筛选项？（当前探查到工具栏 tag 为 system_type，depth/edge-type 是否仍在使用需实现时确认，不影响需求边界）
- [ ] Q2：创建下游神经元时的上游选择范围是否限制为「仅普通 neuron」还是兼容 system neuron？（建议不限，沿用现有列表即可）

## Requirement Decisions / 需求决策

- 2026-08-02 10:00:
  - 决策：采用 sdd-lab 规范，需求阶段仅产出 `lifecycle.md` + `requirements.md`，技术方案留待下一阶段。
  - 原因：用户明确要求严格按 sdd-lab；需求阶段不写实现方案。
