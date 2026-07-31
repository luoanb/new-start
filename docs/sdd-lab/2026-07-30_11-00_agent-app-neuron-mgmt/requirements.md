# Requirements / 需求文档: agent-app-neuron-mgmt

## Restated Understanding / 需求复述

- 我理解当前需求是：在 agent-app GUI 中新增 Neuron（知识图节点）管理界面，提供对 Agent 知识图谱的可视化管理能力。Neuron 是 Assistant 模式中知识表达和课题驱动的核心数据结构，以图结构组织（节点 + 连接）。
- 当前核心目标是：将 TUI 中通过 `/neuron` 命令管理的功能搬到 GUI，提供 Neuron 列表/详情/图结构可视化/连接管理界面。
- 当前边界是：Rust 后端 NeuronStore / NeuronManager 已完整实现，需新增 Tauri 命令暴露到前端。UI 复杂度较高（涉及图结构展示和节点关系可视化），保留 CLI/TUI 作为备选入口。
- 暂不处理：Neuron 的自动创建流程（由 Agent 运行时的 `create_downstream_neuron` tool 自动管理）、Neuron 的权重训练/调整算法、知识图谱自动布局算法的高级定制。

## Scope / 范围

### In

1. **Rust 后端 — 新增 Tauri 命令**
   - `list_neurons -> Vec<Neuron>`：列出所有 Neuron 节点
   - `get_neuron(id: string) -> Neuron`：获取单个 Neuron 详情
   - `update_neuron(id: string, desc?: string, content?: string) -> Neuron`：更新 Neuron 描述或内容
   - `get_connections(id: string) -> Vec<Connection>`：获取指定 Neuron 的出入连接
   - `get_network(id: string, max_depth: number) -> Vec<Neuron>`：获取指定 Neuron 的图网络（广度遍历）

2. **Neuron 列表与详情界面**
   - Neuron 列表展示：ID、描述摘要、权重、system_type 标签、创建时间
   - 按权重排序（默认降序）
   - 点击列表项进入详情视图
   - 详情视图展示：完整描述、内容（可滚动查看）、权重、system_type、tool_ids、时间戳
   - 编辑：支持编辑描述和内容字段

3. **Neuron 连接与图结构可视化**
   - 在详情视图中展示该 Neuron 的连接列表（source → target，含权重）
   - 网络视图：以指定 Neuron 为中心，显示其图网络（最多 2-3 层深度）
   - 图网络以列表或树形结构展示（非可视化画布），格式：`Neuron A ──(weight: 1.0)──> Neuron B ──(weight: 0.8)──> Neuron C`
   - 支持在网络视图中点击节点跳转到该节点的详情

4. **Polller 状态联动（保留）**
   - 如果 Neuron 管理界面需要访问 Poller 状态（查看 Assistant 模式是否在运行），可复用需求2 的 `poll_status` 命令

### Out

- 可视化图结构画布（D3.js / vis.js 等图可视化库 — 复杂度高，保留给未来迭代）
- Neuron 自动创建和权重调整（由 `create_downstream_neuron` / `select_neuron_candidates` 等 AI tools 管理，GUI 不干预自动流程）
- Neuron system_type 修改（系统内置类型，GUI 只读展示）
- Neuron tool_ids 修改（由系统管理）
- 批量操作（批量删除/权重更新）
- Neuron 候选推荐界面（由 `select_neuron_candidates` AI tool 完成）

## User Interaction / 用户交互

- **触发入口**：侧栏面板中新增 "Neurons" 标签页，或在导航中增加入口。也可以在课题详情/Assistant 模式相关界面中添加 Neuron 的快速入口链接。
- **用户操作路径**：
  1. 点击 "Neurons" 标签 → 展示 Neuron 列表（含权重、system_type、描述摘要）
  2. 点击列表项 → 进入 Neuron 详情视图
  3. 在详情中查看描述和内容（长内容滚动）
  4. 点击 "编辑" 按钮 → 描述和内容字段变为可编辑 → 保存后更新
  5. 查看连接列表 → 展示 source/target/weight
  6. 点击 "查看网络" 按钮 → 以当前 Neuron 为中心展开 2 层网络 → 以缩进树形展示
  7. 在网络视图中点击其他 Neuron 名称 → 跳转到该 Neuron 详情
- **系统反馈**：
  - 列表加载中显示 loading 状态
  - 详情加载中显示 skeleton 或 loading 指示器
  - 编辑保存成功 → 轻量提示，字段恢复只读
  - 编辑保存失败 → banner 展示错误信息
  - 网络视图加载 → 树形结构即时渲染
- **状态变化**：
  - Neuron 编辑后，列表项摘要同步更新
  - 网络视图为独立浏览流，不改变当前 Neuron 选择
- **异常/边界交互**：
  - Neuron 列表为空 → 空状态提示
  - 网络视图深度超过实际连接 → 尽最大深度展示，不报错
  - 删除/修改只读字段 → 操作禁用或展示只读提示
  - 连接目标 Neuron 已被删除 → 显示 "已删除" 占位
- **不应发生的交互**：
  - 用户可以通过 GUI 创建或删除 Neuron（此操作由 AI tools 管理）
  - GUI 修改 Neuron 的权重、system_type、tool_ids 等系统管理字段
  - 图网络展示渲染卡顿或无限循环（图结构可能成环）

## Acceptance Criteria / 验收标准

### 后端新增 Tauri 命令
- [ ] `list_neurons` 命令已注册，返回完整 Neuron 列表
- [ ] `get_neuron` 命令已注册
- [ ] `update_neuron` 命令已注册，支持 desc 和 content 部分更新
- [ ] `get_connections` 命令已注册
- [ ] `get_network` 命令已注册，支持 max_depth 参数

### Neuron 管理 GUI
- [ ] Neuron 列表展示 ID、描述摘要、权重、system_type、创建时间
- [ ] 支持权重降序排序
- [ ] 详情视图展示完整 Neuron 信息
- [ ] 详情内描述和内容字段可编辑保存
- [ ] 详情内连接列表展示
- [ ] 网络视图以缩进树形展示 2 层深度
- [ ] 网络视图中可点击跳转到其他 Neuron 详情
- [ ] 空状态有引导提示

## Constraints / 约束

- 业务约束：
  - 文档和代码冲突时，以文档为准，先同步文档再同步代码。
  - `Spec is Truth`，`No Spec, No Code`，`No Approval, No Execute`。
  - Neuron 管理属于辅助功能，CLI/TUI 保留作为备选入口。
- 技术约束：
  - 新增 Tauri 命令仅限于对 Gateway 现有方法（`neuron_store()`, `neuron_manager()`）的封装。
  - GUI 不应绕过 Tauri 命令直接访问 Rust 后端 state / storage。
  - 前端技术栈保持 SvelteKit + Svelte 5 + `@tauri-apps/api`。
  - 图网络使用缩进树形列表展示，不引入图可视化库（D3、vis.js 等）。
  - 前端 Neuron 类型定义需对齐 Rust `Neuron` / `Connection` 结构。
  - 网络视图需处理图结构中可能存在的环（visited set 防无限递归）。
- 时间/兼容性约束：
  - Neuron 管理界面为低频操作入口，核心交互路径（聊天/会话）不受影响。
  - 第一版仅展示和管理已有 Neuron，不提供自动化流程。

## Open Questions / 开放问题

- [ ] Q1 `get_network` 的 `max_depth` 默认值设为多少？GUI 中网络视图的最大深度限制？
  - 当前建议：默认 2 层（当前节点 → 直接连接 → 间接连接），用户无配置选项。
  - 触发来源：方案拟定
  - 影响范围：网络视图的展示范围和性能

## Requirement Decisions / 需求决策

- 2026-07-30 17:43:
  - 决策：需求文档初稿创建，待用户确认。
