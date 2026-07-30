# Technical Plan / 技术方案: neuron-graph-viz

## Restated Understanding / 需求复述

- 演进 `get_network` 返回 `NeuronSubgraph`；GUI 网络视图默认用 Svelte Flow 渲染局部有向图，且 `max_depth` 可在 1–5 调节并重新拉取。
- 核心目标：子图契约 + 默认可控深度的 Flow 图视图（可切回树）。
- 不引入全图无 depth 模式；不持久化坐标；不改写只读管理边界。

## Current Code Reality / 代码现实

| 能力 | 现状 | 证据 |
|------|------|------|
| `NeuronStore::get_network` | BFS 返回 `Vec<Neuron>`，遍历时读了边但丢弃 | `neuron_store.rs` |
| Tauri `get_network` | 默认 depth=2，返回 `Vec<Neuron>` | `lib.rs` |
| AI `GetNetworkTool` | 默认 depth=3，JSON=`Vec<Neuron>` | `neuron_manager.rs` |
| GUI `NeuronNetwork.svelte` | 节点数组 + 假缩进 | 组件现状 |
| 前端依赖 | 无 `@xyflow/svelte` | `package.json` |

## Done Contract / 完成契约

- 完成证明：
  1. `get_network` → `NeuronSubgraph`；单测覆盖节点与边裁剪
  2. 网络视图默认 Flow 图；depth 1–5 可调并重新拉取
  3. 可切树；点击节点跳转详情
  4. `cargo test` 相关通过；`svelte-check` 无新增错误
- 未完成：全图模式、持久化布局、图上改边/改权

## Solution Options / 方案候选

### Option A / 演进 API + SVG 自绘

- 推荐：否（已被用户否决渲染部分）

### Option B / 新增 `get_subgraph` 保留旧 API

- 推荐：否（用户确认演进）

### Option C / 演进 API + Svelte Flow + depth 可控（选定）

- 推荐：是
- 方案摘要：破坏性演进 `get_network`；GUI 引入 `@xyflow/svelte`；网络工具条提供 depth 控件与树/图切换。
- 优点：子图一次到位；Flow 自带缩放/平移/适配 depth 变大；交互符合产品预期
- 缺点：新依赖与主题对接；AI tool JSON 形态变化
- 风险：自定义节点样式与现有 CSS 变量对齐

## Decision / 方案决策

- Selected / 选定方案：**Option C**（演进 `get_network` + Svelte Flow + depth 可控；默认图）
- Why / 选择原因：用户 2026-07-30 23:06 确认 Q1–Q3
- Decision Owner / 决策人：user
- Decision Time / 决策时间：2026-07-30 23:06
- Open Questions 状态：全部关闭

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：破坏性变更（返回值形态）
- 消费方：Tauri GUI、TUI、AI `get_network` tool、Rust 单测
- 真相源：`models.rs` + `types.ts`

### `NeuronSubgraph`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuronSubgraph {
    pub seed_id: String,
    pub neurons: Vec<Neuron>,
    pub connections: Vec<Connection>,
}
```

```ts
export type NeuronSubgraph = {
  seed_id: string;
  neurons: Neuron[];
  connections: Connection[];
};
```

语义：

- BFS 从 `seed_id` 出发，`max_depth` 达到则不再扩展（与现逻辑一致）
- 邻接扩展仍用出入边（无向邻域）
- `connections`：两端均 ∈ 返回神经元 ID 集合
- 不返回布局坐标

### 命令与默认值

| 入口 | 签名 | 默认 depth |
|------|------|------------|
| Tauri `get_network` | `(id, max_depth?: number) -> NeuronSubgraph` | `2` |
| AI `GetNetworkTool` | 同前，返回子图 JSON | `3`（保持工具现状） |
| GUI 控件 | 用户选 1–5，显式传入 `max_depth` | 初始 `2`；同一次网络浏览会话内保留用户所选 depth |

### Compatibility Notes / 兼容说明

- 更新 `GetNetworkTool` description：返回 `{ seed_id, neurons, connections }`
- TUI：展示节点数、边数，并可列出边摘要
- 明确不做：全图 API、服务端布局、边分页

## Frontend Design / 前端设计要点

### 依赖

```sh
# packages/agent-app
npm install @xyflow/svelte
```

- 引入 `@xyflow/svelte/dist/style.css`（可在图组件内 import）
- 主题：节点/边颜色优先映射现有 CSS 变量（`--color-primary`、`--color-surface`、`--color-text` 等）

### 组件结构

```
NeuronNetwork.svelte
├── 工具条：返回 | 树/图切换 | depth 选择（1–5）
├── NeuronNetworkTree.svelte   # 消费 subgraph.neurons（可附带边摘要）
└── NeuronNetworkGraph.svelte  # Svelte Flow：nodes/edges 由 subgraph 映射
```

### NeuronNetworkGraph

- `nodes`：由 `Neuron` 映射；`id=neuron.id`；自定义 node 展示 desc / weight / system_type 标记；seed 节点高亮
- `edges`：由 `Connection` 映射；`id=source->target`；`label` 或 data 含 weight；`markerEnd` 箭头；可选按权映射 strokeWidth
- 布局：前端按无向最短路分层（seed=0），同层纵/横向排布后写入 `position`；depth 或数据变化时重算并 `fitView`
- 交互：`on:nodeclick` → `onJumpTo(id)`；`nodesDraggable=true`（临时）；`edgesUpdatable=false`；禁用连线创建
- Controls + Background；Minimap 可选（默认关，避免噪声）

### Depth 控件

- `<select>` 或分段按钮：`1 | 2 | 3 | 4 | 5`
- 变更 → `loading` → `invoke("get_network", { id: rootId, max_depth })` → 重建 nodes/edges
- 会话内保留 depth；离开网络视图再进入：保留会话值（组件未销毁则自然保留；若销毁则回默认 2）

### i18n

- 树/图切换、depth 标签、图空态、加载中等中英键

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 用户明确批准执行
- 需求/方案已按 Q1–Q3 回写（本文件）

### Step 1. 后端子图契约

- `models.rs`：新增 `NeuronSubgraph`
- `neuron_store.rs`：`get_network` 返回子图；BFS 收集并裁剪边；更新单测
- `neuron_manager.rs`：返回类型 + tool 描述/序列化
- `lib.rs` / `tui/app.rs`：适配

### Step 2. 前端依赖与类型

- `package.json`：添加 `@xyflow/svelte`
- `types.ts`：`NeuronSubgraph`
- `translations.ts`：新文案

### Step 3. 网络视图

- 拆分/改写 `NeuronNetwork*.svelte`
- 默认 `mode=graph`；depth 控件；Flow 图 + 树切换；点击跳转

### Step 4. 检查与回写

- `cargo test`（neuron 相关）
- agent-app `npm run check`
- 回写 `lifecycle.md`：验证结果、状态

## Risk And Mitigation / 风险与缓解

| 风险 | 缓解 |
|------|------|
| AI tool JSON 破坏 | 同步改 description；调用方在同仓 |
| depth=5 节点过多 | Flow 缩放 + fitView；后续再加 Top-N |
| Flow 默认样式违和 | 自定义 node/edge + CSS 变量 |
| Svelte 5 runes 与 Flow 绑定 | 按官方 `$state.raw` / bind:nodes 模式 |
| 成环 | Store visited + 前端分层 visited |

## Display Notes / 展示约定（已确认）

- 主列表：保持 desc / weight / 短 id / system_type / created_at
- 网络树：D{n}、→/←、边权、desc、system_type、短 id；seed 高亮
- Flow 节点：desc + weight + system_type 角标；seed 高亮

## Execute Checkpoint / 执行检查点

- 状态：已执行完成（2026-07-30 23:18）
- 验证：`test_network_bfs` ok；`svelte-check` 0 errors；`cargo check --bins` ok
