# Spec: 神经元网络图布局引擎升级（防拥挤）

## Goal

- 要解决什么问题：Graph-first 改版后神经元一多，网络画布节点"一堆挤在一起"，既重叠又成一团，看不出簇结构。用户已选定 **B 路线（引擎升级）**：尺寸感知碰撞 + 弹簧只作用于高权重边，根治全连接塌缩。
- 验收结果：默认 60+ 节点视图无重叠、图不再塌缩成团、高权重连接仍聚成清晰簇；布局保持确定性（重渲染坐标不变）；`pnpm run check` 0 error。

## Done Contract

- 什么算完成：
  1. `layoutForceNodes` 的弹簧力只作用于高权重边（低权重边仅渲染、不参与受力），消除全连接图整体吸向中心的塌缩效应。
  2. 斥力 / 碰撞基于节点真实尺寸（bbox），节点不再互相重叠。
  3. 布局参数修正：迭代、位移上限下限、初始半径；`NeuronNetworkGraph` 的 `fitView` 加 `padding` 与 `minZoom`。
  4. 保持确定性：固定 PRNG seed，同一子图每次计算结果一致。
- 由什么证明：`pnpm --filter agent-app check` 0 error；App 内手动观察 60+ 节点视图：无重叠、有簇感、刷新后坐标稳定。
- 哪些情况仍算未完成：簇聚合（C 路线，spec 原定 Out）；节点拖拽持久化；布局过渡动画；后端改动（无）。

## Scope

- In：`packages/agent-app/src/lib/features/neuron/networkLayout.ts`（`layoutForceNodes` 重写）、`packages/agent-app/src/lib/components/NeuronNetworkGraph.svelte`（`fitView` 参数）。
- Out：`layoutFlowNodes` 保留不动；`NeuronManager` 构图逻辑（TOP_N / depth）不动；后端 Rust 不动。

## Facts / Constraints

- **唯一调用点**：`layoutForceNodes` 仅被 `NeuronNetworkGraph.svelte:38` 的 `rebuild()` 调用，改动影响面小。
- **节点真实尺寸**（`NeuronFlowNode.svelte:31-41`）：`min-width: 140px`、`max-width: 200px`；label 单行截断，宽随文本变化；高约 56px（padding 8+10、两行文本 12px/10px）。
- **图近全连接**：spec `2026-08-01_18-00_neuron-directed-graph.md` 已确认神经元关系近似"笛卡尔积式全连接"，这是弹簧塌缩的根因。
- **当前参数问题**：`k = sqrt(area/n)` ≈ 40px（n≈80）远小于节点宽；`repulse=6000` 过弱；初始半径 260~380px 过小；300 次固定迭代 + 位移上限递减至 ~16px，收敛不充分。
- **无前端测试框架**（package.json 无 test 脚本），验证以 `pnpm run check` + 手动为准。
- 边权重 `weight` 已参与渲染（`stroke-width = 1 + norm*2.5`），弹簧降权只影响受力不影响渲染。

## 设计

### 1. 弹簧只作用于稀疏骨架边（治塌缩）

- 构建弹簧图 `springAdj` 时，只保留稀疏骨架：**每个节点仅保留与其相连权重最高的 K 条边（K=3）**，取其并集作为弹簧边集合。
- 理由（实现决策）：全连接图下"top 50%"仍是近全连接，每节点几十条弹簧边依旧整体吸向中心，无法防塌缩；每节点 top-3 使弹簧图退化为稀疏骨架（每节点弹簧度 ≤ 3），强关联对仍聚簇，弱/噪声边不产生拉力。
- 判定规则实现为纯函数 `selectSpringEdges(connections, k = 3): Connection[]`，与布局解耦、可单测（手工验证即可）。
- 低权重边照常渲染（粗细仍映射权重），只是不参与受力。
- 效果：高权重强关联聚簇，图不再被全连接弹簧吸向中心。

### 2. 尺寸感知斥力 + 碰撞分离（防重叠）

- 为每个节点估算 bbox：宽 `w = clamp(label.length * 8, 140, 200) + 12`（内边距余量），高 `h = 56`。
- 斥力改按有效距离计算：将节点近似为圆，半径 `r = hypot(w, h) / 4`；有效距离 `eff = dist - (rA + rB)`，`eff <= 1` 时 clamp 到 1，`f = repulse / (eff * eff)`。
- 每轮迭代末尾追加碰撞分离 pass（d3-force `collide` 风格）：若两节点 bbox 重叠（`|dx| < wa/2+wb/2 且 |dy| < ha/2+hb/2`），沿连线方向按重叠量各推一半。
- 理想距离下限抬升：`k = max(k, avgNodeW * 0.9)`。

### 3. 参数与渲染微调

- `iterations: 300 → 400`；位移上限 `limit = 120 * damping ** (it/30)`，下限 24px。
- 初始位置半径 `260 + rand*120 → 360 + rand*160`。
- `NeuronNetworkGraph.svelte`：`fitView` 改为 `fitView={{ padding: 0.3 }}`，`minZoom` 由库默认降到 0.1（让放大缩小的可用范围更大，避免视觉上永远"压扁"）。

### 接口保持

- `layoutForceNodes(subgraph, options?: { iterations?, seed? })` 签名不变，`rebuild()` 调用点零改动（仅内部实现变化）。

## Open Questions

- [x] 弹簧边阈值方案：已定为**稀疏骨架（每节点 top-3）**——top 50% 在全连接下仍近全连接，无法防塌缩。
- [ ] `fitView` padding 0.3 与 `minZoom` 0.1 是否合适：以手动验证观感为准，可微调。

## Restated Understanding

- 我理解当前任务是：在不改构图数据、不改渲染组件结构的前提下，重写 `layoutForceNodes` 的受力模型——弹簧只作用于高权重边以消除全连接塌缩，斥力与碰撞感知节点真实尺寸以消除重叠，并微调迭代/位移/初始半径与 `fitView` 参数；布局结果保持确定性。
- 当前核心目标是：默认视图（60+ 节点）不挤、有簇感、不抖动。
- 当前边界是：不做簇聚合、不持久化拖拽、不动后端、不动 `layoutFlowNodes`。
- 暂不处理：C 路线（TOP_N/depth 降密度与簇聚合）、拖拽持久化、布局动画。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：神经元网络图布局引擎升级（B 路线）。
- 当前核心目标：弹簧降权 + 尺寸感知碰撞 + 参数修正，使默认视图不拥挤且有簇感。
- 当前进度：实现完成，静态检查 + 算法冒烟通过。
- 下一步 1：用户 App 内观察默认视图观感，反馈后按需微调参数。
- 下一步 2：确认簇感与缩放手感是否达标。
- 验证方式：`pnpm run check`（0 error）+ 算法冒烟（无重叠/确定性）+ App 内观察。
- Execution Approval: 已批准（2026-08-05）。

## Change Log

- 2026-08-05: 初始 micro-spec。决策：B 路线（弹簧只作用于高权重边 + 尺寸感知碰撞），不做簇聚合（C 留后续）。
- 2026-08-05（实现决策）：弹簧边方案由"top 50%"调整为**稀疏骨架（每节点 top-3 边并集）**——全连接图下 top 50% 仍近全连接，无法防塌缩；K 可经 `selectSpringEdges(connections, k)` 参数微调。
- 2026-08-05（实现决策 2）：`@xyflow/svelte@1.6.2` 的 `fitView` 只接受 `boolean`，padding 需经独立 `fitViewOptions` prop 传入（`fitViewOptions={{ padding: 0.3 }}`）；`minZoom` 为顶层 prop。

## Validation

- Self-check：实现完成。`networkLayout.ts` 新增 `estimateNodeSize` / `selectSpringEdges`，重写 `layoutForceNodes`（稀疏骨架弹簧 + 尺寸感知有效距离斥力 + 碰撞分离 pass + 迭代 400/位移下限 24/初始半径 360~520 + `k = max(sqrt(area/n), avgW*0.9)`）；`NeuronNetworkGraph.svelte` 增加 `fitViewOptions={{ padding: 0.3 }}` 与 `minZoom={0.1}`。`layoutFlowNodes` 与后端未动。
- Static checks：`pnpm --filter agent-app check` 0 error（43 既有 warning，未新增）。
- Runtime / Test（算法冒烟，Node 直跑转译后代码）：80 节点 + 3160 条全连接边 → 弹簧骨架仅 120 条；400 次迭代 131ms；**重叠对 0 / 3160**（bbox 无相交）；两次调用结果完全一致（确定性 true）；布局范围约 2400×2000px。
- Human confirmation：micro-spec 已获用户批准后实现；App 内视觉验证待用户进行。
- 结果汇总：代码完成，静态检查 + 算法冒烟通过；运行时 UI 观感待用户确认。
- 核心目标是否已由证据证明完成：算法层面已证明（无重叠 + 确定性 + 骨架稀疏防塌缩）；视觉手感需人工确认。
- 若未完成，当前剩余差距：无代码差距；仅剩 App 内观察默认视图观感（簇感、缩放手感）。
- 剩余风险：碰撞 pass 对极多节点（>150）为 O(n²)（与既有斥力同量级，可接受）；`estimateNodeSize` 的 label 宽度估算与真实渲染可能有偏差（`+12` 余量缓冲 + 碰撞 pass 兜底）。

## Resume / Handoff

- 当前状态：实现完成，静态检查 + 算法冒烟通过，待 App 内视觉验证。
- 当前卡点：无。
- 下一步唯一动作：用户 App 内打开神经元网络面板，观察默认视图（60+ 节点）是否无重叠、有簇感、缩放正常；如观感不理想，按反馈微调 `selectSpringEdges` 的 K、`k` 下限系数（0.9）或 `fitViewOptions.padding`。
- 下一轮核心目标：默认视图不挤、有簇感、确定性保持。
