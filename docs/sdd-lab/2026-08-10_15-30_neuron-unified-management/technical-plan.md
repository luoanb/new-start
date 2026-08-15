# Technical Plan / 技术方案: Neuron Unified Management

## Overview / 概述

* 将系统神经元与普通神经元收拢到统一管理：新增 info 容器《神经元》列表视图（分页/搜索/类型筛选/多选/编辑/创建/发起入口），主区《神经元》画布降级为子页面（数据源改为列表选中项），编辑交互在现有 `NeuronDetailDrawer` 上扩展（系统类型绑定/换绑/取消 + 行为管理控件），移除 `session-specs` 面板。

* 后端补齐三件事：分页列表命令、系统类型绑定命令、行为写入放行（裁决类开放编辑）。

## Goals / 目标

* <br />

  1. 后端：新增 `list_neurons_page`（分页+搜索+全部/系统/普通筛选）、`set_neuron_system_type`（绑定/换绑/取消，唯一约束）、`update_neuron_behavior`（全部系统神经元行为可写）；移除 `list_session_specs` / `create_session_spec` / `update_session_spec_behavior` 三个管理面命令（能力被统一命令取代）。

* <br />

  1. 前端布局：info 容器新增《神经元》列表视图（`providers / models / ★ 神经元`）；主区 `neurons` 画布保留为子页面。

* <br />

  1. 列表交互：搜索、类型筛选（全部/系统/普通）、滚动加载更多、单选点击设画布核心、顶栏多选开关、列表项编辑/发起操作、顶部「＋ 创建」。

* <br />

  1. 编辑扩展：`NeuronDetailDrawer` 增加系统类型绑定（非系统神经元）/ 换绑 / 取消（均二次确认）+ 行为管理控件（绑定后显示）。

* <br />

  1. 移除 `session-specs` 面板及其入口（`SessionCreateModal` 的「按系统神经元发起」卡片、`mainViews` / `mainPanelMeta` / `MainPanelType` 登记、i18n 文案、dataStore 相关 action）。

* <br />

  1. 布局持久化 v8 → v9 迁移：info 容器补 `neurons-list` 视图、清理 main 区残留 `session-specs` 面板。

## Non-Goals / 非目标

* 画布数据获取方式改造（保留现状 `list_neurons` 全量 + 连接拉取；`get_network` 优化列为 Phase 3）。

* 画布节点视觉改造（系统徽标圆点为可选增强，不在验收门槛）。

* `try_llm_select` / `generate_drafts` 行为变更。

* 多选核心的「多 seed 展开」语义（沿用现状：仅 `coreSelection[0]` 为画布 seed）。

## Background & Architecture Overview / 背景与架构

* 后端现状：

  * `NeuronStore` 已有 `set_system_type(id, Option<&str>)`（[neuron\_store.rs#L347](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L347)）与唯一索引（[L66](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L66)）、`set_behavior`（[L1070](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L1070)）、`list_neurons`（全量，[L222](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L222)）。

  * `SessionSpecManager` 的 `update_behavior_for_admin` 目前经 `get_session_behavior` 强校验 `session.` 前缀（[spec\_manager.rs#L98-L106](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/spec_manager.rs#L98-L106)）；`ensure_session_neuron`（懒创建）bootstrap 仍在使用，保留。

  * 管理面命令集中在 [lib.rs#L440-L670](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L440-L670)。

* 前端现状：

  * `viewRegistry`（info/panel 容器视图）与 `mainViews` / `mainPanelMeta` / `MainPanelType`（main 区）解耦（[views.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/views.ts)、[layoutTypes.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/layoutTypes.ts#L8-L15)）。

  * `NeuronManager` 现状：顶栏 = 搜索 + 核心 MultiSelect（Top60）+ 创建 + 深度/布局/连线；`coreSelection[0]` 驱动 `canvasSeed`；编辑交互 = `NeuronDetailDrawer`（[NeuronManager.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/NeuronManager.svelte)）。

  * `NeuronDetailDrawer` 现状：system\_type 只读、desc/content/tool\_ids 编辑、权重/连接调整（[NeuronDetailDrawer.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/NeuronDetailDrawer.svelte)）。

  * `dataStore` 现状：`sessionSpecs` 状态 + 5 个系统神经元 action（[dataStore.svelte.ts#L98-L320](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/stores/dataStore.svelte.ts#L98-L320)）。

  * 布局持久化 v8 在 [layoutStorage.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/layout/layoutStorage.ts) 的 `normalize` 分支迁移。

## Design Details / 设计细节

### 1. 后端：统一数据与命令

#### 1.1 `NeuronStore` 新增分页查询

```rust
pub struct NeuronPage {
    pub items: Vec<Neuron>,
    pub total: usize,
    pub has_more: bool,
}

pub enum NeuronKindFilter { All, System, Normal } // 或 String 入参："all" | "system" | "normal"

pub fn list_neurons_page(
    &self,
    page: usize,        // 0-based
    page_size: usize,
    search: Option<&str>,       // 匹配 desc / id（大小写不敏感）
    kind: NeuronKindFilter,     // system = system_type NOT NULL；normal = system_type IS NULL
) -> AppResult<NeuronPage>;
```

* SQL：`SELECT ... WHERE deleted_at IS NULL [AND system_type IS NOT NULL|IS NULL] [AND (desc LIKE ? OR id LIKE ?)] ORDER BY weight DESC, id ASC LIMIT ? OFFSET ?` + `SELECT COUNT(*)` 同条件。

* `page_size` 上限（如 100）防止滥用；`has_more = offset + items.len() < total`。

#### 1.2 `NeuronManager` 新增转发方法

```rust
pub fn list_neurons_page(&self, page, page_size, search, kind) -> AppResult<NeuronPage>
pub fn set_system_type_for_admin(&self, id: &str, system_type: Option<&str>) -> AppResult<Neuron> {
    // 空串归一为 None；system_type 唯一冲突由唯一索引报错，映射为 InvalidInput 友好提示
    self.store()?.set_system_type(id, system_type)
}
pub fn update_behavior_for_admin(&self, id: &str, behavior: SessionBehavior) -> AppResult<Neuron> {
    self.specs.update_behavior_for_admin(id, behavior)
}
```

#### 1.3 `SessionSpecManager::update_behavior_for_admin` 放宽校验

* 现状：`get_session_behavior(id)?` 强校验 `session.` 前缀（仅系统神经元可写）。

* 改为：校验「`system_type` 非空即可」（所有系统神经元行为可编辑，裁决类开放），保持 `set_behavior` 写入。

* 执行面 `get_session_behavior`（`resolve_round` 用）**保持不变**（仍要求 `session.` 前缀，系统神经元会话语义不受影响）。

```rust
pub fn update_behavior_for_admin(&self, id: &str, behavior: SessionBehavior) -> AppResult<Neuron> {
    let neuron = self.store()?.get_neuron(id)?
        .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))?;
    if neuron.system_type.as_deref().is_none() {
        return Err(AppError::InvalidInput(format!(
            "neuron {id} has no system_type; behavior requires a system neuron"
        )));
    }
    self.store()?.set_behavior(id, Some(&behavior))
}
```

#### 1.4 `lib.rs` 命令增删

* 新增：

  * `list_neurons_page(page: usize, page_size: usize, search: Option<String>, kind: String) -> NeuronPage`

  * `set_neuron_system_type(id: String, system_type: Option<String>) -> Neuron`（空串按 None 处理；写后 `StateChange::Neurons`）

  * `update_neuron_behavior(id: String, behavior: SessionBehavior) -> Neuron`（写后 `StateChange::Neurons`）

* 移除：

  * `list_session_specs` / `create_session_spec` / `update_session_spec_behavior`（管理面被统一命令取代）

* 保留：

  * `open_session` / `converse_session`（会话发起与执行；前端列表「发起」入口复用）

  * `update_neuron`（普通字段，与 behavior 分命令写入，避免双写）

### 2. 前端：布局注册与迁移

#### 2.1 `layoutTypes.ts`：version 8 → 9

* `LayoutState.version: 9`；`MainPanelType` 移除 `"session-specs"`。

* `DEFAULT_LAYOUT.containers.info.views = ["providers", "models", "neurons-list"]`。

#### 2.2 `views.ts`

* `viewRegistry` 新增：

  ```ts
  neuronsList: { id: "neurons-list", title: "views.neuronsList", component: NeuronListPanel, movableTo: "*" }
  ```

* `mainViews` 移除 `session-specs` 条目；`mainPanelMeta` 移除 `session-specs`。

#### 2.3 `layoutStorage.ts`：v8 → v9 迁移分支

* 在 `normalize` 增加 `if (parsed.version === 8)`：`merge` 后：

  * `info.views` 若不含 `"neurons-list"` 则插入 `"models"` 之后（默认位置；用户自定义过 info 也仅追加，不重置）；

  * `main.panes[].panels` 过滤掉 `type === "session-specs"` 的面板（清理旧系统神经元面板）。

* `parsed.version === DEFAULT_LAYOUT.version`（9）走原 merge 路径（合并含默认的 neurons-list）。

#### 2.4 移除 `session-specs` 入口

* 删除 `SessionSpecsPanel.svelte`。

* `SessionCreateModal.svelte`：移除「按系统神经元发起」卡片与 `onOpenSpecs` 属性（系统神经元发起收敛到列表项「发起」）。

* `+page.svelte`：移除 `openSessionSpecs` 处理与 `onOpenSpecs` 传参。

* i18n：删除 `views.sessionSpecs`、`sessionSpecsPanel.*`；新增 `views.neuronsList` 与 `neuronListPanel.*` / `neuronEditor.*` 文案（en + zh）。

### 3. 前端：共享状态与事件（dataStore）

在 `dataStore` 新增（替换 `sessionSpecs` 相关 action）：

```ts
// 列表 ←→ 画布共享
neuronSelection: string[] = [];        // 画布核心（单选=1，多选=数组）
neuronSelectionMode: "single" | "multi" = "single";
neuronEditRequestId: string | null = null;   // 列表「编辑」→ 画布打开抽屉
neuronCreateRequest: number = 0;             // 列表「创建」→ 画布打开创建弹窗（计数触发）
neuronLaunchRequestId: string | null = null; // 列表「发起」→ 打开系统神经元会话

function setNeuronSelection(ids: string[]): void
function toggleNeuronSelection(id: string): void
function requestEditNeuron(id: string): void
function requestCreateNeuron(): void
function requestLaunchNeuron(id: string): void  // 调 openSession(id, "chat") + 插入会话面板
```

* 移除 `sessionSpecs` 状态、`refreshSessionSpecs`、`createSessionSpec`、`updateSessionSpecBehavior`。

* 保留 `openSession` / `converseSession`（`requestLaunchNeuron` 复用 `openSession` + 现有会话面板插入逻辑）。

### 4. 前端：新列表视图 `NeuronListPanel.svelte`

* 位置：info 容器（`ViewHost` 挂载，`viewContext` 自取 store）。

* 状态与交互：

  * 工具栏：搜索输入（防抖 200ms）、类型筛选 `select`（全部/系统/普通）、多选开关（checkbox）、「＋ 创建」按钮。

  * 列表：`dataStore.neuronSelection` 驱动高亮；单选模式点击行 = `setNeuronSelection([id])`；多选模式点击 = `toggleNeuronSelection(id)`。

  * 分页：滚动到底部显示「加载更多 ↓」→ `list_neurons_page(page++, ...)` 追加；搜索/筛选变更重置到第 0 页。

  * 行内容：`system_type` 徽标（前缀映射色板，见 6）+ `desc` + `[编辑]` / 系统神经元显示 `[发起]`。

  * 空态与加载态沿用现有列表样式。

* 刷新：订阅 `dataStore.state.neuronsVersion`（`StateChange::Neurons`）→ 重载第 0 页。

### 5. 前端：`NeuronManager` 改造（画布子页面）

* 移除：搜索输入、`coreSelection` MultiSelect（Top60）、`topNeurons`/`coreOptions`、`search`/`filteredNeurons` 的搜索过滤逻辑（`filteredNeurons` 退化为全量 `neurons`）。

* 数据源改共享：`$effect` 监听 `dataStore.neuronSelection` → `canvasSeed = selection[0]`；空选择保持现有空态。

* 保留：深度 slider、布局 Select、连线方式 Select、`clearFilters` 等价物移除、「设为画布核心」节点操作（`onSetSeed` 改为写回 `dataStore`：单选替换 / 多选 append）。

* 创建弹窗：保留组件与逻辑；触发改为 `$effect` 监听 `dataStore.neuronCreateRequest` 变化 → `openCreateOrphan()`（创建后清空该请求计数消费）。

* 编辑抽屉：`$effect` 监听 `dataStore.neuronEditRequestId` → `openDrawer(id)` 并消费（置 null）。

* 全量数据加载保留（`load()` 现状），供画布 BFS；列表分页与画布全量并存（Phase 3 可优化为 `get_network`）。

### 6. 前端：`NeuronDetailDrawer` 扩展

* 新增区域（`drawer-body` 内，分隔线）：

  * **系统类型区**：未绑定 → 显示「未绑定」+ `[绑定]`（输入框输 system\_type）；已绑定 → 显示 `system_type`（mono）+ `[换绑]` `[取消绑定]`。

  * **行为管理区**（仅 `system_type` 非空显示）：复用抽离后的 `BehaviorFields`（selection / tools / insert\_id），保存调 `update_neuron_behavior`。

* 二次确认：`[换绑]` / `[取消绑定]` 先弹确认对话框（内容：将换绑为 X / 将取消绑定并变为普通神经元、行为控件隐藏），确认后调 `set_neuron_system_type`。

* 保存编排：

  * desc/content/tool\_ids → `update_neuron`（现状不变，`handleSave`）。

  * system\_type → `set_neuron_system_type`（独立操作即时保存 + 二次确认）。

  * behavior → `update_neuron_behavior`（独立操作，绑定后表单「保存行为」按钮）。

* 刷新：任一保存成功后调 `onChanged`（现状已触达 `refreshDrawerAndGraph`）并依赖 `neuronsVersion` 同步列表。

### 7. `BehaviorFields` 抽离复用

* 从 `SessionSpecsPanel.svelte` 把 behavior 表单 snippet（selection 下拉 + global limit + tools 下拉 + allowlist + insert\_id）抽为 `BehaviorFields.svelte` 组件（props：`behavior: SessionBehavior | null`），供抽屉复用；`SessionSpecsPanel` 删除后无旧调用方。

### 8. 系统类型徽标前缀映射

* 前端工具 `systemTypeColor(type: string): string`：`session.*` → `--color-system-core`、`assistant_*` → `--color-system-assistant`、其余 → `var(--color-system-${type}, var(--color-system-default))`（保持现有 `var()` 回落机制）。

## Edge Cases / 边界情况

* 绑定 system\_type 已存在 → 唯一索引冲突：错误文案「system\_type X 已被 neuron Y 使用」，前端展示于抽屉错误区，不落库。

* 普通神经元直接出现行为区？→ 不应发生：行为区由「system\_type 非空」条件渲染；后端 `update_neuron_behavior` 同样拒绝无 system\_type。

* 列表分页 + 画布全量刷新时序：`StateChange::Neurons` 触发列表重载第 0 页 + 画布 `load()`（现状）；已加载更多页在刷新后丢弃，由用户再滚动加载（简单一致）。

* 空选中（列表全取消）：画布回到空态（现状 `!canvasSeed` 分支）。

* 多选模式下「设为画布核心」：append 进 `neuronSelection`（去重）；单选模式替换。

* 迁移：老用户布局无 `neurons-list` → 自动插入；main 区遗留 `session-specs` 面板 → 过滤移除。

## Testing Plan / 测试方案

* 后端（`cargo test --lib`）：

  * `list_neurons_page`：分页边界（首页/末页/超页）、search 命中、kind 三档过滤、has\_more 正确。

  * `set_system_type_for_admin`：普通绑定成功 / 换绑成功 / 取消成功 / 唯一冲突报错 / 空串归一 None。

  * `update_behavior_for_admin`：`session.*` 可写（回归）、`assistant_*` 可写（新）、无 system\_type 拒绝。

  * 移除 `session-specs` 相关测试的适配（如有引用 `create_session_spec` 命令的测试改走统一命令）。

* 前端：`svelte-check` 0 errors；`cargo check --all-targets` 通过。

* 手动验证路径：

  1. info 栏出现「★ 神经元」tab；搜索/筛选/加载更多正常。
  2. 单选点击 → 主区画布以该项为核心展开；多选开关 → 勾选多项。
  3. 列表「编辑」→ 抽屉打开；绑定 system\_type → 出现行为控件并保存生效；换绑/取消二次确认。
  4. 列表「＋ 创建」→ 创建弹窗（孤立）；「发起」→ 打开系统神经元会话。
  5. 旧布局迁移：升级后 info 自动含神经元列表、session-specs 面板消失。

## Migration Plan / 迁移计划

* 布局持久化 v8 → v9：见 Design Details 2.3（`layoutStorage.normalize` 分支）。

* 后端命令删除属破坏性变更：本迭代一次性落地（前端同步移除调用方），无灰度。

## Open Questions / 开放问题

* [x] 列表「发起」默认模式：**已确认**（2026-08-10）：默认以 `chat` 模式发起系统神经元会话——`requestLaunchNeuron(id)` 调 `open_session(id, "chat")`（后续如需 chat/agent/assistant 选择，作为增强另立迭代）。

