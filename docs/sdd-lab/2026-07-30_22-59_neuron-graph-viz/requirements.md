# Requirements / 需求文档: neuron-graph-viz

## Restated Understanding / 需求复述

- 我理解当前需求是：在已有 Neuron 管理 GUI（列表 / 详情 / 缩进树网络）之上，提供真正的**局部有向图可视化**；同时补齐后端数据契约，使一次请求即可获得可视化所需的**节点 + 边**子图。
- 当前核心目标是：以选中 Neuron 为中心的 ego-network 图视图可渲染、可点击跳转详情；`max_depth` 可在 GUI 调节并重新拉取；数据层返回完整子图。
- 当前边界是：复用现有 NeuronStore / Tauri 命令链路与 `NeuronManager` 入口；图为只读浏览，不改动创建/删除/改权/改边；图渲染采用 Svelte Flow。
- 暂不处理：全图一次加载全部 Neuron、拖拽改拓扑、权重训练可视化、Cytoscape 级图分析、批量图编辑。

## Scope / 范围

### In

1. **后端 — 演进 `get_network` 为邻域子图契约**
   - 返回 `NeuronSubgraph { seed_id, neurons, connections }`
   - `connections` 仅包含两端均落在返回 `neurons` 集合内的边
   - 保留现有无向邻接遍历语义（出入边都扩展邻域）；边本身仍为有向（`source → target` + `weight`）
   - 同步更新同包消费方：Tauri、前端类型、`NeuronNetwork`、TUI、AI `get_network` tool、相关单测

2. **前端 — 网络视图升级（默认图）**
   - 默认进入**图视图**；可切换回缩进树列表
   - 图视图基于 **Svelte Flow（`@xyflow/svelte`）**：平移/缩放、有向边、节点可点击跳转详情
   - **`max_depth` 可控**：网络视图提供深度控件；变更后重新 `invoke("get_network")` 拉取子图
   - 深度范围：最小 `1`，最大 `5`，默认 `2`（与现网 GUI 默认一致）
   - 视觉编码：节点展示 `desc`（或短 id）与权重；边展示方向与权重；`system_type` 非空可区分
   - 空邻域 / 仅自身 / 加载失败有明确状态
   - 图上允许拖拽节点做**临时排版**（不持久化、不改边）

3. **兼容与安全语义**
   - 成环：BFS `visited` 防无限扩展；渲染不出现递归死循环
   - GUI 仍不可创建/删除 Neuron，不可改边、不可改权重 / `system_type` / `tool_ids`

### Out

- 全图一次加载全部 Neuron（不设「depth=∞ / 全图」模式）
- 拖拽改连接、在图上编辑权重或内容、持久化节点坐标
- Cytoscape 等重量级图分析库
- Neuron 自动创建 / 候选推荐 UI
- 独立新导航入口（继续走现有 StatusBar → Neuron 管理 → 详情 → 网络）

## User Interaction / 用户交互

- **触发入口**：Neuron 详情 →「查看网络」
- **用户操作路径**：
  1. 进入网络视图 → **默认图视图**（Svelte Flow），`max_depth=2`
  2. 调节深度控件（1–5）→ 重新加载子图并刷新画布
  3. 可切换回缩进树列表；再切回图
  4. 点击图中节点 → 跳转该 Neuron 详情
  5. 画布内可平移/缩放；可拖节点临时排版（刷新或改 depth 后重新布局）
  6. 返回 → 与现有 `NeuronManager` 状态机一致
- **系统反馈**：切换 depth / 初次进入显示 loading；失败 banner；空邻域提示
- **状态变化**：depth 仅作用于当前网络浏览会话（不要求持久化到配置）；跳转详情后 seed 切换，再进网络按新 seed、默认 depth=2（或保留会话 depth，见技术方案：保留会话 depth）
- **异常/边界**：
  - seed 不存在 → 错误提示并可返回
  - 深度内无邻居 → 仅显示中心节点
  - 高度数 / 深度偏大导致节点多 → 依赖 Flow 缩放与 fitView；本迭代不做 Top-N 截断 UI
- **不应发生**：在图上改结构；无限递归；全图无 depth 限制加载

## Acceptance Criteria / 验收标准

- [ ] `get_network` 返回 `NeuronSubgraph`，边两端均在节点集合内
- [ ] Tauri / Store / Manager / AI tool / TUI / 前端类型 / 单测与新契约对齐
- [ ] 网络视图默认图；可切换树 / 图
- [ ] 图基于 Svelte Flow，可见有向边与权重信息
- [ ] 深度控件 1–5 可用，变更后重新拉取并刷新图
- [ ] 点击图节点可跳转详情
- [ ] 成环图不导致加载或渲染死循环
- [ ] 空态与错误态可用
- [ ] GUI 仍无法改边 / 改权重 / 创建删除节点

## Constraints / 约束

- 业务约束：`Spec is Truth`；只读浏览；不削弱现有列表/详情/编辑 `desc`/`content`
- 技术约束：Svelte 5 + Tauri 2；新增依赖 `@xyflow/svelte`；GUI depth 默认 `2`、范围 `1..5`；AI tool 默认 depth 仍可为 `3`（与现工具一致），但返回形态改为子图对象
- 时间/兼容性约束：本迭代仍是局部 ego-network，不为全图探索引入额外命令

## Open Questions / 开放问题

- [x] Q1 API 形态：演进 `get_network` → 子图
  - 用户确认：演进
- [x] Q2 前端渲染：Svelte Flow；depth 可控
  - 用户确认：Svelte Flow，depth 可控（1–5，默认 2）
- [x] Q3 网络视图默认：图
  - 用户确认：默认图

## Requirement Decisions / 需求决策

- 2026-07-30 22:59: 需求与方案草稿创建。
- 2026-07-30 23:06:
  - Q1=演进 `get_network`；Q2=Svelte Flow + depth 可控；Q3=默认图。
  - depth GUI：`1..5`，默认 `2`；变更触发重新拉取。
  - 待用户批准执行后进入 `executing`。

## Display Contract / 展示约定

### 主列表 `NeuronList`（保持）

- 主行：`desc`
- 次要：`weight`、短 `id`、`system_type`（有则标签）、`created_at`
- 不上列表：`content`、`tool_ids`、`updated_at`

### 网络树列表（与图互补）

- 深度标记（相对 seed：`D0`/`D1`/…）或缩进
- `desc`；与父/seed 相关的边权；出入方向（←/→ 或 in/out）
- `system_type`（有则）；seed 高亮；可选短 id
- 不上树列表：`content`

### Flow 节点

- `desc` + `weight` + `system_type` 角标；seed 高亮
