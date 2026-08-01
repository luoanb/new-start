# 神经元网络图：有向图语义 + 连接去重方案

- 日期：2026-08-01
- 关联：`docs/specs/2026-08-01_14-30_neuron-graph-first.md`（Graph-first 改版）
- 状态：待确认

## 背景与问题

Graph-first 改版后，用户反馈两点核心认知偏差：

1. **不是树**：神经元之间的关系是"任意节点对之间的有向边"（近似笛卡尔积式全连接），
   不是父→子、无环、无跨层的树。当前 `networkLayout.layoutFlowNodes`
   使用**按 depth 分层的分层布局**（layered layout），视觉上呈现层级列，易让人误以为是树图。
2. **连接重复**：渲染期出现 `each_key_duplicate`（key = `source->target` 重复）。
   根因是后端 `get_connections` 聚合时存在**完全相同（同 source、同 target）的重复连接**。

## 设计原则（用户确认）

- `A→B` 与 `B→A` 是**两条不同的合法有向边**，必须共存。
- `A→B` 与 `A→B`（source、target 完全相同）是**非法的重复连接**，不允许。
- 去重粒度：**方向敏感**，按 `(source, target)` 组合判定，不按无向对 `{A,B}` 判定。

## 改动 1：布局改为力导向（force-directed）

目标：视觉上明确表达"非树"——节点自由散开、交叉边自然呈现，消除分层列带来的层级错觉。

### 1.1 新增力导向布局函数

文件：`packages/agent-app/src/lib/features/neuron/networkLayout.ts`

- 新增 `layoutForceNodes(subgraph, options?)`：
  - 纯前端简单力导向（repulsion + spring + centering），迭代固定次数（如 300 步）后输出坐标。
  - 无需引入 d3-force 等重依赖（保持当前零新增依赖）；若迭代实现成本过高，可接受引入
    `d3-force`（需更新 package.json + pnpm-lock）。**默认方案：手写轻量力导向**，不新增依赖。
  - 输入：`NeuronSubgraph`；输出：`LayoutNode[]`（与现有 `LayoutNode` 类型一致）。
- 保留 `layoutFlowNodes` 不删除（供未来可选"分层视图"切换使用），但 Graph 默认改用 `layoutForceNodes`。

### 1.2 Graph 组件切换默认布局

文件：`packages/agent-app/src/lib/components/NeuronNetworkGraph.svelte`

- `rebuild()` 内 `layoutFlowNodes(sg)` 改为 `layoutForceNodes(sg)`。
- 因为力导向坐标在 data 变化时稳定（确定性迭代 + 固定随机种子），不会每次重排导致抖动。

### 1.3 可选：布局切换控件

在 `NeuronManager` toolbar 增加"力导向 / 分层"切换（默认力导向）。
- 若评估成本可控则做；否则仅切默认布局，跳过切换控件。

## 改动 2：(source, target) 方向敏感去重

目标：消除重复连接，修复 `each_key_duplicate`，并消除 edge id 冲突隐患。

### 2.1 前端聚合去重（立即止血）

文件：`packages/agent-app/src/lib/components/NeuronManager.svelte`

在 `load()` 聚合 `allConnections` 后，按 `(source, target)` 去重：

```ts
const seen = new Set<string>();
const deduped: Connection[] = [];
for (const c of conns) {
  const k = c.source + "->" + c.target;
  if (seen.has(k)) continue;
  seen.add(k);
  deduped.push(c);
}
allConnections = deduped;
```

语义：保留 `A→B` 与 `B→A` 共存；仅丢弃完全同向重复。

### 2.2 去重贯穿所有消费方

- `buildSubgraph()` / `pruneByDepth()` 直接消费 `allConnections`，去重后自然受益。
- `openDrawer()` 中 `drawerConns = allConnections.filter(...)` 同样受益。
- `NeuronNetworkGraph` 的 `edge.id = source->target` 在去重后保证唯一。
- `NeuronDetailDrawer` 的 `#each connections as c (c.source + "->" + c.target)` 去重后唯一。

### 2.3 后端唯一约束（治本，建议同步提给后端）

- 连接表建议加唯一约束 `UNIQUE(source, target)`（若业务允许同方向仅一条）。
- 若业务需保留"同方向多条但属性不同"，则唯一键细化为 `(source, target, type)` 之类。
- 本 spec 前端部分不改动后端；后端约束作为建议项单独提 issue/PR。

## 不做的事

- 不引入树图布局（d3-hierarchy / dagre rankdir），与"非树"诉求相反。
- 不把去重改成无向对 `{A,B}` 去重（会错误合并 `A→B` 与 `B→A`）。
- 不删除 `layoutFlowNodes`（保留供可选切换）。

## 受影响文件清单

| 文件 | 改动 |
|------|------|
| `packages/agent-app/src/lib/features/neuron/networkLayout.ts` | 新增 `layoutForceNodes` |
| `packages/agent-app/src/lib/components/NeuronNetworkGraph.svelte` | `rebuild` 改用 `layoutForceNodes` |
| `packages/agent-app/src/lib/components/NeuronManager.svelte` | `load()` 聚合时 `(source,target)` 去重 |
| `packages/agent-app/src/lib/components/NeuronManager.svelte`（可选） | toolbar 加布局切换控件 |

## 验证

- `pnpm -C packages/agent-app check`（svelte-check）0 error / 0 warning。
- 渲染含 `A→B` 与 `B→A` 的样例数据：两条边均显示，无 `each_key_duplicate`。
- 渲染含重复 `A→B` 两次的数据：仅显示一条，无 key 冲突。
- 视觉：节点呈力导向散开，无分层列层级错觉。
