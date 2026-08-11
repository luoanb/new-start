# Spec: NeuronManager 拆分到 core/neuron/ 文件夹（Facade + 领域服务）

## Goal

- 要解决什么问题：`neuron_manager.rs` 2916 行职责过载（查询/选型/创建/演化/工具 6 种职责混在一个 struct），`core/` 目录全部扁平文件、无领域分组。
- 验收结果：`NeuronManager` 拆为 `core/neuron/` 文件夹下的 Facade + 4 个领域服务；**公开 API 与行为完全不变**；`cargo check` 0 error、`cargo test --lib` 全绿。

## Done Contract

- 完成定义：
  1. 新建 `src-tauri/src/core/neuron/` 文件夹，含 `mod.rs` + `manager.rs`（Facade）+ `query.rs` + `selection.rs` + `creation.rs` + `evolution.rs` + `tools.rs` + `store.rs` + `model.rs` + `config.rs` + `spec.rs`。
  2. `NeuronManager` 保留全部公开方法签名，方法体改为一行委托（`self.selection.select_candidates(query).await`）。
  3. 原 `neuron_store.rs` / `neuron_model.rs` / `neuron_config.rs` / `spec_manager.rs` 一并迁入 `core/neuron/`（重命名为 `store.rs` / `model.rs` / `config.rs` / `spec.rs`）。
  4. `core/mod.rs` 由 `pub mod neuron_manager;` 等改为 `pub mod neuron;`，并在 `neuron/mod.rs` 重导出全部既有类型/常量，**外部消费方引用路径不变**。
  5. 领域服务划分（依赖单向，无环）：
     - `NeuronQuery`：纯 store 读/写（查询、网络、管理面、容量回收）
     - `NeuronSelection`：候选池 + 选型 + **生成原语自含**（ensure_creator/generate_drafts/persist_plain/create_neuron_user_prompt/fill_candidates_batch 等全部归自身）
     - `NeuronCreation`：创建/系统神经元/bootstrap/creator 编排（持有 Selection，调用其生成原语与 select_one）
     - `NeuronEvolution`：变体状态机（持有 Selection，复用 ensure_creator）
  6. 打破 `Creation ↔ Selection` 双向依赖的机制：生成原语全部下沉到 `NeuronSelection`（`fill_candidates_batch` 本就在选型域），依赖方向固定为 `Creation → Selection → {store, model_caller}`、`Evolution → Selection`、`Tools → Facade`；跨域副作用（delete_for_admin 的 creator 缓存失效）由 Facade / Creation 调用 `selection.clear_creator_cache_if_matches` 组合完成。
  7. 6 个 Tool 实现（`tools.rs`）继续持有 `Arc<NeuronManager>`（工具需组合访问多服务，留在 Facade 侧）。
- 由什么证明：`cargo test --lib` 全绿（含现有 ~1020 行测试）；`cargo check` 0 error；代码与本 spec 对照。
- 哪些情况仍算未完成：无（本次 In 范围全部落地）。

## Scope

- In:
  - `neuron_manager.rs` 拆分为 `core/neuron/` 下多文件（manager/query/selection/creation/evolution/tools）
  - `neuron_store.rs` / `neuron_model.rs` / `neuron_config.rs` / `spec_manager.rs` 迁入 `core/neuron/`（重命名 store/model/config/spec）
  - 公开 API 保持（Facade 委托），`core/mod.rs` 与 `neuron/mod.rs` 重导出调整
  - 测试迁移为 `manager.rs` 内 `#[cfg(test)] mod tests;` → 子模块文件 `manager/tests.rs`（`use super::*` 语义不变，保持全绿）
- Out:
  - 任何行为变更、公开签名变更、Schema 变更
  - 代码逻辑重构（如演化状态机重写）——本次只做结构性拆分
  - 其它 core 文件（providers/call_service/assistant_mode 等）不迁入

## Facts / Constraints

- 外部消费方引用方式（确认清单）：
  - `assistant_mode.rs:18-19` `neuron_manager::NeuronManager` + `neuron_store::NeuronStore`；`call_service.rs:19,736-738`；`gateway.rs:21-24`；`tool_registry.rs:82` `super::neuron_manager::NeuronManager`
  - `core/mod.rs:17-20,24` 声明 `neuron_config/neuron_manager/neuron_model/neuron_store/spec_manager`
  - 消费方调用的是 `NeuronManager` 的公开方法（如 `get_session_behavior`、`adjust_weight`、`maybe_evolve_creator_variants`、`select_candidates`），Facade 委托后签名不变即可零改动。
- 依赖方向（实际代码核验后确认，非"单向无环"原假设）：
  - `create_neuron`（Creation）→ `select_one`（Selection）选变体；`select_candidates`（Selection）→ `fill_candidates_batch` → `generate_drafts`（模型出网）→ `ensure_creator` + `persist_plain`。
  - 结论：生成原语（`ensure_creator`/`generate_drafts`/`generate_draft`/`fill_candidates_batch`/`create_neuron_user_prompt`/`available_tools_block`/`persist_plain`/`persist_system_root`/`ensure_own_candidate_pool` + `creator_id` 缓存）全部归 `NeuronSelection` 自含。
  - 最终依赖链：`Creation → {Query, Selection, specs}`；`Evolution → Selection`；`Selection → {store, model_caller, config, tool_registry}`；`Tools → Facade`；**无环**。
- 跨域副作用：`delete_for_admin` 的 `creator_id` 缓存失效逻辑移出 Query，由 Facade 与 Creation 在删除后调 `selection.clear_creator_cache_if_matches(id)` 组合完成（行为等价）。
- `rewrite_variant` / `rollback_variant_if_regressed`（私有）归 Evolution；`pick_by_weight`/`now_ms` 归 Selection；`extract_json_object`/`extract_json_array`/`parse_generated_drafts`/`drafts_from_json_value` 归 `model.rs`（共享解析，pub(crate)）。
- 已有先例：`specs: SessionSpecManager` 已是拆出的子组件（`spec_manager.rs`），项目接受子组件化。
- 技术约束：不改 SQLite schema；不引入新 crate；`No Plan Approved, No Execute`。

## 接口契约设计（文件结构）

```
src-tauri/src/core/neuron/
  mod.rs          // pub mod 声明 + lock_error；pub use manager::NeuronManager 及既有类型/常量重导出
  manager.rs      // NeuronManager Facade：struct + Debug + new() 组装各服务 + 全部公开方法委托 + register_ai_tools
  manager/tests.rs // 原 manager.rs 测试模块（~1020 行），`#[cfg(test)] mod tests;` 挂载
  query.rs        // NeuronQuery：get/list/network/connections/update_content/adjust_weight/delete/link/.../recycle
  selection.rs    // NeuronSelection：select_candidates/select_one(_with_history)/select_assistant_candidates/select_one_from(_with_history) + fill_candidates_batch
  creation.rs     // NeuronCreation：create_neuron/ensure_system_neuron/ensure_session_neuron/bootstrap/rebootstrap/ensure_creator/create_plain
  evolution.rs    // NeuronEvolution：record_variant_usage/accumulate_variant_delta/maybe_evolve_creator_variants + rewrite_variant/rollback
  tools.rs        // 6 个 Tool 实现（GetNeuron/ListNeurons/UpdateNeuron/GetNetwork/CreateNeuron/SelectNeuronCandidates）+ required_str
  store.rs        // 原 neuron_store.rs：NeuronStore（SQLite 数据访问）
  model.rs        // 原 neuron_model.rs：NeuronModelCaller trait + DefaultNeuronModelCaller
  config.rs       // 原 neuron_config.rs：NeuronConfigReader
  spec.rs         // 原 spec_manager.rs：SessionSpecManager
```

### 公开方法归属映射（44 个 pub 方法）

| 服务 | 方法 |
|---|---|
| Query | `get/get_neuron/get_by_system_type/get_neuron_by_system_type/list/list_neurons/connections/get_connections/network/get_network/update_content_for_ai/update_content_for_admin/adjust_weight/adjust_edge_weight/list_system_prompt_status/delete_for_admin/link_for_admin/unlink_for_admin/set_tool_ids_for_admin/list_neurons_page/set_system_type_for_admin/mark_used_for_assistant/recycle_if_over_capacity` |
| Selection | `select_candidates/select_one/select_one_with_history/select_assistant_candidates/select_one_from/select_one_from_with_history/ensure_creator` |
| Creation | `create_neuron/ensure_system_neuron/ensure_session_neuron/get_session_behavior/update_behavior_for_admin/list_session_specs/bootstrap/rebootstrap/create_plain` |
| Evolution | `record_variant_usage/accumulate_variant_delta/maybe_evolve_creator_variants` |

> `get_session_behavior/update_behavior_for_admin/list_session_specs` 实际委托 `SessionSpecManager`（`specs` 字段），归 Creation（会话规格与系统神经元创建同域）。

## 改动点

| 文件 | 改动 |
|---|---|
| 新建 `core/neuron/{mod,manager,query,selection,creation,evolution,tools,store,model,config,spec}.rs` | 拆分落地 + 原 4 个文件迁入 |
| `core/mod.rs` | `pub mod neuron_manager/neuron_model/neuron_store/neuron_config/spec_manager;` → `pub mod neuron;`；`pub use` 调整 |
| `core/neuron_manager.rs` / `neuron_store.rs` / `neuron_model.rs` / `neuron_config.rs` / `spec_manager.rs` | 删除（内容迁移） |
| 消费方 | 预期零改动（引用路径经 `neuron/mod.rs` 重导出保持） |

## 兼容性

- 公开 API 零变更：所有外部调用（gateway/assistant_mode/call_service/tool_registry/测试）签名与路径不变。
- `core::neuron_manager::X` 等旧路径兼容：`core/mod.rs` 用内联别名模块 `pub mod neuron_manager { pub use super::neuron::manager::*; }`（neuron_config/neuron_model/neuron_store/spec_manager 同理），旧路径照常解析，消费方零改动。
- 行为不变：Facade 只转发，不重写逻辑；`delete_for_admin` 缓存失效改为组合调用，行为等价。

## Validation

- `cargo check`：0 error。
- `cargo test --lib`：全绿（现有测试即回归验证，行为未变的证明）。
- `cargo clippy`（如项目已用）：无新增警告。
- 手动验证：App 正常 bootstrap、选型、多轮对话、变体演化日志照常。

## Change Log

- 2026-08-12：初始 micro-spec。决策：B 方案（Facade + 4 领域服务）；`core/neuron/` 文件夹；公开 API 零变更。
- 2026-08-12（更新）：用户确认**一步到位**——`neuron_store` / `neuron_model` / `neuron_config` / `spec_manager` 一并迁入 `core/neuron/`（重命名 store/model/config/spec）；引用面已核对（4 文件均未逃出 core 内部，外部经 Gateway 间接使用）。
- 2026-08-12（完成）：全部落地。`cargo check --lib` 0 error 0 warning；`cargo test --lib` 167 passed / 0 failed。测试模块移入 `manager/tests.rs`；`DEFAULT_SELECT_N`/`MAX_CREATE_NEURON_COUNT` 调整为 `pub(crate)`（兄弟服务模块复用）。
