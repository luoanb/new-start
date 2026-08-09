# Spec: 神经元画布工具栏 · 布局算法可插拔 · 视觉主次

## Goal

- 要解决什么问题：当前神经元画布只有一个力导向布局，节点视觉编码只有 seed 粗边框 + 文本权重，全图"人人平等"，用户找不到主要链路与主次关系；顶栏核心多选同时充当"图展开根"和"核心意图"，语义耦合，画布内无法独立指定 seed。
- 验收结果：
  1. 布局算法封装为可插拔 registry，内置「力导向 / 分层」两种，用户在顶栏（连线方式左侧）切换，切换后立即重新布局；
  2. seed 与顶栏核心选择解耦为两个独立数据：顶栏 `coreSelection` 变化单向同步到画布（重置画布 seed），画布内可独立切换 seed 并重新展开；
  3. 布局切换为全局操作，放在顶栏「连线方式」左侧的「布局」下拉；节点选中为节点级操作，**点击画布节点即选中**（`selectedId` + 打开抽屉 + 节点上方悬浮 SvelteFlow 原生 `NodeToolbar` 同时展示），工具栏仅含【设为画布核心】，点击画布空白收起；
  4. 节点视觉主次：节点大小 ∝ 归一化权重（140→260px），权重分档配色（高权重主色高饱和 / 中权重常规 / 低权重淡化）；
  5. `svelte-check` 0 error；现有功能（搜索/深度/连线类型/创建/抽屉）不回归。

## Done Contract

- 完成定义：
  1. `LayoutId = "force" | "layered"`；`layoutRegistry` 提供 `run(subgraph, opts)`；默认 `force`（行为与现状一致），选择持久化到 localStorage。
  2. `canvasSeed: string | null` 成为画布唯一展开根：`subgraph = BFS(canvasSeed, depth)`；顶栏 `coreSelection` 变化 → `canvasSeed = coreSelection[0]`（单向同步，不回写）。
  3. 节点尺寸统一函数 `nodeSizeFor(weight, minW, maxW)`（宽度 140 + norm*120，高度固定 56），布局斥力/碰撞估算与 `NeuronFlowNode` 渲染同源；`estimateNodeSize` 改用该函数。
  4. 权重分档配色：`norm ≥ 0.66` high、`≥ 0.33` mid、其余 low；`NeuronFlowNode` 按档位加 class，high 主色系、low 去饱和淡化；seed 保留粗边框 + 主色光晕（叠加在任何档位之上）。
  5. 顶栏「布局」下拉（`layoutOptions` 渲染，切换即 `layoutId` 更新 + localStorage 持久化）；画布节点悬浮工具栏（SvelteFlow `NodeToolbar`）：**点击节点即选中**（`selectedId` + 打开抽屉 + 工具栏同时展示，`elementsSelectable=false`），工具栏仅含【设为画布核心】（`canvasSeed = 节点`）。
- 由什么证明：`svelte-check` 0 error；手动验证清单（见 Validation）。
- 哪些情况仍算未完成：backbone/主干链路布局与高亮（后续迭代，接口已预留）；多根（集合）展开；NeuronNetwork 树视图与图视图 seed 联动；depth/edgeType 迁入工具栏面板。

## Scope

- In：
  - `networkLayout.ts`：`LayoutId` / `LayoutAlgorithm` / `layoutRegistry`；`layoutForceNodes` 与 `layoutFlowNodes` 适配为两个实现；`nodeSizeFor` 统一尺寸；`estimateNodeSize` 同步。
  - `NeuronManager.svelte`：`canvasSeed` 状态、`coreSelection → canvasSeed` 同步、`buildSubgraph` 改以 `canvasSeed` 为根；`layoutId` 状态（localStorage）。
  - `NeuronNetworkGraph.svelte`：接收 `layoutId`、`seedId`；按 registry 运行布局；边粗映射保持；集成 SvelteFlow 原生 `NodeToolbar`（节点悬浮工具栏）。
  - `NeuronFlowNode.svelte`：接收归一化权重/档位，动态宽度 + 分档配色 + seed 光晕。
  - `NeuronManager.svelte` 顶栏：`edge-type` 左侧新增「布局」下拉（`Select` + `layoutOptions`）。
  - `translations.ts`：neuronPanel 新增/调整 key（`layoutLabel`/`layoutForce`/`layoutLayered`/`selectNode`/`setAsSeed`）。
- Out：主干链路高亮；树视图联动；多根展开；depth/edgeType 移入下拉；布局算法内置更多（radial 等）。

## 背景与现状

- 现状（[NeuronManager.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src/lib/components/NeuronManager.svelte#L33-L116)）：
  - `coreSelection`（顶栏 Top60 多选）同时决定图展开根（`buildSubgraph` 以集合 BFS，`seed_id = coreSelection[0]`）与"核心意图"，空时自动回弹权重最高节点。
  - 布局固定 `layoutForceNodes`（[networkLayout.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src/lib/features/neuron/networkLayout.ts#L189-L344)）；`layoutFlowNodes`（分层）已存在但未被使用。
  - 节点渲染 [NeuronFlowNode.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src/lib/components/NeuronFlowNode.svelte)：固定 `min-width 140 / max-width 200`（宽度实际只由 label 长度决定），仅 `is-seed` 有 2px 主色边框；权重以 `w=3.5` 文本显示。边粗细已 ∝ 连接权重。
  - 用户决策：布局算法可插拔可选；seed 与顶栏核心解耦（顶栏单向同步到画布，画布内可切换 seed/选中）；视觉主次 = 大小∝权重 + 分档配色；本次先出 spec。

## 设计

### 1. 数据模型解耦

| 状态 | 含义 | 来源 |
|---|---|---|
| `coreSelection: string[]` | 顶栏核心意图（多选，Top60） | 顶栏 `MultiSelect`；不允许全空（保留回弹） |
| `canvasSeed: string \| null` | 画布展开根（单节点），**图数据的唯一权威来源** | 顶栏同步或工具栏【设为画布核心】 |
| `selectedId: string \| null` | 当前选中节点（抽屉/高亮） | 节点点击或工具栏【选中】 |

- 同步规则（单向，顶栏 → 画布）：
  - `coreSelection` 变化时：`canvasSeed = coreSelection[0]`（无 → 回弹 top1 后取），随后重建 subgraph。
  - 画布内切换 seed：只改 `canvasSeed`，不回写 `coreSelection`。
- 重建：`subgraph = BFS(canvasSeed, depth)`，可见性过滤沿用 `visibleIds`；`seed_id = canvasSeed`。

### 2. 布局算法可插拔

```ts
// features/neuron/networkLayout.ts
export type LayoutId = "force" | "layered";

export type LayoutOptions = {
  seedId: string;
  nodeSize: (id: string) => { w: number; h: number }; // 与渲染同源的尺寸
};

export type LayoutAlgorithm = {
  id: LayoutId;
  labelKey: string; // i18n key
  run: (subgraph: NeuronSubgraph, opts: LayoutOptions) => LayoutNode[];
};

export const layoutRegistry: Record<LayoutId, LayoutAlgorithm> = {
  force:   { id: "force",   labelKey: "neuronPanel.layoutForce",   run: runForceLayout },
  layered: { id: "layered", labelKey: "neuronPanel.layoutLayered", run: runLayeredLayout },
};
export const layoutOptions: LayoutAlgorithm[] = Object.values(layoutRegistry);
```

- `runForceLayout` = 现有 `layoutForceNodes` 改造：`estimateNodeSize` → 传入 `opts.nodeSize`；seed 布局相关逻辑不变。
- `runLayeredLayout` = 现有 `layoutFlowNodes` 适配：`LayoutNode.data` 增补 weight/systemType/isSeed（与 force 对齐）；水平按 depth 分层、每层按 weight desc 纵向排列，起点 `seedId` 所在层为第 0 层。
- 选择持久化：`layoutId` 读写 localStorage（key `neuron-canvas-layout`），无值回退 `force`。

### 3. 节点尺寸 ∝ 权重（布局与渲染同源）

```ts
export function weightNorm(weight: number, minW: number, maxW: number): number {
  const span = maxW - minW || 1;
  return (weight - minW) / span; // 0..1（单节点图为 1）
}
export function nodeSizeFor(weight: number, minW: number, maxW: number): { w: number; h: number } {
  return { w: Math.round(140 + weightNorm(weight, minW, maxW) * 120), h: 56 }; // 140→260
}
```

- `NeuronManager` 计算 subgraph 内 `minW/maxW`，与 `NeuronNetworkGraph` 同传给 `layoutRegistry.run` 与节点渲染。
- `NeuronFlowNode` 渲染宽度用同一函数结果（inline `style="width:{w}px"`），`estimateNodeSize` 删除/改为使用 `nodeSizeFor`（label 长度不再决定宽度）。

### 4. 权重分档配色 + seed 高亮

- 档位（按 `norm`）：`high ≥ 0.66`、`mid ≥ 0.33`、`low < 0.33`。
- 样式（沿用现有 CSS 变量与 color-mix 惯例）：

```css
.neuron-flow-node.tier-high {
  background: color-mix(in srgb, var(--color-primary) 18%, var(--color-surface));
  border-color: var(--color-primary);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-primary) 35%, transparent);
}
.neuron-flow-node.tier-mid { /* 现状默认：surface + 灰边框 */ }
.neuron-flow-node.tier-low { opacity: 0.72; filter: saturate(0.5); }
.neuron-flow-node.is-seed { border-color: var(--color-primary); border-width: 2px; box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-primary) 22%, transparent); }
```

- `data.weightNorm`（0..1）随节点下发，档位由组件计算；seed 光晕叠加在任意档位之上（is-seed 规则后置）。

### 5. 交互设计：全局布局切换（顶栏）+ 节点级悬浮工具栏

- **布局切换（全局操作，顶栏）**：在顶栏「连线方式」（`edge-type`）左侧新增「布局」分组（`.layout-type`），用现有 `Select` 组件渲染 `layoutOptions`（力导向 / 分层）。切换 → `layoutId` 更新 + `writeLayoutPref` 持久化 → `NeuronNetworkGraph` 按新算法立即重新布局。
- **节点选中（节点级操作，画布悬浮工具栏）**：使用 SvelteFlow 原生 `NodeToolbar`（@xyflow/svelte v1.6.2 已导出），悬浮在节点上方（`Position.Top` / `align=center` / `offset=14`）：
  - 点击画布节点 → `onnodeclick` 设置 `toolbarNodeId` **并立即选中**：`onJumpTo(id)` → `selectedId = 节点` + 打开 `NeuronDetailDrawer`；工具栏与该抽屉同时展示（抽屉固定在画布右侧，工具栏悬浮在节点上方，均由同一节点触发）。
  - 点击画布空白 → `onpaneclick` 清除 `toolbarNodeId`，工具栏收起。
  - 工具栏仅含一个按钮：【设为画布核心】（`onSetSeed(toolbarNodeId)`）→ `canvasSeed = 节点` → 以新根重建 subgraph。
  - 无 `onSetSeed` 的消费方（`NeuronNetwork` 二级视图，无 seed 概念）不渲染工具栏；`elementsSelectable={false}` 关闭库自动选中，`selected` 完全由 `selectedId` 外部控制。
- 实现要点：`toolbarNodeId` 用 `{#if}` 包裹 `NodeToolbar`（库内部要求 `nodeId` 非空，避免空引用抛错）；工具栏容器 `use:portal={'root'}`，节点拖动时 `transform` 跟随节点。

### 6. i18n（neuronPanel）

| key | en | zh | 用途 |
|---|---|---|---|
| layoutLabel | Layout | 布局 | 顶栏布局分组标签 |
| layoutForce | Force-directed | 力导向 | 布局下拉选项 |
| layoutLayered | Layered | 分层 | 布局下拉选项 |
| setAsSeed | Set as seed | 设为画布核心 | 节点工具栏【设为画布核心】按钮 |

> 废弃 key（已删除）：`canvasToolbar` / `currentSeed` / `clearSeed` / `selectAndOpen` / `currentSelected`（齿轮浮层方案取消）；`selectNode`（工具栏仅保留【设为画布核心】）。

## 接口契约

```ts
// NeuronManager.svelte
let canvasSeed = $state<string | null>(null);
let layoutId = $state<LayoutId>(readLayoutPref());

// coreSelection → canvasSeed 单向同步
$effect(() => {
  coreSelection;
  canvasSeed = coreSelection[0] ?? null; // 顶栏已保证非空
});

// 重建（以 canvasSeed 为根）
$effect(() => {
  if (!canvasSeed || !visibleIds.has(canvasSeed)) return; // seed 被过滤时保持现状
  subgraph = buildSubgraph(canvasSeed, depth);
});

// NeuronNetworkGraph
let { subgraph, layoutId, seedId, nodeSize, onJumpTo, onSetSeed, selectedId } = $props();
const laid = $derived(layoutRegistry[layoutId].run(subgraph, { seedId, nodeSize }));

// 节点悬浮工具栏
let toolbarNodeId = $state<string | null>(null);
function onNodeClick(id: string) { toolbarNodeId = id; onJumpTo(id); } // 点击即选中 + 展开工具栏
function onPaneClick() { toolbarNodeId = null; }                      // 点击空白收起
// {#if toolbarNodeId && onSetSeed}<NodeToolbar nodeId={toolbarNodeId} isVisible position={Position.Top}>【设为画布核心】</NodeToolbar>{/if}
```

## 兼容性

- 默认 `layoutId = force`、默认 seed 行为 = 现状（coreSelection[0]），无感知迁移。
- `layoutFlowNodes` 由"未被使用"转为 `layered` 实现，无外部调用受影响。
- `LayoutNode.data` 增补字段对既有消费方（NeuronFlowNode）向前兼容。
- localStorage 新增 key 无旧值冲突。

## Validation

- `svelte-check` 0 error。
- 手动清单：
  - 顶栏勾选/取消核心 → 画布 seed 同步更新并重新展开；
  - 顶栏「布局」下拉切换「力导向 / 分层」→ 布局立即变化并持久化（刷新后保留）；
  - 点击画布节点 → 节点立即选中（选中高亮 + 抽屉打开）+ 节点上方悬浮工具栏出现；点击画布空白 → 工具栏收起（抽屉保持打开）；
  - 工具栏【设为画布核心】→ 以该节点为根重新 BFS，seed 高亮 + 光晕；
  - 高权重节点明显更大且主色，低权重节点小且淡化；seed 始终可辨；
  - 搜索/深度/连线类型/创建/抽屉等既有功能不回归。

## Open Questions

- [ ] 多根（集合 BFS）展开是否保留为画布内可选项？（本 spec 采用单根 canvasSeed）
- [ ] backbone（主干链路）布局：后续迭代实现，是否需在本 spec 的布局接口中加入"主轴长度/层数"等专属 options？

## Change Log

- 2026-08-09：初始 spec。决策：布局算法可插拔（force + layered）；`canvasSeed` 为画布唯一展开根，顶栏 `coreSelection` 单向同步；节点大小∝权重 + 分档配色；画布工具栏抽屉式浮层含布局选择与【选中/设为画布核心】；backbone/主干链路与树视图联动列为后续。
- 2026-08-09（实现）：`networkLayout.ts` 重构为 `LayoutId`/`LayoutAlgorithm`/`layoutRegistry`，`runForceLayout`/`runLayeredLayout` 两个实现，`weightNorm`/`nodeSizeFor` 统一尺寸（140→260px），`readLayoutPref`/`writeLayoutPref` 持久化；`NeuronManager` 新增 `canvasSeed`（顶栏单向同步、画布内可切换）与 `layoutId`，`buildSubgraph` 改单根 BFS，`minW/maxW` 归一化；`NeuronNetworkGraph` 按 registry 运行布局并复用尺寸函数；`NeuronFlowNode` 动态宽度 + 高/中/低分档配色 + seed 光晕；新增 `NeuronCanvasToolbar` 浮层（布局单选 + 节点选择器【设为画布核心/选中并打开】+ 当前 seed/选中展示）；i18n 新增 10 个 key。`svelte-check` 0 error（47 既有 warning）。
- 2026-08-09（交互修正，用户否掉齿轮浮层）：布局切换为**全局操作**，改为顶栏「连线方式」左侧的「布局」下拉（`Select` + `layoutOptions`，切换即重新布局并持久化）；节点选中为**节点级操作**，改为 SvelteFlow 原生 `NodeToolbar` 悬浮在节点上方（点击节点展开、不改变选中态 `elementsSelectable=false`、点击空白收起），工具栏含【选中】（选中并打开抽屉）与【设为画布核心】（`canvasSeed = 节点`）；删除 `NeuronCanvasToolbar.svelte` 与废弃 i18n key（`canvasToolbar`/`currentSeed`/`clearSeed`/`selectAndOpen`/`currentSelected`）；`selectNode` 文案改为「选中」。seed 不再有独立搜索选择器（由画布节点/顶栏设置），原 Open Question 随之关闭。
- 2026-08-09（交互再调整）：**点击节点即选中**——`onnodeclick` 同时设置 `toolbarNodeId` 并 `onJumpTo(id)`（选中高亮 + 打开抽屉），节点悬浮工具栏与抽屉同时展示；工具栏仅保留【设为画布核心】一个按钮，无 `onSetSeed` 的视图（`NeuronNetwork`）不渲染工具栏；删除 i18n key `selectNode`。`svelte-check` 0 error。
- 2026-08-09（性能修复）：点击节点卡顿的根因是 `NeuronNetworkGraph.rebuild()` 内部读取 `selectedId`，被 Svelte 5 `$effect` 动态追踪——每次点击选中都触发全量 `rebuild`，力导向布局（400 次迭代 O(n²) 斥力+碰撞）同步重跑阻塞主线程，分层布局开销可忽略所以无感。修复：`rebuild` 内用 `untrack(() => selectedId)` 读取选中态，布局 effect 不再依赖 `selectedId`；选中态仍由独立 effect 仅更新 `selected` 字段。`svelte-check` 0 error。

