# Neuron 域数据契约（docs/pulsar/neuron/data-model.md）

存储位置：`<data_root>/app.db`，与 Topic / Hook 共库（[gateway.rs:187](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L187)），经各 Store 独立互斥锁隔离。

## 1. 表结构

### neurons

基础列（[store.rs:40-47](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L40-L47)）+ 迁移追加列（[store.rs:57-125](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L57-L125)）：

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | `n_{now_ms}_{seq}` |
| desc | TEXT | ≤20 字能力标签 |
| content | TEXT | 可执行提示词/知识块 |
| weight | REAL | 价值/重要度分，创建恒 0 |
| created_at / updated_at | INTEGER | 时间戳（ms） |
| system_type | TEXT NULL | 系统节点标记，**部分唯一索引**（[store.rs:68-75](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L68-L75)） |
| tool_ids | TEXT | JSON 数组 `'[]'` 默认 |
| lineage_parent_id | TEXT NULL | 生成者（creator 归因） |
| use_count | INTEGER | 活跃信号（使用/编辑/打分都会 +1） |
| accumulated_delta | REAL | 变体分数累计（演化判定） |
| last_used_at | INTEGER NULL | 最近活跃时间 |
| variant_state | TEXT NULL | 变体状态（observing 等；NULL=active） |
| manual_edited | INTEGER | 手动编辑锁（1=禁止自动重写/淘汰） |
| deleted_at | INTEGER NULL | 逻辑删除标记 |
| behavior | TEXT NULL | 行为契约（JSON） |

### connections

| 列 | 类型 | 说明 |
|---|---|---|
| source | TEXT FK→neurons(id) ON DELETE CASCADE | 起点 |
| target | TEXT FK→neurons(id) ON DELETE CASCADE | 终点 |
| weight | REAL | 边权重，创建恒 0 |

PK = (source, target)。有向边。

### neuron_versions

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | 版本 id |
| neuron_id | TEXT FK→neurons(id) ON DELETE CASCADE | 所属节点 |
| content | TEXT | 版本内容快照 |
| source | TEXT | 来源（seed / evolve / rollback 等） |
| created_at | INTEGER | 创建时间 |
| prev_version_id | TEXT NULL | 前驱版本（不可变链） |

索引：`(neuron_id, created_at DESC)`。版本历史随节点逻辑删除保留。

## 2. 不变量（Store 契约）

1. **创建恒 0**：节点与边创建时权重强制为 0，忽略输入（[store.rs:147-150](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L147-L150)）；分数只来自后续评价。
2. **权重增减唯一入口**：`adjust_weight(delta)` / `adjust_edge_weight(source,target,delta)`，两者都会合并"编辑/打分即使用"信号（use_count+1, last_used_at）。
3. **system_type 唯一**：部分唯一索引；赋 system_type 只走 ensure 路径；system_type 非空字符串校验（[store.rs:151-157](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/store.rs#L151-L157)）。
4. **逻辑删除**：回收走 `deleted_at`，版本历史保留；全链路（查询/列表/候选/边）排除已删节点；系统神经元回收豁免。
5. **manual_edited 锁**：管理面编辑过的变体不参与自动重写/淘汰。
6. **observing 不出池**：观察期变体不进候选池与全局候选。

## 3. 前端语义

- 前端图视图读取 `get_connections` / `get_network` 渲染有向图；权重影响节点展示。
- `list_neurons_page` 分页治理；`update_neuron` / `adjust_*` 走管理面（更新会写版本链）。
- AI 工具禁止更新系统神经元内容（`update_content_for_ai` 校验）；管理面可改 behavior（`update_behavior_for_admin`）。
