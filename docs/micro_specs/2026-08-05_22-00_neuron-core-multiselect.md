# Spec: 神经元核心节点多选（Top60）

## Goal

* 要解决什么问题：当前全局图的核心写死 `TOP_N=60`，且构图逻辑"核心 + 无条件全部 1 跳邻居"导致 **depth 深度滑块失效**——真实数据为星型 hub 图（179 节点、核心 1 跳覆盖全图），depth=1..5 算出的都是同一张图。用户希望在顶栏用**多选下拉自己挑核心节点**：下拉列出 top-60 高权重神经元，勾选哪些节点，画布就以哪些节点为核心、按 depth 展开子图。

* 验收结果：顶栏出现 Top60 多选下拉（默认勾选权重最高的第一个节点）；勾选节点后画布仅显示"以勾选节点为核心、depth 跳内"的子图；depth 滑块真正生效；多选多个核心显示多簇并集；过滤/搜索仍作用于最终显示。

## Done Contract

* 什么算完成：

  1. 新增 `MultiSelect.svelte` 多选下拉组件（浮层带勾选态、portal 定位，参照 `Select.svelte` 结构）。
  2. `NeuronManager` 新增 `coreSelection: string[]` 状态，默认 `[权重最高节点 id]`；顶栏渲染 Top60 多选下拉。
  3. `buildSubgraph` 重构：核心 = 勾选节点集合；展开 = `pruneByDepth(coreSelection, depth)`；**移除** `nodeIds` 无条件 1 跳邻居与 `TOP_N` 常量。
  4. i18n 新增 core 多选相关 key（中/英）。
  5. 全取消勾选兜底：不允许全空，自动保留最后一项。

* 由什么证明：`pnpm --filter agent-app check` 0 error；App 内：拖动 depth 图有梯度变化、勾选不同核心画布切换、多选显示并集。

* 哪些情况仍算未完成：后端 `get_network` 重构；核心选择持久化；拖拽持久化。

## Scope

* In：新增 `packages/agent-app/src/lib/components/MultiSelect.svelte`；改 `NeuronManager.svelte`（状态 + 顶栏 + buildSubgraph）；改 `lib/i18n/translations.ts`。

* Out：后端 Rust 不动；`Select.svelte`（单选，别处使用）不动；`NeuronNetworkGraph` / `networkLayout` 不动；depth 语义不变。

## Facts / Constraints

* **根因（已用真实数据验证）**：`app.db` 中 179 神经元 / 173 连接，星型 hub 结构（最大度数 83）；top-60 核心的 1 跳邻居已覆盖全部 179 节点，故 `pruneByDepth(core, d)` 对 d=1..5 均返回全图 → depth 滑块无效。[NeuronManager.svelte:58-72](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src/lib/components/NeuronManager.svelte) 中 `nodeIds` 无条件加入全部 1 跳邻居是本问题的直接来源。

* **top-60 列表**：按 `weight` 降序取全部神经元前 60 个（若总数 <60 则全列）；下拉 label 用 `desc || id`。

* **默认选第一个**：权重最高的神经元。

* **depth 语义不变**：`pruneByDepth(coreIds, depth)` 从核心集合 BFS 展开 depth 跳（沿用现有函数，仅 core 来源改变）。

* `Select.svelte` 为单选且被 `NeuronNetwork.svelte` 等使用，复用其 portal/浮层/键盘导航结构新建 `MultiSelect.svelte`，不改旧组件。

## 接口契约设计

### 前端

```ts
// 新增 lib/components/MultiSelect.svelte —— 多选下拉
let {
  value = $bindable(), // string[]
  options,             // { value: string; label: string }[]
  placeholder = "",
  align = "left",
  disabled = false,
  onchange,
}: {
  value?: string[];
  options: { value: string; label: string }[];
  placeholder?: string;
  align?: "left" | "right";
  disabled?: boolean;
  onchange?: (values: string[]) => void;
} = $props();
```

* trigger 文案：已选 0 项显示 placeholder；已选 ≥1 显示 `已选 N · {第一项 label}`。

* 浮层选项点击 = toggle 选中态（不关闭浮层）；checkbox 样式由 `class:checked` 控制。

* 键盘导航 / portal / backdrop 复用 `Select.svelte` 现有实现模式。

### NeuronManager 改造

```ts
// 核心选择：默认权重最高的节点
let coreSelection = $state<string[]>([]);
const topNeurons = $derived(          // top-60 下拉候选（全部神经元，权重降序）
  [...neurons].sort((a, b) => b.weight - a.weight).slice(0, 60)
);
// load() 完成后若 coreSelection 为空 → 默认选中第一项
function ensureDefaultCore() {
  if (coreSelection.length === 0 && topNeurons.length > 0) {
    coreSelection = [topNeurons[0].id];
  }
}

function buildSubgraph(): NeuronSubgraph {
  if (coreSelection.length === 0) return empty;
  const coreIds = new Set(coreSelection);
  // 核心 + depth 跳内（沿用 BFS 剪枝；移除原 nodeIds 无条件 1 跳展开）
  const finalIds = pruneByDepth(coreIds, depth);
  const subNeurons = filteredNeurons.filter((n) => finalIds.has(n.id));
  const subConns = allConnections.filter(
    (c) => finalIds.has(c.source) && finalIds.has(c.target),
  );
  return { seed_id: coreSelection[0], neurons: subNeurons, connections: subConns };
}

// 全取消兜底：保持最后一项
function toggleCore(id: string) {
  coreSelection = coreSelection.includes(id)
    ? coreSelection.filter((x) => x !== id)
    : [...coreSelection, id];
  if (coreSelection.length === 0) coreSelection = [id]; // 不允许全空
}
```

* 顶栏在 search 后新增：`<MultiSelect bind:value={coreSelection} options={topNeurons.map(...)} />`，label 前缀 `t("neuronPanel.coreSelect")`（"核心"）。

* 过滤/搜索：`filteredNeurons` 仍用于最终显示与 `visibleIds` 限制，选中但被搜索过滤掉的节点自动不出现在画布（不报错）。

### i18n（translations.ts）

```ts
// zh: coreSelect: "核心"; en: coreSelect: "Core"
// 触发按钮计数文案用现有 t() 拼接（"已选 {n}"）
```

## Open Questions

* [x] 下拉候选基于全部神经元（非过滤后）：确认，选全部 top-60。

* [ ] 全取消勾选：默认自动回弹保留最后一项（不允许全空）。

## Restated Understanding

* 我理解当前任务是：顶栏新增 Top60 多选下拉列出高权重节点，勾选决定画布"核心集合"，画布 = 核心 + depth 跳内子图；depth 滑块保留并真正生效；移除写死的 TOP\_N 与无条件 1 跳邻居。

* 当前核心目标是：核心由用户可控、depth 有效、多选并集。

* 当前边界是：不做后端重构、不做核心持久化、不动单选 Select。

* 暂不处理：选择持久化、拖拽持久化。

## Goal Alignment Check

* 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。

* 若否：N/A。

## Checkpoint Summary

- 当前任务理解：Top60 核心多选下拉，勾选节点作为核心按 depth 展开。
- 当前核心目标：depth 生效 + 核心可控 + 多选并集。
- 当前进度：实现完成，静态检查 + 真实数据模拟通过。
- 下一步 1：用户 App 内打开神经元面板观察：顶栏核心下拉、拖 depth 有梯度、多选并集。
- 下一步 2：确认交互手感与图规模是否合适。
- 验证方式：`pnpm run check`（0 error）+ 真实数据模拟（depth 梯度/多选并集）。
- Execution Approval: 已批准（2026-08-05）。

## Change Log

- 2026-08-05: 初始 micro-spec。决策：Top60 多选下拉替代写死 TOP_N；核心=勾选节点，展开=pruneByDepth(core, depth)，移除无条件 1 跳邻居（根治 depth 失效）。
- 2026-08-05（实现）：新增 `MultiSelect.svelte`（portal 浮层、checkbox 勾选、全选/清空、键盘导航）；`NeuronManager` 新增 `coreSelection`/`topNeurons`/`coreOptions`，重构 `buildSubgraph`（核心=勾选集合 + pruneByDepth），默认核心由 `$effect` 兜底为权重最高节点、全取消自动回弹；i18n 新增 `common.selected` / `common.selectAll` / `neuronPanel.coreSelect`。

## Validation

- Self-check：实现完成。MultiSelect 复用 Select 的 portal/浮层模式，`value` 为 `$bindable` 且内部用 `selected = value ?? []` 处理可选 prop；NeuronManager 中 `nodeIds` 无条件 1 跳邻居与 `TOP_N` 常量已移除。
- Static checks：`pnpm --filter agent-app check` 0 error（43 既有 warning，未新增）。
- Runtime / Test（真实数据 `app.db` 模拟前端 BFS 构图）：
  - 默认核心（权重最高节点"关键词学习法AI助手"）depth=1/2/3 → 节点 24/113/139，梯度明显（修复前 depth=1..5 恒为 179 全图）；
  - 多选权重前二核心 depth=1/2/3 → 106/133/139，并集正确；
  - 3 跳后饱和（139<179）属星型图特性，部分未连通的孤立节点不显示，符合"按深度查表"语义。
- Human confirmation：micro-spec 已获批准；App 内视觉验证待用户进行。
- 结果汇总：代码完成，静态检查 + 真实数据模拟通过；UI 观感待用户确认。
- 核心目标是否已由证据证明完成：depth 生效已用真实数据证明；视觉交互需人工确认。
- 若未完成，当前剩余差距：仅剩 App 内观察顶栏下拉交互与画布观感。
- 剩余风险：多选下拉 60 项浮层滚动（max-height 260px，无需虚拟化）；默认核心为单一 hub 时 depth≥3 已饱和（预期，用户换核心或降 depth 可控）。

## Resume / Handoff

- 当前状态：实现完成，静态检查 + 真实数据模拟通过，待 App 内视觉验证。
- 当前卡点：无。
- 下一步唯一动作：用户 App 内观察顶栏 Top60 多选下拉、拖 depth 梯度、多选并集；如交互/观感需调整，反馈后微调。
- 下一轮核心目标：depth 生效、核心可控、多选并集。

