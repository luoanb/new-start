# Exec Scheme Bridge: agent-app-neuron-mgmt

## 1. 改动依赖范围内的能力与代码现实

**范围**：仅本次实现会调用或必须修改/扩展的仓库能力。

| 能力 | 现状 | 证据 |
|------|------|------|
| NeuronManager 公共方法 | 够用 — 已有 `list_neurons` / `get_neuron` / `update_for_admin` / `get_connections` / `get_network` | [neuron_manager.rs](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src-tauri/src/core/neuron_manager.rs) L60-L120 |
| Gateway 暴露 NeuronManager | 够用 — `neuron_manager()` 返回 `Arc<NeuronManager>` | [gateway.rs](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src-tauri/src/core/gateway.rs) L393-L395 |
| Tauri `with_gateway` 辅助模式 | 够用，可直接复用 | [lib.rs](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src-tauri/src/lib.rs) L284-L290 |
| Svelte 主面板切换机制 | 需扩 — `+page.svelte` 目前固定渲染 ChatArea，需增加 NeuronView 切换逻辑 | [page.svelte](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/routes/+page.svelte) |
| StatusBar 按钮扩展 | 需扩 — 需新增神经元按钮入口 | StatusBar 组件 |
| 前端 Neuron/Connection 类型 | 缺 — `types.ts` 无对应类型 | [types.ts](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/lib/types.ts) |
| i18n 翻译键 | 缺 — 无 neuronPanel 相关翻译 | [translations.ts](file:///home/lab/Documents/trae_projects/new-start/packages/agent-app/src/lib/i18n/translations.ts) |

## 2. 外部依赖：包与本任务用到的精确 API

| 包（版本） | 本任务依赖的具体 API | 备注 |
|------------|----------------------|------|
| `@tauri-apps/api` (lockfile) | `invoke<T>(cmd, args)` | 已有，无需新增 |
| 无新增外部依赖 | — | 图网络用缩进树形列表，不引入图可视化库 |

## 3. 设计契约

**技术文档出处**：`docs/sdd-lab/2026-07-30_11-00_agent-app-neuron-mgmt/requirements.md`

**契约正文**：

### Rust 后端 — 5 个 Tauri 命令

所有命令通过 `with_gateway` 获取 Gateway 后调用 `neuron_manager()` 方法。无需新增 `with_neuron_store` 辅助函数（NeuronManager 内部处理锁）。

```rust
// 命令签名草案
#[tauri::command]
fn list_neurons(state: State<'_, Mutex<Gateway>>) -> TauriResult<Vec<Neuron>>;

#[tauri::command]
fn get_neuron(state: State<'_, Mutex<Gateway>>, id: String) -> TauriResult<Neuron>;

#[tauri::command]
fn update_neuron(
    state: State<'_, Mutex<Gateway>>,
    id: String,
    desc: Option<String>,
    content: Option<String>,
) -> TauriResult<Neuron>;

#[tauri::command]
fn get_connections(
    state: State<'_, Mutex<Gateway>>,
    id: String,
) -> TauriResult<Vec<Connection>>;

#[tauri::command]
fn get_network(
    state: State<'_, Mutex<Gateway>>,
    id: String,
    max_depth: Option<usize>,
) -> TauriResult<Vec<Neuron>>;
```

实现细节：
- `list_neurons` → `gateway.neuron_manager().list_neurons()`
- `get_neuron` → `gateway.neuron_manager().get_neuron(&id)`
- `update_neuron` → `gateway.neuron_manager().update_for_admin(&id, NeuronUpdate { desc, content })`
- `get_connections` → `gateway.neuron_manager().get_connections(&id)`
- `get_network` → `gateway.neuron_manager().get_network(&id, max_depth.unwrap_or(2))`

### 前端组件结构

```
+page.svelte
├── StatusBar (新增🧠按钮)
├── SessionList
├── {#if showNeuronView}
│   └── NeuronManager        ← 新组件，替代 ChatArea
│       ├── NeuronList       子组件 — 列表视图
│       ├── NeuronDetail     子组件 — 详情（含编辑、连接列表）
│       └── NeuronNetwork    子组件 — 树形网络视图
├── {:else}
│   └── ChatArea             (现有)
├── SidePanel
└── ErrorBanner
```

视图状态机（在 NeuronManager 内管理）：

```
state: "list" | "detail" | "network"
list  → 点击某条 → detail
detail → 点击「查看网络」→ network
detail → 点击其他神经元（连接或网络中）→ 切换到该 neuron 的 detail
network → 点击节点 → 切换到该 neuron 的 detail
list/detail/network → 点击返回 → list
```

### 前端类型定义（types.ts 新增）

```typescript
export type Neuron = {
  id: string;
  desc: string;
  content: string;
  weight: number;
  system_type?: string | null;
  tool_ids: string[];
  created_at: number;
  updated_at: number;
};

export type Connection = {
  source: string;
  target: string;
  weight: number;
};
```

### 数据流

```
启动 → invoke("list_neurons") → NeuronList 展示
选择单位 → invoke("get_neuron", { id }) → NeuronDetail 展示详情
编辑保存 → invoke("update_neuron", { id, desc?, content? }) → 刷新详情
连接列表 → invoke("get_connections", { id }) → 详情内连接区展示
网络视图 → invoke("get_network", { id, max_depth: 2 }) → NeuronNetwork 树形展示
```

所有调用通过 `withLoading` / `try/catch` 处理 loading/error 状态。

### 相对技术文档的增量说明

| 项目 | 说明 |
|------|------|
| 沿用 | 需求文档中定义的 5 个后端命令、Neuron 列表/详情/编辑/连接/网络视图的交互路径 |
| 改写 — 视图切换 | 原文「侧栏标签页」改为「主面板替换（StatusBar 按钮切换）」—— 用户确认的入口方案 |
| 改写 — Neuron 权重排序 | 默认按权重降序，前端排序而非后端（`list_neurons` 后端按 `created_at DESC`） |
| 补充 — `max_depth` 默认值 | 契约中固定为 `2`，以解决需求文档 Q1 |
| 改写 — 无 NeuronCreate/Delete 命令 | 与原文一致：创建/删除由 AI tools 管理，GUI 不提供 |
