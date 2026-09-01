# Neuron 域对外服务契约（docs/pulsar/neuron/services.md）

本文件定义 Neuron 域对系统其他部分的**对外接口**。调用方只需满足契约，不感知网络内部实现。
服务实现统一经 `NeuronManager`（[core/neuron/manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs)）收敛。

## 1. 提示词服务（Prompt Service）

**契约**：调用方表达"本次会话需要什么能力/角色"，域内完成候选池装配与选型，返回**角色**（选中节点的 `content` + 工具授权）。调用方对网络内部无感。

**入口**：

| API | 说明 | 落点 |
|---|---|---|
| `select_role(seed, state, ctx) -> RoleOutcome` | 会话统一选型入口：scope 装配候选池 → 选型 → 返回角色 | [manager.rs:428](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L428) |
| `select_assistant_candidates(...) -> Vec<CandidateResult>` | 助手模式候选池装配与逐候选选型 | selection.rs |
| `get_by_system_type(system_type)` | 按 system_type 查系统节点（裁决/选型指令读取） | [manager.rs:146](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L146) |

**已知调用方**：round_resolver（Conversation 域）按 `ASSISTANT_SELECT_NEURON` 常量取选型器并调用 `select_role`（[conversation_runner.rs:1247](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L1247)）；Hook 域裁决调用经 Assistant 域读取 `assistant_*` 指令。

**选型策略（域内实现，非契约）**：scope 装配候选池 → 候选数 n=1 时硬规则短路（跳过模型、记使用）→ LLM 选型（输出 `{"neuron_id":...}`，[assistant_select_neuron 种子](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/config.rs#L56-L76)）→ 命中后回挂边 + `mark_used` → LLM 失败权重回退（确定性折中，非真随机）。

## 2. 评价服务（Feedback Service）

**契约**：外部把"分数 + 目标 + 来源"交给本域，本域负责分数**如何影响网络**（节点权重、关联边 delta、lineage 归因、变体演化）。

**入口**：

| API | 说明 | 落点 |
|---|---|---|
| `apply_score_feedback(目标会话/课题, neuron_ids, score)` | 评价落网：节点权重 + 边 delta + lineage 归因 + 触发变体演化 | [assistant_session.rs:990](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L990)（经 NeuronManager 收敛） |
| `score_feedback(conversation_id, message_index, score)` | 用户手动消息评价入口（Tauri 命令） | [lib.rs:697](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L697) |
| `adjust_neuron_weight(id, delta)` | 直接权重增减（管理面） | query.rs |
| `adjust_edge_weight(source, target, delta)` | 直接边权重增减（管理面） | query.rs |

**外部调用方（分数来源，本域不负责）**：

1. Hook 域 `user_round_judgement` 合并裁决：模型分析用户意图后给出评分（[user_round_judgement.rs:175](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/instances/user_round_judgement.rs#L175)）。
2. 用户手动评价：`score_feedback` 命令（[lib.rs:697](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L697)）。
3. TUI `/neuron` 命令、RPC（[rpc.rs:766](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/rpc.rs#L766)）。

**边界**：打分来源可能继续扩展，本域契约不变——**本域只定义"分数如何落网"，不定义"分数从哪来"**。

## 3. 管理面 API（NeuronManager）

| 组 | API | 说明 |
|---|---|---|
| 启动 | `bootstrap` / `rebootstrap` | 底座构建 / 重建（见 lifecycle.md） |
| 创建 | `ensure_system_neuron` / `ensure_session_neuron` / `create_neuron_plain` / `generate_drafts` | 统一创建流 |
| 查询 | `get_neuron` / `list_neurons` / `list_neurons_page` / `get_connections` / `get_network` | 单点/分页/网络遍历 |
| 治理 | `update_neuron` / `adjust_neuron_weight` / `adjust_edge_weight` / 管理面图操作 | 内容与拓扑 |
| 行为 | `get_session_behavior` / `update_behavior_for_admin` / `set_neuron_system_type` | `session.%` 与系统节点行为 |
| 演化 | `maybe_evolve_creator_variants` / 变体状态迁移 | creator 自迭代 |
| 回收 | `recycle_if_over_capacity` | 容量回收（后台 runtime 定时调用） |

## 4. AI 工具（System 标签，6 个）

[core/neuron/tools.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/tools.rs) 注册，供模型决策使用：

`get_neuron` / `list_neurons` / `update_neuron` / `get_network` / `create_neuron` / `select_neuron_candidates`

约束：System 标签工具随主对话 wire 授权（非 Core 常驻）；`create_neuron` 工具与 creator 系统节点同名耦合（[tools.rs:263](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/tools.rs#L263)）。

## 5. Tauri / RPC 命令

**Neuron 组（9）**：`list_neurons` / `get_neuron` / `update_neuron` / `get_connections` / `get_network` / `create_neuron_plain` / `adjust_neuron_weight` / `adjust_edge_weight` / `score_feedback`

**Session Specs 组（6）**：`open_session` / `list_neurons_page` / `set_neuron_system_type` / `update_neuron_behavior` / `reset_system_prompts` / `list_insert_catalog`

前端 UI：NeuronNetworkGraph / NeuronManager / NeuronDetailDrawer；远程模式经 `/api/rpc` 同契约（[rpc.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/rpc.rs)）。
