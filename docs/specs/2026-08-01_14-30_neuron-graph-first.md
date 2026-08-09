# Spec: Neuron 面板 Graph-first 改版

## Goal

- 要解决什么问题：当前 Neuron 面板是「列表为主、网络图埋在详情二级页之后」的三态切换（`list → detail → network`）。神经元一多就变成同质化卡片墙，既看不出结构，也看不出关联关系；而「网络关系」恰恰是 Neuron 区别于普通记忆/笔记的核心价值，却要两层点击才看得到。
- 验收结果：Neuron 面板以网络图为主视图，列表退化为「紧凑索引 + 过滤器」，详情降级为「节点抽屉/浮层」，用户进入即见结构、点击即聚焦、数量多也不乱。

## Done Contract

- 什么算完成：
  - `NeuronManager` 改造为「索引侧栏 + 网络画布」同屏分栏，默认进入即为图视图。
  - 列表项点击 = 在图中聚焦/高亮该节点并展开其邻居，而非页面跳转。
  - 节点点击 = 底部/浮层抽屉展示该神经元详情与编辑，不抢占主视图。
  - 提供全局构图能力（全量或 top-N + 邻居），并支持按 `system_type` 过滤与搜索。
  - 复用现有 `NeuronNetworkGraph` / `NeuronFlowNode` / `get_network`，尽量不改 Rust 后端（必要时仅补一个全局构图命令）。
- 由什么证明：本地 `pnpm --filter agent-app dev` 可在桌面端看到新布局；交互对照本 spec；linter 无错。
- 哪些情况仍算未完成：后端全局构图命令的算法优化（如簇聚合）、性能压测、空状态插画细化（Out，仅留极简空态）。

## Scope

- In:
  - `NeuronManager.svelte` 布局改造（三态切换 → 分栏同屏）
  - 新增/调整一个「紧凑索引侧栏」组件（复用 `NeuronList` 思路，降密度、加连接数徽章）
  - 新增「节点详情抽屉」组件（复用 `NeuronDetail` 字段与编辑逻辑，改为抽屉形态）
  - 网络画布交互增强：`focusView` 聚焦、节点 `selected` 联动、过滤驱动淡入淡出
  - 全局构图数据源（前端合并 `list_neurons` + 各 `get_connections`，或新增 `list_neurons_full`）
- Out:
  - 后端 `NeuronManager` 业务 API 重构（见 `2026-08-01_02-40_neuron-manager-api.md`）
  - 力导向算法/簇聚合的高阶优化
  - 移动端适配（<800px 侧栏折叠沿用现有规则）
  - 节点拖拽持久化（仅允许临时拖拽，不落库）

## Facts / Constraints

- 已确认事实：
  - 现有组件已具备 Graph-first 的底座：`NeuronNetworkGraph.svelte` 用 `@xyflow/svelte`，支持 `fitView`、节点点击、`nodeTypes` 自定义节点 `NeuronFlowNode`；`NeuronNetwork.svelte` 已有 `maxDepth` 与 `graph/tree` 切换；`NeuronDetail.svelte` 已有完整字段与编辑逻辑；`NeuronList.svelte` 已有列表渲染。
  - 现有命令：`list_neurons`（全量列表）、`get_neuron`、`get_connections(id)`、`get_network(id, max_depth)`（局部子图）。GUI 目前只消费 list/get/update/connections/network（见 `neuron-manager-api.md` 事实）。
  - 产品定位：Assistant 模式下 Neuron 驱动课题深入，网络关系是核心价值；UI 遵循 `DESIGN.md` 三栏哲学（chat-first / 呼吸感 / 模式可见 / 克制 / 反馈先于形式）。
- 技术/业务约束：
  - `No Spec, No Code` / `No Plan Approved, No Execute`：本 spec 批准后才能进入实现。
  - 动效只动 opacity / transform（DESIGN.md Motion 约束）。
  - 主题变量沿用 `--color-*` tokens，深浅双主题独立辨识。
  - 尽量不新增 Rust 命令；若必须，仅补 `list_neurons_full` 返回 `neurons + all_connections`。
  - 性能：神经元数百+ 时不可一次性渲染全部节点。默认只渲染 top-N（按 weight）高权重节点 + 其直接邻居；过滤/搜索时实时调整可见集合。
- 已知风险：
  - 全局图节点过多导致力导向卡顿 → 用 top-N + 邻居 + 簇着色缓解；后续可加虚拟渲染。
  - 纯图视图对「只想快速找某神经元」的用户不够快 → 索引侧栏必须保留且好用（搜索 + 过滤 + 点击聚焦）。
  - 初始进入若撒满全屏会显得乱 → 进入时 `fitView` 到权重最高簇，而非全部铺开。

## Restated Understanding

- 我理解当前任务是：把已对齐的「Graph-first」产品口径固化为 Neuron 面板交互 spec，供后续 `NeuronManager` 等组件实现对照。
- 当前核心目标是：图为主视图、列表为索引、详情为抽屉，三者同屏互补，解决「列表多了不好看、关系看不到」的痛点。
- 当前边界是：只落 spec，不写 technical-plan 执行、不改代码。
- 暂不处理：后端算法重构、性能压测、移动端专项。

## 目标布局

```
┌───────────────────────────────────────────────────────────┐
│  Neuron    [搜索框]  [system_type 过滤 chips]  [深度滑块]   │ ← 顶部工具条
├──────────────┬────────────────────────────────────────────┤
│  索引侧栏     │   网络画布（主视图，占 ~70% 宽度）          │
│  (紧凑列表)   │   - 节点 = 神经元（按 system_type 着色）    │
│  · 名称+权重条 │   - 边 = 连接（粗细 = 权重）               │
│  · 连接数徽章  │   - 进入即 fitView 到高权重簇              │
│  · 可折叠分组  │   - 点节点 → 底部抽屉看详情/编辑           │
│  · hover高亮   │   - 点列表项 → 画布聚焦该节点+展开邻居     │
│               │   - 点空白 → 收回抽屉                       │
└──────────────┴────────────────────────────────────────────┘
        │
        │ 点击节点 → 底部抽屉（不抢占主视图）
        ▼
┌───────────────────┐
│  Neuron 详情抽屉   │  desc / content / 连接 / 时间戳 / 编辑
└───────────────────┘
```

## 交互规格

### 1. 默认视图 = 网络画布
- 进入 Neuron 面板即渲染全局图（top-N 高权重节点 + 邻居）。
- 调用 `fitView` 自动聚焦到权重最高的若干簇，避免全屏铺开。
- 若无可渲染节点（空库），显示 DESIGN.md 风格极简空态：「暂无神经元 / 在 Assistant 模式下对话以生成」+ 单一操作按钮。

### 2. 索引侧栏（列表降级）
- 复用 `NeuronList` 渲染思路，但降密度：每行仅「名称（截断）+ 权重条 + 连接数徽章」；`system_type` 作为左侧色条/小标。
- 支持按 `system_type` 折叠分组（沿用方案 C 的「可折叠树/分组」）。
- 点击行 → **不跳转页面**，改为在画布中对应该节点 `selected=true` 并 `focusView`，同时展开其 1 跳邻居（驱动 `get_network(id, 1)` 或前端过滤）。
- hover 行 → 画布中对应节点高亮（opacity/描边变化，仅 transform/opacity，符合动效约束）。

### 3. 节点详情 = 抽屉（非页面）
- 点击画布节点 → 底部弹出抽屉（全宽，固定高度约画布一半；标题栏图标按钮可在「右侧 / 底部」两种停靠方式间切换），内容与 `NeuronDetail` 一致：desc / content / 连接列表 / 时间戳 / tool_ids / 编辑。
- 编辑逻辑直接复用 `NeuronDetail` 的 `handleSave`（调用 `update_neuron`）。
- 点画布空白或抽屉关闭按钮 → 抽屉收回，节点取消选中。
- 抽屉内的「查看网络」按钮移除（已无意义，图本身是主视图）。

### 4. 过滤与搜索
- 顶部工具条：`system_type` 过滤 chips（多选）+ 文本搜索（匹配 desc/id）+ `maxDepth` 滑块（控制「全局图中展开几跳」）。
- 过滤/搜索变化 → 画布节点集合实时淡入淡出（opacity transition，250ms ease-out），索引侧栏同步过滤。
- 深度滑块沿用 `NeuronNetwork` 现有 `DEPTHS = [1..5]` 语义，但作用对象变为全局图展开深度。

### 5. 视觉与主题
- 节点着色按 `system_type`（新增一组语义色，沿用 `--color-*` token 体系，深浅双主题各自定义）。
- 高权重节点尺寸/描边略增强（视觉层级），但不炫技（符合 Anti-references：无玻璃拟态、无渐变装饰）。
- 边粗细 = 连接权重（沿用 `NeuronNetworkGraph` 的 `stroke-width` 映射）。
- 选中节点描边用 `--color-primary`，与 `NeuronFlowNode.is-seed` 风格统一。

## 实现要点（落地指引，非执行）

### 前端
- `NeuronManager.svelte`：
  - 去掉 `view: list|detail|network` 三态；改为 `selectedId`、`drawerOpen`、`filters` 状态。
  - 布局改为 CSS grid：`[索引侧栏 260px] [画布 1fr]`，抽屉用绝对定位覆盖底部。
  - 数据：一次性 `list_neurons` 拿全量 → 前端构建 `nodes/edges`；连接数由 `get_connections` 或 `get_network` 填充（优先新增 `list_neurons_full`）。
- 新增 `NeuronIndex.svelte`（紧凑索引侧栏，基于 `NeuronList` 改造）。
- 新增 `NeuronDetailDrawer.svelte`（基于 `NeuronDetail` 改造为抽屉形态）。
- `NeuronNetworkGraph.svelte`：增加 `selectedId` / `focusId` props，支持外部聚焦与选中联动；保留 `onnodeclick → onJumpTo(id)`（改为打开抽屉）。
- 动效：仅 opacity/transform；节点淡入淡出用 CSS transition。

### 后端（可选，最小）
- 仅当前端合并 `list_neurons` + 多次 `get_connections` 成本过高时，新增 Tauri 命令 `list_neurons_full() -> { neurons: Neuron[], connections: Connection[] }`，复用现有 `NeuronManager` 方法，**不改 schema**。

## 验收清单

- [ ] 进入 Neuron 面板默认显示网络画布（非列表）。
- [ ] 索引侧栏可点击聚焦节点，画布 `fitView` 到该节点及邻居。
- [ ] 点击画布节点弹出底部抽屉，展示并可编辑详情；关闭后画布正常。
- [ ] `system_type` 过滤 + 搜索 + 深度滑块实时联动画布与侧栏，仅用 opacity/transform 动效。
- [ ] 空库时显示极简空态。
- [ ] 深浅双主题下节点着色与描边清晰可辨。
- [ ] linter 无错；`pnpm --filter agent-app dev` 可运行。
- [ ] 未新增 Rust 命令（或仅新增 `list_neurons_full` 且无 schema 变更）。
