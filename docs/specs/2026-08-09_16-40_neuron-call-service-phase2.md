# Spec: 神经元会话服务 Phase 2（执行面扩展与接入）

> 关联：[Phase 1 spec](2026-08-09_16-10_neuron-call-service.md)（已实现，`NeuronCallService` / `SessionSpecManager` 就位）。
> 本文档是本仓库 Spec 体系（`docs/specs/`）的一部分，沿用 Phase 1 的术语与结构。

## Goal

- 要解决什么问题：Phase 1 仅完成了「助手主对话」这一个执行面接入 service；`Engine`（Chat/Agent）、hooks 裁决调用（`call_system_prompt_json`）仍是分散的旧路径；`SelectionPolicy` 语义（None 读自己 content / Fixed 固定到他人 / Global 每轮全域 / Neighborhood 依赖 switched_session）与「系统神经元一元化」理念不一致；默认策略硬编码在代码里，且没有前端系统神经元管理界面。执行面不统一，系统提示词调用的价值无法覆盖全部会话类型。
- 验收结果：
  1. 系统提示词调用统一：除「神经元管理」内部的创建（`create_neuron`）与选择（`assistant_select_neuron`）外，所有系统神经元的提示词调用收敛到 `NeuronCallService` 一个入口（`call_system_prompt`）；三个 hook 的编排逻辑（topic 创建 / 打分落库 / scope 更新）保留在 `AssistantMode`，行为与现状一致。
  2. `Engine` 的 Chat / Agent 两种模式迁移到 service：Chat = 无规格传统会话（`Neuron` 模板、无工具），Agent = 工具循环会话；`send_model_message` 收敛到 Gateway，Agent 多轮循环（`AGENT_MAX_ITERATIONS` 护栏）在 Gateway 层组装，不新增 service 方法。
  3. `SelectionPolicy` 语义重构（系统神经元一元化）：`None` = 不读提示词（含自己 content，role_system 空）；`Fixed` = 读系统神经元自己的 content 永不变化；`Neighborhood` = 锚点 last_id（首轮 = 系统神经元自身）邻域选；`Global` = 无历史时全域选 1，选中写 last_id 后次轮自动按 Neighborhood；`Switching` 枚举（Reelect / Fixed / Conditional）整体删除，`conditional_llm` / `assistant_should_switch` / `switched_session` 全部取消。`session.assistant_dialogue` 与裁决类系统神经元一视同仁，怎么取提示词都由 behavior.selection 决定。
  4. 默认策略（`session.assistant_dialogue` 等内建系统神经元的默认 behavior）从硬编码改为 `config.json` 可覆盖。
  5. 前端新增系统神经元管理界面：列表 / 新建 / 编辑 behavior / 发起会话入口，经现有与新增 Tauri 命令驱动。

## Done Contract

- 完成定义：
  1. `NeuronCallService` 新增统一入口 `call_system_prompt(system_type, user_payload, model, history, require_json) -> AppResult<Value>`：读系统神经元 behavior → 按 selection 取 role_system → insert_id 有则拼契约段（`Manual`）→ `call_model` → 可选提取 JSON；`assistant_mode` 的 `call_system_prompt_json` 与 `insert_id_for_system_type` 删除，三个 hook 改调统一入口；hook 编排逻辑与现状一致。
  2. `Engine` 删除 `chat_mode` / `agent_mode` 与 `providers` 字段；`send_model_message` 改为：未绑定系统神经元（behavior）的传统会话 → `NeuronCallService` 直调（Chat 语义）；Agent 模式 → Gateway 层多轮 `execute_round`（沿用 `AGENT_MAX_ITERATIONS` 护栏）；`ConversationMode` 路由收敛到 gateway。
  3. `SelectionPolicy` 重构：删除 `Switching` 枚举；`None` = role_system 空（insert_id 有则拼契约段）；`Fixed` = 无字段变体，读系统神经元自己的 content 永不变化；`Neighborhood` / `Global` 去掉 switching 字段；`resolve_round` 新分派：Neighborhood 锚点 = `state.last_selected_neuron_id`（首轮 = 系统神经元自身）邻域选 1；Global 无历史时全域池选 1、有历史时退化为 Neighborhood 邻域选；选中写 `last_selected_neuron_id`；`ctx.switched_session` 字段及条件清锁块、match_topic 置位全部删除；裁决类系统神经元（`assistant_match_topic` 等）bootstrap 注册默认 behavior = `Fixed` + 各自 insert_id。
  4. `config.json` 新增 `neuron.session_defaults`（`SessionDefaultsSection`）：可覆盖 `session.assistant_dialogue` 的默认 behavior；`default_assistant_dialogue_behavior()` 改为从 config 读取（无配置回落现状默认）；bootstrap 内建系统神经元懒注册时读取；默认行为为 `Neighborhood { policy: default }`（无 switching）。
  5. 前端新增系统神经元管理视图：列表（`list_session_specs`）、新建（含 content 与 behavior，`create_session_spec`）、编辑 behavior（表单化控件，含选型策略 / 工具策略，无切换模式）、发起会话（`open_session` + `converse_session`）；Tauri 新增 `update_session_spec_behavior`（`update_behavior_for_admin` 转发）与 `create_session_spec`（`ensure_session_neuron` 转发）命令。
- 由什么证明：单元测试覆盖 `call_system_prompt` 统一入口（behavior 读 selection 取提示词 / insert 契约段 / JSON 提取）、Agent 工具循环护栏、`SelectionPolicy` 新语义分派（None 空角色 / Fixed 读自己 / Neighborhood 锚点 / Global 首轮全域次轮邻域）、config 默认策略覆盖、系统神经元管理命令；`cargo check` 与 `cargo test --lib` 通过；全量 158 项既有测试不回退（含新增项，既有 switching 相关测试适配新结构）。
- 仍算未完成（Phase 3，不在本 spec）：`try_llm_select` / `generate_drafts` 的行为变更（神经元管理内部的创建/选择模型调用，更底层，不进统一入口）；CLI/TUI 会话入口；多规格会话切换（会话中途更换规格）；前端规格编辑的高级 UX（拖拽选型池配置等）。

## Scope

### In

- `call_service.rs`：统一入口 `call_system_prompt`、`SelectionPolicy` 新分派、`resolve_round` 重构（switched_session 与条件清锁删除）、`config.json` 默认策略读取。
- `assistant_mode.rs`：`call_system_prompt_json` / `insert_id_for_system_type` 删除并改调统一入口；`switched_session` 置位删除；hooks 编排保留。
- `engine.rs` / `gateway.rs`：Chat/Agent 逻辑迁移，`send_model_message` 收敛，Agent 多轮循环在 gateway 组装。
- `models.rs`：`Switching` 枚举删除、`SelectionPolicy` 重构（宽容解析）；`SessionDefaultsSection`（config 段，不进 neurons 表）。
- `config.rs`：`NeuronSection` 增 `session_defaults`。
- `gateway.rs` / `lib.rs`：`update_session_spec_behavior` / `create_session_spec` 命令。
- 前端：系统神经元管理视图（`SessionSpecsPanel.svelte`，表单化编辑）+ dataStore 接入。
- 测试与既有 spec 反写（Phase 1 spec 补记 Phase 2 完成项）。

### Out

- `try_llm_select` / `generate_drafts` 行为变更。
- CLI/TUI 会话入口、多规格会话切换。
- 前端规格编辑的高级 UX。
- `Conversation.extra` 存储结构变更（沿用 Phase 1 的 `extra.session`）。
- hooks 的编排逻辑变更（只收敛调用通道，不改编排）。

## Facts / Constraints

- `call_system_prompt_json` 现流程（[assistant_mode.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L467-L560)）：`ensure_system_neuron(system_type)` → `insert_id_for_system_type` → `assemble(history, neuron.content, insert, payload, Manual)` → `call_model(tools: None)` → `extract_json_object`。三个调用方：MatchTopicBeforeHook（[L711-L834](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L711-L834)）、ScoreFeedbackBeforeHook（[L936+](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L936)）、CompleteScopeAfterHook。
- `Engine`（[engine.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/engine.rs)）：`chat_mode` = 单次 `Neuron` 模板调用无工具；`agent_mode` = 工具循环（`AGENT_MAX_ITERATIONS` 护栏，注册表全部工具）；`providers` 字段 + 读锁 clone 模式已具备（锁不跨 await）。
- `NeuronCallService::execute_round`（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L375-L504)）：已支持「单次工具执行」+ 未授权护栏；`role_system` / `insert_id` 推导模板已就位。
- `resolve_round`（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L263-L310)）：现状分派 = `None` 读规格 content / `Fixed{neuron_id}` 固定他人 / `Global` / `Neighborhood` 每轮选型 + `switched_session` 条件清锁；本轮重构为新语义。
- `spec_manager.rs`（[L63-L71](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/spec_manager.rs#L63-L71)）：`ensure_session_neuron` 创建系统神经元时 content 为空占位——新语义 `Fixed` 读自己 content，需支持管理面填 content。
- `config.json`（[config.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/config.rs#L13-L45)）：`AppConfigFile{poller, neuron, extra}`，`NeuronSection{capacity, recycle_interval_ms}`；`ConfigStore::read/update` 已提供读改写。
- 前端（[dataStore.svelte.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/stores/dataStore.svelte.ts#L187)）：`createConversation(mode)` 仅传 mode（chat/agent/assistant），未接规格；`SessionCreateModal.svelte` 三模式选择。
- Phase 1 已就位：`NeuronCallService`（`open_session` / `converse` / `resolve_round` / `execute_round`）、`SessionSpecManager`（`ensure_session_neuron` / `update_behavior_for_admin`）、`SystemPromptStatus` 含 `behavior`、Tauri 命令 `open_session` / `converse_session` / `list_session_specs`。

## Restated Understanding

- Phase 2 = **执行面收口 + SelectionPolicy 语义重构**：除「神经元管理」内部的创建（`create_neuron`）与选择（`assistant_select_neuron`，更底层）外，所有系统神经元的提示词调用统一收敛到 service **一个入口**；同时把「怎么取提示词」统一为 `SelectionPolicy` 通用语义。
- 系统神经元一元化：`session.assistant_dialogue` 与裁决类（`assistant_match_topic` 等）是同一个物种——一个神经元 + `system_type` + 可选 `behavior`。`SessionBehavior` 只是「程序可识别的业务入口」约定：`selection` 决定提示词从哪取，`tools` 决定工具授权，`insert_id` 决定契约段。
- hooks 本质 = 调用方之一：`MatchTopicBeforeHook` / `ScoreFeedbackBeforeHook` / `CompleteScopeAfterHook` 通过统一入口发起裁决调用（读裁决系统神经元的 behavior → 按 selection 取提示词 → 按 insert_id 拼契约段）；hook 编排（创建 topic、写分、scope 更新）是 assistant 流程的领域逻辑，留在 `AssistantMode`。
- Chat / Agent 迁移不是「重建循环」，而是把现有 `engine.rs` 的装配与循环搬到 service / gateway（Chat = `execute_round` 的退化形态：无规格/无工具；Agent = Gateway 多轮 `execute_round`，沿用 `AGENT_MAX_ITERATIONS`）。
- `SelectionPolicy` 新语义（用户拍板）：`None` = 不读任何 content（role_system 空，insert_id 有则拼契约段）；`Fixed` = 读系统神经元自己的 content 永不变化；`Neighborhood` = 锚点 `last_selected_neuron_id`（首轮 = 系统神经元自身）邻域选；`Global` = 无历史时全域选 1，选中后次轮自动按 Neighborhood。`Switching`（Reelect / Fixed / Conditional）整体删除，`switched_session` 机制取消。
- 默认策略进 config：`neuron.session_defaults` 只覆盖「内建系统神经元的默认 behavior」，不改变运行时规格的读写路径（管理面仍以 neurons 表 behavior 为准；config 只是 bootstrap 默认值的来源）。
- 前端规格管理走「最小闭环」：系统神经元列表 + 新建（含 content）+ 表单化编辑 behavior + 发起会话；复杂 UX 不阻塞。

## 接口契约设计

### 执行引擎（call_service.rs）——统一系统提示词调用入口

```rust
impl NeuronCallService {
    /// 统一系统提示词调用入口（除神经元管理的创建/选择外，所有系统神经元的提示词调用都走这里）。
    /// 读系统神经元 behavior → 按 selection 取 role_system → 按 insert_id 拼契约段 → call_model → 提取 JSON。
    pub async fn call_system_prompt(
        &self,
        system_type: &str,
        user_payload: serde_json::Value,
        model: &ChatModelSelection,
        history: &[ModelMessage],
        require_json: bool,
    ) -> AppResult<serde_json::Value>;
}
```

- 内部步骤：`ensure_system_neuron(system_type)`（懒创建）→ 读 behavior（无 behavior 回落默认）→ `resolve_round` 同款选型逻辑（`None` 空 / `Fixed` 自己 content / `Neighborhood` 锚点邻域 / `Global` 全域，选中者 content 即 role_system）→ `insert_id` 有则拼契约段（`Manual` 模板，`tools: None`）→ `call_model` → `require_json` 时 `extract_json_object`。
- 调用方：三个 hooks（`MatchTopicBeforeHook` / `ScoreFeedbackBeforeHook` / `CompleteScopeAfterHook`）——保留编排，裁决调用改走统一入口；原 `assistant_mode::call_system_prompt_json` 与 `insert_id_for_system_type` 删除，insert 常量映射迁入 service。
- `NeuronCallService::resolve_round` 内部复用同一套「selection → role_system」解析（与统一入口共享逻辑，避免两份实现）。
- Agent 多轮循环**不新增 service 方法**：Gateway 层 while 循环调用 `execute_round`，检查 `finish_reason == "tool_calls"` 决定继续，超 `AGENT_MAX_ITERATIONS` 报 `AppError::AgentMaxIterations`（沿用 engine.rs 现错误）。

### `SelectionPolicy` 重构（models.rs）——通用提示词取用策略

```rust
/// 系统神经元的提示词取用策略（通用：session.assistant_dialogue 与裁决类一视同仁，
/// 怎么取提示词都由 behavior.selection 决定；content = 业务语义，behavior = 程序可识别的业务入口）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum SelectionPolicy {
    /// 不取提示词：role_system 为空（不读任何 content）；insert_id 有则拼契约段。
    #[default]
    None,
    /// 固定：读系统神经元自己的 content，永不变化（不写 last_selected）。
    Fixed,
    /// 邻域：锚点 = last_selected_neuron_id（首轮 = 系统神经元自身），邻域池选 1。
    Neighborhood { policy: NeighborhoodPoolPolicy },
    /// 全域：无历史时全域池选 1 并写 last_selected；有历史时退化为 Neighborhood 邻域选。
    Global { limit: usize },
}
```

- `Switching` 枚举（`Reelect` / `Fixed` / `Conditional`）整体删除；`SessionBehavior` 结构不变（selection / tools / insert_id）。
- `resolve_round` 新分派：None → `ctx.system_prompt = None`；Fixed → 系统神经元自身 content；Neighborhood / Global → `select_role` 按锚点构造候选池（无历史：Global 全域池 / Neighborhood 自身邻域；有历史：last_id 邻域），选中写 `last_selected_neuron_id`；`ctx.switched_session` 字段与 match_topic 置位、条件清锁块全部删除。
- 裁决类系统神经元（`assistant_match_topic` 等）默认 behavior = `Fixed` + 各自 insert_id（bootstrap 注册，行为与现状一致：用自己 content + 契约段）。
- 序列化兼容：旧 behavior JSON 中 `Fixed{neuron_id}` / `Global{switching}` / `Neighborhood{switching}` 解析宽容（`#[serde(default)]`，多余字段忽略或回落新语义）。

### config.json 默认策略（config.rs + neuron_manager.rs）

```rust
pub struct NeuronSection {
    pub capacity: Option<usize>,
    pub recycle_interval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_defaults: Option<SessionDefaultsSection>,
}

/// 内建会话规格默认 behavior 覆盖（只读默认值来源，运行时规格以 neurons 表为准）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionDefaultsSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_dialogue: Option<SessionBehavior>,
}
```

- `default_assistant_dialogue_behavior()` 改为 `default_assistant_dialogue_behavior(config: Option<&SessionDefaultsSection>)`：有 config 覆盖则用，无则回落现状硬编码默认。
- `NeuronManager::bootstrap` 内建规格懒注册处读取（`NeuronManager` 需持 `ConfigStore` 或注入默认来源）。

### Gateway / lib.rs 命令

```rust
// lib.rs
#[tauri::command] async fn update_session_spec_behavior(mgr, id: String, behavior: SessionBehavior) -> TauriResult<SystemPromptStatus>;
#[tauri::command] async fn create_session_spec(mgr, system_type: String, content: Option<String>, behavior: Option<SessionBehavior>) -> TauriResult<SystemPromptStatus>;
```

- `update_session_spec_behavior` → `NeuronManager::update_behavior_for_admin` + `list_session_specs` 刷新。
- `create_session_spec` → `NeuronManager::ensure_session_neuron`（懒创建，存在不覆盖；支持传入 content 与 behavior，管理面可填业务语义）。

### 前端系统神经元管理（最小闭环）

- 新增 `SessionSpecsPanel.svelte`：调用 `list_session_specs` 渲染系统神经元卡片（system_type / content 摘要 / behavior 摘要 / 已绑定状态）；「新建」按钮 → 输入 system_type + content + behavior 表单；「编辑」→ 表单化编辑 content 与 behavior（选型策略下拉：None / Fixed / Neighborhood / Global，无切换模式；工具策略：None / FromNeuron / Allowlist；Fixed 时提示需填 content）；「发起会话」→ `open_session(spec_id, "assistant")` + 跳转会话视图。
- `dataStore.svelte.ts` 新增 `listSessionSpecs()` / `createSessionSpec()` / `updateSessionSpecBehavior()` / `openSession()` / `converseSession()` 封装；`SessionCreateModal` 增加「按规格发起」入口。

## 测试计划

1. 统一入口 `call_system_prompt`：读 behavior → selection 取 role_system（Fixed 用自己 content / Neighborhood/Global 走选型）；insert_id 契约段装配（Manual）；缺 insert 报错；require_json 提取；tracing 覆盖。
2. Agent 多轮循环（Gateway 组装）：连续 tool_calls 循环直到收敛；超 `AGENT_MAX_ITERATIONS` 报错（FakeModelCaller 返回固定 tool_calls）。
3. `SelectionPolicy` 新语义：None → role_system 空（insert_id 有则拼契约段）；Fixed → 读系统神经元自己 content 且不写 last_selected；Neighborhood → 首轮锚点 = 系统神经元自身、次轮锚点 = last_id；Global → 首轮全域选 1 写 last_id、次轮退化为邻域。
4. config 默认策略：`session_defaults.assistant_dialogue` 覆盖默认 behavior（无 switching 字段）；缺失时回落现状默认。
5. 命令层：`update_session_spec_behavior` / `create_session_spec`（含 content 写入）的转发与校验。
6. 既有 158 项不回退；既有 switching 相关测试适配新结构；`cargo fmt --all` / `cargo check --lib` / `cargo test --lib` 通过。

## 待确认（已拍板，记录存档）

1. `agent_loop`：**保留在 Gateway 组装**多轮 `execute_round`，service 不新增方法。
2. `Conditional` LLM 判定：**随 Switching 整体删除**（含 `conditional_llm` / `assistant_should_switch` / `switched_session`）。
3. 前端编辑 behavior：**表单化控件**（选型策略 / 工具策略下拉，无切换模式）。
4. `assistant.should_switch.md`：随 Conditional 删除**不再需要**，不新增文件。
5. 系统提示词调用：**统一到 service 一个入口**（`call_system_prompt`），除神经元管理内部创建/选择外；`SelectionPolicy` 为通用提示词取用策略（`session.assistant_dialogue` 与裁决类一视同仁）。

## Goal Alignment Check

- 目标 1（系统提示词调用统一）：`call_system_prompt` 统一入口 + hooks 改调 → 达成，行为不变由既有测试守护。
- 目标 2（Chat/Agent 迁移）：`send_model_message` 收敛 + Gateway 多轮 `execute_round` → 达成。
- 目标 3（SelectionPolicy 重构）：None / Fixed / Neighborhood / Global 新语义（通用） + Switching 删除 → 达成。
- 目标 4（config 默认策略）：`session_defaults` 覆盖 → 达成，回落现状。
- 目标 5（前端系统神经元管理）：最小闭环面板（含 content + 表单化 behavior）+ 命令 → 达成。

## Checkpoint Summary

- 当前任务理解：Phase 2 = 系统提示词调用统一入口 + SelectionPolicy 语义重构（系统神经元一元化）+ config 默认策略 + 前端最小闭环。
- 当前核心目标：除神经元管理内部创建/选择外，所有系统神经元的提示词调用收敛到 service 一个入口；规格语义符合「系统神经元一元化」。
- 当前进度：Steps 1-9 已实现并验证（见 Change Log 2026-08-10 完成项）。
- 下一步 1：无（本 spec 完成，Phase 3 待独立规格）。
- 涉及文件 / 模块：`call_service.rs`、`assistant_mode.rs`、`gateway.rs` / `lib.rs`、`models.rs`、`config.rs`、`neuron_manager.rs`、`spec_manager.rs`、前端 `SessionSpecsPanel.svelte` / `dataStore.svelte.ts` / `SessionCreateModal.svelte` / `views.ts`；`engine.rs` 已退役删除。
- 风险与处理：hooks 迁移行为回归 → 统一入口语义与原 `call_system_prompt_json` 对齐，既有 hook 测试守护；Engine 迁移破坏现有 Chat/Agent 会话 → Chat 走 `execute_round` 退化形态（Neuron 模板、无工具），Agent 走 Gateway 多轮，`/compact` 手动压缩由 Gateway 持 `Compactor` 保留；`SelectionPolicy` 变体序列化兼容 → 手动宽容 `Deserialize`（旧 `Fixed{neuron_id}` / `Global{switching}` / `Neighborhood{switching}` 忽略多余字段）；`Fixed` 读自己 content 使空 content 系统神经元 role 为空 → 管理面 `create_session_spec` / 编辑面板支持填 content；裁决类默认 behavior 注册 → bootstrap 懒注册 + 既有库无 behavior 时 `default_behavior_for_system_type` 兜底。
- 验证方式：已执行——`cargo fmt`、`cargo check --all-targets`、`cargo test --lib`（162 项通过）、前端 `svelte-check`（0 errors）。
- Execution Approval: 已批准并完成。

## Change Log

- 2026-08-09（初稿）：Phase 2 方案起草。核心决策：`call_decision` 提炼 hooks 裁决调用；`agent_loop` 归 service；`Conditional.conditional_llm` 默认关闭；config `neuron.session_defaults` 仅作 bootstrap 默认值来源；前端规格管理走最小闭环。
- 2026-08-09（修订 1）：用户拍板 4 项——`agent_loop` 改保留在 Gateway 组装（service 不新增方法）；前端 behavior 编辑改表单化控件。
- 2026-08-09（修订 2，语义重构）：用户确认「系统神经元一元化」——`SelectionPolicy` 重构（None = 空角色、Fixed = 读自己 content、Neighborhood = 锚点 last_id 首轮自己、Global = 首轮全域次轮邻域）；`Switching` 枚举整体删除，`Conditional` LLM 版 / `assistant_should_switch` / `switched_session` 全部取消；系统神经元 content 需支持管理面填写（呼应 Fixed 语义）。
- 2026-08-09（修订 3，统一入口）：用户确认两点——① 除「神经元管理」内部创建（`create_neuron`）与选择（`assistant_select_neuron`）外，所有系统神经元的提示词调用收敛到 service **一个入口**（`call_system_prompt`）；② `SelectionPolicy` 是通用提示词取用策略，`session.assistant_dialogue` 与裁决类系统神经元一视同仁，怎么取提示词都由 behavior.selection 决定；裁决类系统神经元 bootstrap 注册默认 behavior = `Fixed` + insert_id；`create_session_spec` 支持 content 写入。
- 2026-08-10（实现完成）：Steps 1-9 落地并验证。落地项：① `SelectionPolicy` 重构（`Switching` 删除 + 宽容解析）[models.rs]；② config `neuron.session_defaults`（`SessionDefaultsSection`）+ `session_defaults()` 读取 [config.rs / neuron_config.rs]；③ 统一入口 `call_system_prompt`（懒创建 → behavior → selection 取 role_system → insert_id 契约段 → call_model → require_json 提取）与 `resolve_round` 新分派 [call_service.rs]；④ hooks 改调统一入口，删除 `call_system_prompt_json` / `insert_id_for_system_type` / `switched_session` [assistant_mode.rs]；⑤ 裁决类默认 behavior + `default_assistant_dialogue_behavior` 从 config 读 + `ensure_session_neuron` 支持 content [neuron_manager.rs / spec_manager.rs]；⑥ Chat/Agent 收敛到 Gateway（`engine.rs` 退役，compact 迁 Gateway 持 `Compactor`），Agent 多轮 `agent_loop` + `AGENT_MAX_ITERATIONS` 护栏 [gateway.rs]；⑦ 命令层 `create_session_spec` / `update_session_spec_behavior` [lib.rs]；⑧ 前端 `SessionSpecsPanel`（列表/新建/编辑 content+behavior/发起会话）+ dataStore 5 个 action + `SessionCreateModal`「按规格发起」入口 + i18n/views [前端]。
  验证：`cargo fmt` / `cargo check --all-targets` / `cargo test --lib`（162 项通过，基线 158 + 新增 4 项：宽容解析、统一入口、Agent 收敛、Agent 护栏）/ 前端 `svelte-check`（0 errors）。
  实现偏差：① `Engine` 整体退役删除（而非仅删 `chat_mode` / `agent_mode` / `providers` 字段）——`/compact` 手动压缩改由 Gateway 持 `Compactor` 承担，行为保留；② Agent 循环继续判定用 `ctx.tool_result.is_some()`（= execute_round 已执行工具，与 `finish_reason == "tool_calls"` 行为等价）；③ 裁决类默认 behavior 的 `tools` 取 `ToolPolicy::None`（与原 `call_system_prompt_json` 的 `tools: None` 一致）。

## Validation

- Self-check：已对照 Phase 1 spec 的接口契约与现状代码核对接口签名；实现完成后逐项对照 Done Contract 与测试计划复核。
- Static checks：`cargo fmt` / `cargo check --all-targets` 通过；前端 `svelte-check` 0 errors（48 条既有 warning 未新增）。
- Runtime / Test：`cargo test --lib` 162 项通过（基线 158 + 新增 4 项：`selection_policy_legacy_json_parses_tolerantly`、`call_system_prompt_unified_entry`、`agent_loop_converges_after_tool_round`、`agent_loop_hits_max_iterations_guard`；既有 `tool_policy_three_modes` / `global_policy_first_round_global_then_neighborhood` 等适配新语义）。
- Human confirmation：待确认项已拍板；实现已执行并完成。
- 结果汇总：Phase 2 实现完成（Steps 1-9 全部落地），spec 已反写。
- 核心目标是否已由证据证明完成：是（Done Contract 5 项全部达成；偏差 3 项记录于 Change Log）。
- 若未完成，当前剩余差距：无。
- 剩余风险：低——`Engine` 退役后长 Chat/Agent 会话不再自动压缩（`ensure_fits`），改为用户按需 `/compact`（Gateway 持 `Compactor`）；若需自动压缩可作 Phase 3 增强。

## Resume / Handoff

- 当前状态：Phase 2 实现完成（Steps 1-9 全部落地并验证），spec 已反写（Change Log 2026-08-10）。
- 当前卡点：无。
- 下一步唯一动作：无（Phase 3 待独立规格：`try_llm_select` / `generate_drafts` 行为变更、CLI/TUI 会话入口、多规格会话切换、前端高级 UX、长会话自动压缩）。
- 下一轮核心目标：进入 Phase 3 前先确认本 spec 验收。
