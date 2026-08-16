# Spec: 神经元管理封装为系统工具（System 标签）

## Goal

- 要解决什么问题：`ToolTag::System` 标签落地至今没有实际工具供给——System 模式会话（`tool_tags = [Core, System]`）自动注入的 System 标签工具集为空。`neuron/tools.rs` 里已备好 6 个 neuron 管理 tool adapter（查询 / 更新 / 创建 / 选型），长期保留未注册。
- 本次目标：把神经元管理封装成 `ToolTag::System` 工具集注册上架，系统模式会话自动带上，作为**新增入口**。
- 边界：以前 AI 创建流程（Agent / Assistant 模式的 insert 驱动：`neuron.draft_from_model` / `neuron.select_one` / `creator.variant_evolve`）**完全不变**；`create_neuron` 等 System 工具与既有流程共用同一底层（`NeuronManager::create_neuron` 统一创建流程），只是由系统模式显式调用触发。
- 验收结果：System 会话可调用 neuron 管理工具；Agent / Assistant / Chat 行为不变；`cargo test` 全绿。

## Done Contract

1. `neuron/tools.rs` 6 个 adapter（`get_neuron` / `list_neurons` / `update_neuron` / `get_network` / `create_neuron` / `select_neuron_candidates`）去除 `dead_code` 标记，通过新的 `pub fn register_system_tools(registry: &mut ToolRegistry, manager: Arc<NeuronManager>)` 以 `ToolTag::System + ToolSource::Native` 注册。
2. **`list_neurons` 改为受限分页查询**：不得返回全部神经元（防卡顿 / 防上下文撑爆）。参数 `page`（默认 0）/ `page_size`（默认 20，store 层 `clamp(1,100)` 硬顶）/ `search`（可选，按 desc / id 模糊）/ `kind`（可选：`all` / `system` / `normal`，复用 `NeuronKindFilter::parse`），底层改调 `NeuronManager::list_neurons_page`，返回 `NeuronPage { items, total, has_more }`。原 `list_neurons()` 全量实现不再暴露给模型。
3. 补齐 6 个自描述手册 `inserts/<name>.md`（Native 门禁要求；含 `## 工具` 段且首行非空以满足 `catalog_carries_hints` 测试）。
4. 三处装配路径统一注册（`register_system_tools` 为唯一注册点，避免 MCP 整体替换丢工具）：
   - `Gateway::build` 主路径：`neuron_manager` 创建后写入共享 `tool_registry`；
   - 启动期后台 MCP 装配：`base_registry` 构建后注册（`neuron_manager` 创建提前到 spawn 之前）；
   - `assemble_and_replace`（`save_tool_config` / `reassemble_tools` 运行期重装配）：`base_registry` 构建后注册。
5. 测试注入路径（`test_tool_registry`）不注册 neuron 工具（行为不变）。
6. 由什么证明：`cargo test` 全绿；启动后 `list_tool_info` 返回 8 个本地工具（2 Core + 6 System），System 会话 wire 含 System 标签工具；Agent / Assistant 既有工具 wire 不变。

## Scope

- In：`neuron/tools.rs`（去 dead_code + 注册函数）、`inserts/*.md` × 6、`gateway.rs`（装配顺序与三处注册）、相关测试。
- Out：不改 `NeuronManager` / 创建 / 选型业务逻辑；不改前端（ToolPanel 徽标、System 会话创建入口均已存在）；不改 Agent / Assistant insert 流程；不改 `ConversationMode::tool_tags` 映射。

## Facts / Constraints

- System 模式会话已存在：前端 `SessionCreateModal` 有 system 选项（[SessionCreateModal.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/SessionCreateModal.svelte#L14)）；`gateway.rs` 路由 `System → assistant.converse`（[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L595-L605)）；`ConversationMode::tool_tags()`：`System → [Core, System]`（[hoist-tool-tag-mapping](file:///home/lab/Documents/trae_projects/new-start-wt/docs/micro_specs/2026-08-16_hoist-tool-tag-mapping.md#L42-L47)）。
- `register_tagged(Native)` 走 insert 门禁：缺失 `inserts/<name>.md` 时 `InsertCatalog::require` panic（[tool_registry.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/tool_registry.rs#L119-L131)）。
- 工具装配三路径都经 `assemble_local_tools` / `assemble_mcp_progressive` 重建并整体替换共享 registry（[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L1020-L1024)、[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L487-L504)），neuron 工具必须并入 base 才不会被替换丢失。
- `NeuronManager::new` 依赖 `Arc<RwLock<ToolRegistry>>`（[manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L85-L90)），且其依赖（`neuron_store` / `providers` / `NeuronConfigReader` / `tool_registry`）在 `Gateway::build` 的 MCP spawn 之前均已就绪，创建可安全提前。
- adapter 已调用全部存在的 manager API：`list_neurons` / `get_neuron` / `get_connections` / `get_network` / `update_content_for_ai` / `create_neuron` / `select_candidates`（[manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron/manager.rs#L134-L309)）。

## 接口契约设计

### 1. 注册函数（neuron/tools.rs）

```rust
/// 注册神经元管理 System 工具（系统模式会话自动带上；AI 创建流程不变）。
pub fn register_system_tools(registry: &mut ToolRegistry, manager: Arc<NeuronManager>) {
    registry.register_tagged(ToolTag::System, CreateNeuronTool::new(Arc::clone(&manager)), ToolSource::Native);
    registry.register_tagged(ToolTag::System, ListNeuronsTool::new(Arc::clone(&manager)), ToolSource::Native);
    registry.register_tagged(ToolTag::System, GetNeuronTool::new(Arc::clone(&manager)), ToolSource::Native);
    registry.register_tagged(ToolTag::System, GetNetworkTool::new(Arc::clone(&manager)), ToolSource::Native);
    registry.register_tagged(ToolTag::System, UpdateNeuronTool::new(Arc::clone(&manager)), ToolSource::Native);
    registry.register_tagged(ToolTag::System, SelectNeuronCandidatesTool::new(Arc::clone(&manager)), ToolSource::Native);
}
```

### 2. 工具清单与 insert 门禁

| 工具 | 能力 | manager 底层 | insert |
|---|---|---|---|
| `list_neurons` | **受限分页查询**：`page` / `page_size`(≤100) / `search` / `kind`，返回 `NeuronPage{items,total,has_more}`，不返回全量 | `list_neurons_page` | 新增 |
| `get_neuron` | 单神经元详情 + 连接 | `get_neuron` / `get_connections` | 新增 |
| `get_network` | BFS 网络子图（`max_depth` 可控） | `get_network` | 新增 |
| `create_neuron` | 统一创建流程创建 1..=10 个普通神经元 | `create_neuron` | 新增 |
| `update_neuron` | 更新普通神经元描述 / 内容 | `update_content_for_ai` | 新增 |
| `select_neuron_candidates` | 选型候选（高权重 + 补缺），`n` 由参数限定 | `select_candidates` | 新增 |

### 2.1 `list_neurons` 接口形态（防上下文撑爆）

```rust
// parameters
{
  "type": "object",
  "properties": {
    "page":       {"type": "integer", "minimum": 0, "default": 0},
    "page_size":  {"type": "integer", "minimum": 1, "maximum": 100, "default": 20},
    "search":     {"type": "string", "description": "按 desc / id 模糊搜索，可选"},
    "kind":       {"type": "string", "enum": ["all", "system", "normal"], "default": "all"}
  }
}
// execute → manager.list_neurons_page(page, page_size, search, NeuronKindFilter::parse(kind))
// 返回 NeuronPage { items, total, has_more }（Serialze 已具备）
```

- 返回大小上限由两层保障：schema `page_size ≤ 100` + store 层 `clamp(1, 100)`。
- `search` 用于缩小范围；`has_more` 提示模型翻页，避免一次拉全量。

### 3. 装配顺序调整（gateway.rs）

`neuron_manager` 创建提前至 MCP 后台装配 spawn（L159）之前；三处装配各自在 base/共享 registry 上调用 `register_system_tools`。

## Validation

- `cargo test` 全绿（含 insert 门禁 / hint / registry tag 用例）。
- 冒烟（可选）：启动后 `list_tool_info` 含 8 个本地工具（2 Core + 6 System）；System 会话一轮 wire 含 System 标签工具；Agent / Assistant 会话 wire 与实现前一致。

## 改动点

| 文件 | 改动 |
|---|---|
| `src/core/neuron/tools.rs` | 去 `dead_code`、结构体 `pub(crate)` 可见、新增 `register_system_tools` |
| `inserts/` × 6 | 新增 `create_neuron.md` / `list_neurons.md` / `get_neuron.md` / `get_network.md` / `update_neuron.md` / `select_neuron_candidates.md` |
| `src/core/gateway.rs` | `neuron_manager` 创建提前；三处装配调用 `register_system_tools` |
| 测试 | 装配路径相关测试同步；新增 System 标签注册断言 |

## Open Questions

- [x] 6 个工具是否全量上架：**全量上架**（用户确认）。
- [x] `create_neuron` / `select_neuron_candidates` 与 Agent / Assistant 共用底层创建流程，System 会话创建出的神经元按现状入库、无额外权限区分：**不区分**（用户确认默认方案）。工具集只暴露普通神经元治理（创建 / 查询 / 更新 / 选型），不含系统神经元 admin 操作（`ensure_system_neuron` / `set_system_type` / 删除），对系统提示词等关键设施无影响。
