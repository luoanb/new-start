# Neuron 域愿景差距与待优化点（docs/pulsar/neuron/roadmap.md）

本文件记录：① 设计愿景 vs 代码现状的差距；② 已确认但**语义不清、暂停优化**的点；③ 未来方向。
核对基准：2026-09-01。

## 1. 设计愿景 vs 现状

| # | 愿景 | 现状（代码事实） | 差距 |
|---|---|---|---|
| 1 | 自主/动态维护节点和边 | 选型命中自动回挂边、打分调边权重、定时回收删节点 | 已有节点间"自主产生新关联"未实现（见 §3） |
| 2 | 对外提示词服务，外部无感 | `select_role` / `select_assistant_candidates` 已收敛为对外入口（[services.md](./services.md)） | 基本对齐 |
| 3 | 择优迭代（目前以权重为主） | 权重体系完整：创建恒 0 → 打分叠加 → 选型向高权重 | 对齐；未来可加其他择优信号 |
| 4 | 评价体系（两种用户介入） | ① Hook 裁决（模型分析用户意图）② 用户手动 `score_feedback` | **完全对齐** |
| 5 | 遗忘：达上限自主删除 | 异步定时 `recycle_if_over_capacity`（超 300 删低价值，逻辑删除，系统豁免） | 语义差异见 §2.1 |
| 6 | 未来：更迭（创建+建边）、新关联、节点/边打分 | 变体更迭已实现；回挂边已实现；边权重打分已实现 | "已有节点间自主产生新关联"未落地 |

## 2. 已确认语义、暂停优化

### 2.1 "一换一"（愿景第 5 点）

**确认（2026-09-01）**：接受现状——回收是异步定时批量清（超容量删低价值），**创建路径不强制同步淘汰**，不要求"新节点落库前先清一个旧节点"。如需"严格一换一"是未来可选优化。

### 2.2 系统神经元分类（愿景相关：多类节点共享 `system_type` 字段）

**状态：语义不清，暂停优化。**

**问题**：`system_type` 非空的节点承载多种行为（creator 生成器、选型器、裁决指令、会话规范），目前不虚构分类，仅客观陈述"通用容器 + 编译期常量契约"（见 [index.md §4](./index.md)）。字段值（content/behavior/tool_ids）不构成分类依据——它们是同一通用容器的可变性状。

**结构性事实（分类的可能依据，尚未实施）**：以下 system_type 被多个独立模块**硬编码引用**，是编译期常量（`pub const`，[manager.rs:32-33](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L32-L33)）：

| system_type 常量 | 引用方（模块） | 用途 |
|---|---|---|
| `create_neuron`（CREATOR_SYSTEM_TYPE） | [selection.rs:76/90/106](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/selection.rs#L76) | 生成原语、变体池宿主 |
| `assistant_select_neuron` | [conversation_runner.rs:1247](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L1247)、[selection.rs:504](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/selection.rs#L504) | 选型器提示词 |
| `assistant_user_round_judgement` / `assistant_round_review` | Hook 域 / assistant_session | 裁决指令 |
| `session.` 前缀 | [creation.rs:282](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/creation.rs#L282)、[spec.rs:44/124](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/spec.rs#L44) | 会话规范，behavior 强制 |
| `"create_neuron"` | [tools.rs:263](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/tools.rs#L263) | 创建工具同名耦合 |

**结论**：删除任一系统节点，对应引用模块即失效——它们是"不可删依赖"。但它们是否构成真正的"类型/物种"，取决于未来是否重构。

**优化方向（待排期，非文档能解决）**：把散落的常量收敛为显式 `SystemNeuronKind` 枚举 + 注册表，让"节点 ↔ 消费方"成为可校验关系。此前文档一律不虚构分类。

### 2.3 其他语义模糊点（观察记录）

- **weight 语义**：已固化为"价值/重要度分"，但接口名 `adjust_weight` 易与"连接强度"混淆（文档已纠正，代码注释可后续补）。
- **lineage 与 network 双轨**：lineage（生成归因）与 connections（网络结构）并存且可不同，前端仅有网络图——两者关系需显式说明（已在本目录文档固化）。
- **variant_state 三态**：NULL=active、observing、manual_edited 互斥规则散落在 SQL 与状态机。
- **pool→7→1 与邻域池配额**：创建流候选池与助手邻域池（NeighborhoodPoolPolicy）是两套机制，易混淆（本目录文档已区分）。

## 3. 未来方向（愿景第 6 点）

- **已有神经元间自主产生新关联**：目前网络边仅由"选型命中回挂"产生；"节点间自主建边"未实现，需配套边价值评估与去环/幂等约束。
- **创建即建边**：目前创建可挂父（parent 边）；"新节点与已有节点自动产生关联"未实现。
- **节点与边权重的持续打分**：已具备（apply_score_feedback + adjust_edge_weight），可扩展打分来源。

## 4. 决策记录

| 日期 | 决策 |
|---|---|
| 2026-09-01 | 域文档独立成册（本目录 5 篇），Neuron 为第一个固化域 |
| 2026-09-01 | "一换一"接受现状（异步定时回收，创建路径不强制同步淘汰） |
| 2026-09-01 | 系统神经元语义不清 → 暂停，不虚构分类，仅陈述编译期常量契约 |
