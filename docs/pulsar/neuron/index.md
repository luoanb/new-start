# Neuron 域总览（docs/pulsar/neuron/index.md）

> 本目录为神经元域的独立文档。核对基准：2026-09-01 磁盘代码。
> 分篇：本文件（概念与边界）· [services.md](./services.md)（对外服务契约）·
> [lifecycle.md](./lifecycle.md)（生命周期）· [data-model.md](./data-model.md)（数据契约）·
> [roadmap.md](./roadmap.md)（愿景差距与待优化点）。

## 1. 域定义

Neuron 域 = **可复用能力节点的网络 + 提示词服务 + 评价择优 + 遗忘回收**。

代码自述（[core/neuron/mod.rs:1](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/mod.rs#L1)）：*"Neuron 领域：知识原子网络 + 选型 + 创建 + 演化 + 系统神经元行为"*。

一句话：神经元是"一个可被选中并当系统/知识文本执行的能力节点"（[DEFAULT_CREATE_NEURON_PROMPT](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/config.rs#L18)），Neuron 域维护这个节点网络的增删改查、选型、打分、演化与容量回收，并把"选一个角色"以服务形式提供给系统其他部分。

## 2. 核心概念

| 概念 | 定义 | 代码落点 |
|---|---|---|
| 神经元 | 能力节点：`desc`（≤20 字标签）+ `content`（可执行提示词/知识块）+ `weight` + `tool_ids` + 网络位置 | neurons 表 |
| 权重 weight | **价值/重要度分**（不是连接强度）：创建恒 0，只经 `adjust_weight(delta)` 增减；选型与回收排序依赖 | [store.rs:147-150](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L147-L150) |
| 边 connection | source→target 有向挂靠关系，同样创建恒 0，仅经 `adjust_edge_weight` 增减 | connections 表 |
| 变体 variant | creator（`create_neuron`）生成的普通神经元，`lineage_parent_id` 指向 creator；变体有观察/晋升/回滚状态 | evolution.rs |
| lineage 与 network | lineage = "谁生成了我"（演化归因）；connections = "我挂在哪"（网络结构）。两套并存、可不同 | 字段 vs 边表 |
| 系统神经元 | `system_type` 非空的节点；**语义不清，当前按"通用容器 + 编译期常量契约"客观陈述，见 roadmap** | 见 §4 |
| 普通神经元 | `system_type IS NULL`：可被选型、打分、回收、变体化的知识节点 | neurons 表 |
| 容量回收 | 超 `neuron.capacity`（默认 300）时删除低价值普通节点（逻辑删除），系统神经元豁免 | query.rs / config.rs |

## 3. 域职责边界

### 3.1 本域负责

- 节点与边的增删改查、网络遍历、分页治理（query 服务）。
- **提示词服务**：对外选型（`select_role` / `select_assistant_candidates`），返回"角色"（选中节点的 content + 工具授权）。选型策略（候选池装配 → n=1 短路 → LLM 选型 → 权重回退）是内部实现。见 [services.md](./services.md)。
- **评价服务**：`apply_score_feedback`——把外部打分落成节点权重、关联边 delta、lineage 归因，并触发 creator 变体演化。见 [services.md](./services.md)。
- **创建与系统节点治理**：统一创建流、bootstrap / rebootstrap、`session.` 规范节点、行为（behavior）读写。
- **演化**：creator 变体状态机（表现好的晋升并差分重写回 creator，差的回滚/淘汰）。
- **遗忘**：定时容量回收。

### 3.2 本域不负责

- 会话轮次管线（Conversation 域）、课题状态机（Topic 域）、裁决执行逻辑（Hook 域）。
- 打分**来源**（谁评价、为什么评价）由外部决定；本域只负责分数**如何影响网络**。
- 数据与 Topic / Hook 共库 `app.db`（[gateway.rs:187](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L187)），但互斥锁隔离，域间不共享内部状态。

### 3.3 与外部域的交互

| 外部域 | 交互 | 方向 |
|---|---|---|
| Conversation（round_resolver） | 选型驱动：按 `ASSISTANT_SELECT_NEURON` 常量取选型器，调用 `select_role` 取角色注入 wire | 调用 Neuron |
| Assistant（assistant_session） | 打分回写：`apply_score_feedback`（合并裁决/用户评价） | 调用 Neuron |
| Hook | 裁决 hook 输出 score/neuron_ids → Assistant 域转交 Neuron | 数据经 Assistant 中转 |
| Tool | `core/neuron/tools.rs` 注册 6 个 System 标签 AI 工具（需 `inserts/*.md`）；creator 的 tool_ids 从注册表挑选 | 双向 |
| Provider | `DefaultNeuronModelCaller`：用 `default_model_selection` + `call_model`（禁 thinking） | 调用 Provider |
| Gateway | 装配 `neuron_manager`；异步 bootstrap；`spawn_neuron_recycle_runtime` 后台回收 | 容器 |
| 前端 | NeuronNetworkGraph / NeuronManager / NeuronDetailDrawer + Neuron 组 / Session Specs 组 commands | 调用 |

## 4. 系统神经元（现状客观陈述，语义不清）

`system_type` 非空的节点暂按以下客观事实描述（**不虚构分类**，不展开业务语义）：

- `create_neuron`：内容为"神经元创作者"生成指令（[config.rs:16](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/config.rs#L16)），作为 creator 被 [selection.rs:76/90/106](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/selection.rs#L76) 硬编码引用（生成原语 + 变体池宿主）。
- `assistant_select_neuron`：内容为"会话角色选择器"判定指令（[config.rs:56-76](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/config.rs#L56-L76)），被 [conversation_runner.rs:1247](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L1247) 与 [selection.rs:504](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/selection.rs#L504) 硬编码引用（选型器）。
- `assistant_user_round_judgement` / `assistant_round_review`：裁决指令，被 Hook 域读取。
- `session.` 前缀：会话规范节点，`starts_with("session.")` 前缀校验（[creation.rs:282](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/creation.rs#L282)、[spec.rs:44](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/spec.rs#L44)），behavior 强制非空，由 SessionSpecManager 管理。

> **这些 system_type 是编译期常量契约**（`pub const`，[manager.rs:32-33](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L32-L33)），被多个独立模块写死引用。它们是不是同一类、要不要收敛为显式类型——**语义不清，暂停，列入 [roadmap.md](./roadmap.md) 待优化**。

## 5. 内部结构（Facade）

依赖方向 `manager → {query, selection, creation, evolution, specs}` 无环：

| 服务 | 职责 |
|---|---|
| manager.rs | 门面，组合 4 服务 + `SessionSpecManager`；公开 API 与拆分前一致；含 `select_role` / `scope_for_selection` |
| query.rs | 查询与治理：单点/分页/网络遍历、内容更新、权重与边调整、管理面图操作、使用信号、容量回收 |
| selection.rs | 候选池装配 + LLM 选型 + 权重回退 + 回挂边；自含生成原语（ensure_creator / generate_drafts / persist_plain / fill_candidates_batch） |
| creation.rs | 统一创建流、系统节点 ensure/reset、bootstrap / rebootstrap |
| evolution.rs | creator 变体状态机（observing→active 晋升 / 负分回滚 / 差分重写） |
| spec.rs | `session.%` 规范节点 behavior 唯一收敛点 |
| tools.rs | 6 个 System 标签 AI 工具 |
| store / model / config | 数据访问层 / 模型调用接口 / 种子与 config 读取 |

## 6. 快速索引

- 对外服务契约（怎么调用）→ [services.md](./services.md)
- 生命周期（节点怎么生老病死）→ [lifecycle.md](./lifecycle.md)
- 存储与数据契约（表结构、不变量）→ [data-model.md](./data-model.md)
- 愿景差距与待优化点 → [roadmap.md](./roadmap.md)
