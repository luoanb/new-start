# Technical Plan / 技术方案: 神经元面板 — 移除 tag 筛选 + 创建/权重

- 关联需求：`requirements.md`（同迭代目录）
- 生成日期：2026-08-02
- 约束：本迭代不涉及视觉稿，无 `visual-design.md`；技术方案需在 `lifecycle.md` 进入 `planned` 后产出。

## Goal / 目标

按需求实现三件事：移除顶部 tag 筛选；新增「创建神经元（孤立/下游）」；新增「调整权重（自身/边）」。后端复用既有 `neuron_manager` 能力，仅新增 1 个公开方法 + 暴露 3 个 Tauri 命令；前端改 2 个组件 + 补类型。

## Current State / 当前实现状态

**已有、未暴露的命令缺口**：
- `neuron_manager.adjust_weight(&self, id, delta)` → `store.adjust_weight`（neuron_manager.rs:133，已 pub）
- `neuron_manager.adjust_edge_weight(&self, source, target, delta)` → `store.adjust_connection_weight`（neuron_manager.rs:137，已 pub）
- `neuron_manager.persist_plain(create, link_to)`（neuron_manager.rs:817，**私有**）：`link_to=None` → `store.create_neuron(weight=0)`；`Some(src)` → `store.create_downstream_neuron(src, create, 0.0)` 自动建边权重 0。正是「孤立/下游」所需的 store 直持久化路径，且不触发 LLM。

**前端现状**：
- tag 筛选在 `NeuronManager.svelte`：`.toolbar` 的 `allTypes`/`selectedTypes`/`filteredNeurons`（system_type chips）。
- 权重只读展示在 `NeuronDetailDrawer.svelte`（`权重` 字段 + `关联` 列表），无编辑控件。
- 无创建入口。

**命令注册**：`lib.rs:485-526` 的 `generate_handler!`，Neuron 段位于 `lib.rs:514-519`，当前仅有 `list_neurons/get_neuron/update_neuron/get_connections/get_network`。

## Plan / 计划

### 1. 后端：新增公开创建方法

文件：`packages/agent-app/src-tauri/src/core/neuron_manager.rs`

在 `NeuronManager` impl 内（紧邻 `persist_plain`，neuron_manager.rs:817 附近）新增 `pub` 方法，包裹私有 `persist_plain`：

```rust
/// 前端手动创建：store 直持久化，不触发 LLM 草稿生成。
/// link_to = None => 孤立神经元；Some(id) => 该神经元的下游神经元（自动建边，边权重 0）。
pub fn create_plain(&self, create: NeuronCreate, link_to: Option<&str>) -> AppResult<Neuron> {
    self.persist_plain(create, link_to)
}
```

> 复用 `persist_plain` 已保证 `weight=0` 与下游边权重 `0`，满足 `neuron-create-weight-zero` 约束。

### 2. 后端：暴露 3 个 Tauri 命令

文件：`packages/agent-app/src-tauri/src/lib.rs`

在 Neuron 命令段（lib.rs:514 `// Neuron` 之后）新增：

```rust
#[tauri::command]
async fn create_neuron_plain(
    mgr: State<'_, Arc<NeuronManager>>,
    desc: String,
    content: Option<String>,
    link_to: Option<String>,
) -> TauriResult<Neuron> {
    let create = NeuronCreate {
        desc,
        content: content.unwrap_or_default(),
        weight: 0.0,
        system_type: None,
        tool_ids: vec![],
    };
    mgr.inner()
        .create_plain(create, link_to.as_deref())
        .map_err(|error| error.payload())
}

#[tauri::command]
async fn adjust_neuron_weight(
    mgr: State<'_, Arc<NeuronManager>>,
    id: String,
    delta: f64,
) -> TauriResult<Neuron> {
    mgr.inner().adjust_weight(&id, delta).map_err(|error| error.payload())
}

#[tauri::command]
async fn adjust_edge_weight(
    mgr: State<'_, Arc<NeuronManager>>,
    source: String,
    target: String,
    delta: f64,
) -> TauriResult<Connection> {
    mgr.inner()
        .adjust_edge_weight(&source, &target, delta)
        .map_err(|error| error.payload())
}
```

### 3. 后端：注册命令

文件：`packages/agent-app/src-tauri/src/lib.rs`，`generate_handler!`（lib.rs:485-526）的 `// Neuron` 段（lib.rs:514-519）追加：

```rust
create_neuron_plain,
adjust_neuron_weight,
adjust_edge_weight,
```

### 4. 前端：类型补充

文件：`packages/agent-app/src/lib/types.ts`

新增（或内联 `invoke` 参数，建议显式类型）：

```ts
export type CreateNeuronPlainInput = {
  desc: string;
  content?: string;
  link_to?: string | null;
};
```

> `Neuron` / `Connection` 已存在，无需改动。

### 5. 前端：移除 tag 筛选

文件：`packages/agent-app/src/lib/components/NeuronManager.svelte`

- 删除 `allTypes`（基于 `neuron.system_type` 的去重集合）与 `selectedTypes` 状态。
- 删除 `.toolbar` 内渲染 `system_type` chips 的 `{#each allTypes}` 区块及其 `on:click` 处理。
- `filteredNeurons` 移除对 `system_type` 的过滤分支，仅保留其余有效筛选项（按需求 Q1 确认：若 depth/edge-type 等已无意义则一并废弃；当前探查到工具栏 tag 即 system_type，其余筛选项实现时确认）。
- 保留向 `NeuronIndex` 传递 `neurons` 的逻辑不变。

### 6. 前端：新增「创建神经元」弹窗

文件：`packages/agent-app/src/lib/components/NeuronManager.svelte`

- 工具栏新增按钮 `＋ 创建神经元`，点击置 `showCreate = true`。
- 新增内联弹窗（或独立 `NeuronCreateDialog.svelte`）：
  - 模式单选：`孤立`（默认） / `下游`。
  - `下游` 模式：下拉选择上游神经元（来源当前 `neurons` 列表，显示 `desc` 或 `id`）。
  - 文本输入：`desc`（必填）、`content`（可选多行）。
  - `创建` → `invoke("create_neuron_plain", { desc, content, link_to })`；`取消` → 关闭。
- 成功回调：
  - 重新 `loadNeurons()`（复用既有列表刷新）。
  - `showCreate = false`；`selectedId = 新id`；`onSelect(新id)` 打开抽屉。
- 失败：`console.error(String(e))` + 内联错误文案（来自 `AppError.payload()`）。

### 7. 前端：新增权重调整控件

文件：`packages/agent-app/src/lib/components/NeuronDetailDrawer.svelte`

- 「权重」字段旁加步进控件：`− [weight] ＋`，步长 `0.05`。
  - `invoke("adjust_neuron_weight", { id: neuron.id, delta: ±0.05 })`，用返回值覆盖 `neuron.weight`。
- 「关联（connections）」每条边旁加步进控件：`− [w] ＋`，步长 `0.05`。
  - `invoke("adjust_edge_weight", { source, target, delta: ±0.05 })`，刷新 `connections[i].weight`。
- 加 `saving` 锁防止并发重复点击；失败给出可见提示。
- 调整后通知父组件刷新列表权重徽标（触发一次 `list_neurons` 或回调，可选）。

### 8. 调用一致性

- 复用既有 `import { invoke } from "@tauri-apps/api/core";` 与 `$lib/types` 类型。
- 错误提示沿用 `console.error(String(e))`，并补充用户可感知的提示（最小实现用内联文字，不引入新依赖）。

## Impact / 影响

- 后端：新增 1 个 pub 方法（`create_plain`）+ 3 个命令；不改变既有 `create_neuron`（LLM 流程）、`update_neuron` 等调用方。
- 前端：2 个组件改动（NeuronManager、NeuronDetailDrawer）；types.ts 新增 1 类型。
- 运行态：不影响 assistant 对权重的消费逻辑；仅人工可读写。

## Risks / 风险

- `persist_plain` 未被并发保护：其内部 `store` 已用 `Mutex`，创建与权重调整均在 `store` 锁内完成，无新增竞态。
- 下游模式上游 id 不存在：`store.create_downstream_neuron` 应返回错误，命令经 `error.payload()` 回传前端提示（AC-6）。实现时需确认该函数对缺失 source 的报错路径。
- 命令命名：`create_neuron_plain` 区别于 LLM 流程 `create_neuron`，避免误调用。
- 权重非负限制：方案不做非负限制（与后端数值累加一致），若产品要求非负需另行约束（不在本需求 AC）。

## Verification / 验证

- 后端编译：`cargo check -p agent-app`（或 `pnpm tauri dev` 触发）。
- 前端联调（AC 映射）：
  - AC-1 ← §5（去 system_type chips，保留分组）
  - AC-2 ← §6 孤立模式（weight=0、无连接）
  - AC-3 ← §6 下游模式（边由 `persist_plain` 建权重 0）
  - AC-4 ← §7 神经元权重步进 + `adjust_neuron_weight`
  - AC-5 ← §7 边权重步进 + `adjust_edge_weight`
  - AC-6 ← §6/§7 错误分支（上游不存在、调整失败提示）
- 回归：既有 `list_neurons` / `get_connections` / `get_network` 调用不受影响。

## Open Questions Resolved / 开放问题处置

- Q1（其余筛选项）：按最小改动，仅移除 `system_type` tag 筛选；若工具栏还存在 depth/edge-type 类筛选且已无意义，实现时一并精简并记入 commit。
- Q2（下游上游范围）：不限，沿用现有 `neurons` 列表即可。
