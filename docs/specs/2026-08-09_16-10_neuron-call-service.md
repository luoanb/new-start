# Spec: 神经元会话服务（NeuronCallService）

> **⚠️ Phase 2 语义重构（2026-08-09）**：本文档中以下定义已被 [Phase 2 spec](2026-08-09_16-40_neuron-call-service-phase2.md) 取代，**以 Phase 2 为准**：
> - `SelectionPolicy`：`None` 改为「不读任何 content（role_system 空）」；`Fixed { neuron_id }` → `Fixed`（读系统神经元自己的 content，永不变化）；`Global` / `Neighborhood` 移除 `switching` 字段。
> - `Switching` 枚举（`Reelect` / `Fixed` / `Conditional`）整体删除；`switched_session` 机制、`Conditional` 语义、`Conditional` LLM 判定版全部取消。
> - 系统神经元一元化：不存在「规格神经元」类别，管理的是系统神经元；`SessionBehavior` 只是程序可识别的业务入口约定（`selection` 决定怎么取提示词，`session.assistant_dialogue` 与裁决类一视同仁）。
> - 系统提示词调用统一到 `NeuronCallService::call_system_prompt` 一个入口（除神经元管理内部创建/选择外）。

## Goal

- 要解决什么问题：模型调用入口分散在 `Engine`（Chat/Agent）、`AssistantMode::run_core`、`call_system_prompt_json`、`neuron_manager::try_llm_select` / `generate_drafts` 多处；「绑定系统神经元 + 候选池选型 + 工具授权 + 是否切换神经元」这组行为没有统一的、可复用的声明式承载，调用方无法「管理好一个规格后基于它发起会话」。
- 验收结果：
  1. 「会话规格」以带 `behavior` 字段的**系统神经元**承载（`system_type = 'session.<id>'`），`content` = role_system，`behavior` = 策略（选型（含切换模式）/ 工具）+ 可选 `insert_id`（契约正文）。
  2. 新增 `NeuronCallService` 作为唯一执行引擎：`open_session(规格神经元)` 发起会话，`converse(session, input, model)` 一轮执行。
  3. 单候选短路：候选池仅 1 个时跳过选型模型直接使用。
  4. 助手主对话（`run_core`）迁移接入该服务作为第一个调用方，行为保持不变；其本质 = 「无固定业务神经元的动态选型会话」。

## Done Contract

- 完成定义：
  1. `neurons` 表新增 `behavior` 列（TEXT JSON），`Neuron.behavior: Option<SessionBehavior>`，旧行解析失败回落 `None`。
  2. `SessionBehavior` 两策略（`selection` 内含切换模式 / `tools`）+ `insert_id`（契约正文来源）序列化闭环；拼接规则按 insert_id 有无推导（不进配置）、单候选短路为硬规则；`NeuronManager` 经内部 `SessionSpecManager` 子组件提供 `ensure_session_neuron`（懒创建）/ `get_session_behavior` / `update_behavior_for_admin` / `list_session_specs`。
  3. `NeuronCallService` 提供 `open_session` / `converse` / `resolve_round` / `execute_round`；`converse` 内部按「resolve 规格 → role 解析/选型（含 n=1 硬规则短路）→ 工具授权 → 装配调用 → 按 selection 内 switching 写回会话态」执行。
  4. 会话态 `SessionState`（`last_selected_neuron_id` / `last_intervention_at` / `intervention_neuron_ids`）从 `topic.extra.assistant` 迁至 `conversation.extra.session.state`；`poll_count` 留 `topic.extra.assistant`；旧数据读取回退兼容。
  5. 助手 `SelectNeuronBeforeHook` + `authorize_tools` + `run_core` 三步收敛为 service 两阶段调用（`resolve_round` → `execute_round`），助手行为与现状一致（含 `persist_selected_neuron` 语义迁移到会话态）。
  6. Gateway 新增 `open_session` / `converse_session` / `list_session_specs` 命令。
  7. 内建规格注册：`session.assistant_dialogue`（助手主对话，默认行为）；bootstrap 时懒创建。
- 由什么证明：单元测试覆盖 behavior 序列化/旧行兼容、n=1 硬规则短路、selection 内 Fixed/Reelect 切换、工具三策略、`converse` 一轮端到端；`cargo check` 与 `cargo test --lib` 通过；全量 158 项既有测试不回退（含 call_service 新增 8 项）。
- 仍算未完成（Phase 2，不在本 spec）：hooks（match_topic / score_feedback / complete_scope）的 `call_system_prompt_json` 规格化迁移；Engine Chat/Agent 迁移到 service；`Conditional` 的 LLM 判定版；默认策略接入 `config.json`；前端规格管理界面。

## Scope

### In

- `models.rs`：`SessionBehavior` + `SelectionPolicy`（含 `Switching`）/ `ToolPolicy`；`Neuron.behavior`；`Conversation.extra`。
- `neuron_store.rs`：`behavior` 列迁移（沿用 `has_column` + `ALTER TABLE`）、读写、宽容解析。
- `neuron_manager.rs`：内部持有 `SessionSpecManager` 子组件（规格管理面），对外薄转发 `ensure_session_neuron` / `get_session_behavior` / `update_behavior_for_admin` / `list_session_specs`；本体创建复用 `ensure_system_neuron` 骨架。
- 新建 `core/spec_manager.rs`：`SessionSpecManager`（behavior 读写/校验/规格列表），仅持 `store`（与 NeuronManager 共享同一 `Arc<Mutex<NeuronStore>>`），不反向依赖 NeuronManager。
- 新建 `core/call_service.rs`：`NeuronCallService` + `SessionState` + 会话态读写；经 `NeuronManager` 读规格（`get_session_behavior`）。
- `assistant_mode.rs`：`SelectNeuronBeforeHook` / `authorize_tools` / `run_core` 收敛到 service；会话态读取回退兼容。
- `gateway.rs` / `lib.rs`：`open_session` / `converse_session` / `list_session_specs` 命令。
- 测试与既有 spec 反写。

### Out

- hooks 裁决调用（`call_system_prompt_json`）迁移——保留现有实现，仅保持对外行为。
- Engine Chat/Agent 迁移、CLI/TUI 新入口。
- 前端规格管理界面（行为编辑 UI）。
- `Conditional` LLM 判定、`config.json` 策略覆盖。
- `try_llm_select` / `generate_drafts` 行为变更（本轮不动其装配，仅收益于 store 层行为列）。

## Facts / Constraints

- `neurons` 表已有成熟迁移模式（[neuron_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L54-L134)：`has_column` + `ALTER TABLE ADD COLUMN`，兼容旧库）。
- `system_type IS NOT NULL` 的神经元天然获得三项保障：排除业务候选池（[list_global_candidates](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L902-L937) 的 `WHERE system_type IS NULL`）、豁免低价值回收（[select_low_value](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L1013-L1041)）、`system_type` 唯一索引（[init_table](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_store.rs#L65-L72)）。规格神经元挂在 `system_type` 下自动隔离。
- `Conversation`（[models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L94-L102)）为 JSON 文件存储，字段 `id / mode / messages / created_at / updated_at`；新增 `#[serde(default)] extra` 即可向后兼容旧文件。
- 助手会话态现状（[assistant_mode.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L1343-L1352)）：`AssistantTopicState{poll_count, last_selected_neuron_id, last_intervention_at, intervention_neuron_ids}` 存于 `topic.extra.assistant`。
- `run_core` 现流程（[assistant_mode.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L473-L648)）：user_input/nudge 落库 → `ModelCallInput::assemble(history, role_system, "", user_input, Neuron)` → `definitions_for(authorized_tool_ids)` → `providers.call_model` → 单次工具执行与落库 → `persist_selected_neuron`。
- 选型现流程（[SelectNeuronBeforeHook](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L925-L961)）：`last_selected_neuron_id` → `neighborhood_default / global_default` 装配候选池 → `select_one_from_with_history(candidates, messages)` 选 1。**当前即使候选池仅 1 个也调用选型模型**。
- `call_system_prompt_json` 现流程（[assistant_mode.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_mode.rs#L650-L700)）：`ensure_system_neuron(system_type)` → `insert_id_for_system_type` → `assemble(history, neuron.content, insert, payload, Manual)` → `call_model(tools: None)`——即「固定系统神经元 + Manual 模板」的规格化调用原型。
- `ensure_system_neuron(system_type, EnsureSystemOpts{reset})`（[neuron_manager.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/neuron_manager.rs#L560-L653)）已具备「存在复用 / 缺失生成草稿 + persist_system_root」的懒创建骨架。
- `ModelAppendTemplate` 现有 `Neuron` / `Manual` 两变体（`model_call_input.rs`）；本轮 `execute_round` 按 `insert_id` 有无推导模板（有 → Manual 契约段，无 → Neuron），模板不暴露为 `SessionBehavior` 字段。
- `NeighborhoodPoolPolicy` / `AssistantCandidateScope` 已存在（`neuron_manager.rs`），`SelectionPolicy::Neighborhood` 复用。

## Restated Understanding

- 方案 D = **管理面（规格神经元：content + behavior）+ 执行面（NeuronCallService）+ 运行态（会话级 SessionState）** 三层。
- 助手主对话的本质是「无固定业务神经元的动态选型会话」→ 映射为内建规格 `session.assistant_dialogue`（`selection: Neighborhood`（`switching: Reelect`）、`tools: FromNeuron`）。
- 单候选短路：候选池仅 1 个时**跳过选型模型**，直接以该候选为 role（等价「current 唯一则必选」）；为**不可配置硬规则**，无开关参数。
- 工具授权：`FromNeuron` = 本轮 role 神经元 `tool_ids ∩ 注册表`（现状语义）；`Allowlist` 显式白名单；`None` 不授权。
- 切换模式（`Switching`，仅 `Global` / `Neighborhood` 携带；`None` / `Fixed` 蕴含永不切换）：
  - `Fixed`：首轮选型后 `last_selected` 即定，后续轮次不再装配候选池 / 不调选型模型，直接复用。
  - `Reelect`：每轮基于 `last_selected` 重新装配并选 1（现状助手行为）。
  - `Conditional`：默认复用 `last_selected`；当本轮调用方标记「上下文已变」（如 `match_topic` 发生 switch/创建）时强制重新选型。第一版为规则版，不引入 LLM 判定。
- 会话态 `last_selected` 的读写从 `topic.extra.assistant` 迁至 `conversation.extra.session.state`；`poll_count` 与干预窗口字段保留在 topic（干预窗口属助手课题语义，Phase 2 再统一）。
- 兼容：旧 `topic.extra.assistant.last_selected_neuron_id` 在会话态缺失时回退读取一次（写入走新位置）。

## 接口契约设计

### 数据模型（models.rs）

```rust
/// 动态选型（Global / Neighborhood）的切换模式；SelectionPolicy::None / Fixed 不涉及切换。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum Switching {
    #[default]
    Reelect,                                // 每轮重新选型（现状助手行为）
    Fixed,                                  // 首轮选定后锁定 last_selected，后续直接复用
    Conditional,                            // 默认复用；上下文已变时强制重选
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum SelectionPolicy {
    #[default]
    None,                                   // 不选型：role = 规格神经元自身（蕴含永不切换）
    Fixed { neuron_id: String },            // 固定业务神经元（蕴含永不切换）
    Global { limit: usize, switching: Switching },              // 全局池 → 选 1
    Neighborhood { policy: NeighborhoodPoolPolicy, switching: Switching }, // 邻域池 → 选 1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ToolPolicy {
    #[default]
    None,
    FromNeuron,                             // 本轮 role 神经元 tool_ids ∩ registry
    Allowlist(Vec<String>),                 // 显式白名单 ∩ registry
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBehavior {
    #[serde(default)]
    pub selection: SelectionPolicy,
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_id: Option<String>,          // 契约正文来源（InsertCatalog::require）
    // 无 template 字段：拼接规则不进配置，execute_round 按 insert_id 有无推导
    // （有值 → Manual 契约段；无值 → Neuron 角色拼接，与 run_core 现状一致）。
    // 无 single_candidate_shortcut：候选池仅 1 个 → 跳过选型模型为不可配置的硬规则。
}

// Neuron 追加（serde 默认 None，兼容旧 JSON/行）
pub struct Neuron { /* 现有字段不变 */ pub behavior: Option<SessionBehavior> }

// Conversation 追加（serde 默认 None，兼容旧文件）
pub struct Conversation { /* 现有字段不变 */ #[serde(default)] pub extra: Option<serde_json::Value> }
```

### 存储层（neuron_store.rs）

```rust
// init_table 追加（沿用 has_column 迁移模式）
if !has_column(&conn, "neurons", "behavior")? {
    conn.execute("ALTER TABLE neurons ADD COLUMN behavior TEXT", [])?;
}
// row_to_neuron：behavior 列解析失败回落 None（宽容），不阻断整行读取。
```

### 规格管理（新建 core/spec_manager.rs，NeuronManager 内部持有）

```rust
/// 规格子组件：承载 session.* 系统神经元的 behavior 语义（数据在 Neuron.behavior 列，
/// 管理逻辑在此组件）。由 NeuronManager 内部持有，仅共享 store，不反向依赖 NeuronManager。
pub struct SessionSpecManager {
    store: Arc<Mutex<NeuronStore>>,   // 与 NeuronManager 共享同一实例
}

impl SessionSpecManager {
    /// 校验并取规格：要求 system_type 非空 且 behavior 非空，否则 AppError::InvalidInput。
    pub fn get_session_behavior(&self, neuron_id: &str) -> AppResult<SessionBehavior>;

    /// 管理面更新入口：只写 behavior，不触碰 content（避免与 update_content_for_admin 双写）。
    pub fn update_behavior_for_admin(&self, id: &str, behavior: SessionBehavior) -> AppResult<Neuron>;

    /// 规格列表（system_type LIKE 'session.%' + behavior 摘要，供 list_session_specs）。
    pub fn list_specs(&self) -> AppResult<Vec<SystemPromptStatus>>;
}

// neuron_manager.rs：子组件持有 + 薄转发（不承载规格语义逻辑）
impl NeuronManager {
    pub specs: SessionSpecManager,   // 内部子组件（Rust 组合字段，类比"子类"）

    /// 懒创建规格神经元：本体创建委托 ensure_system_neuron 骨架；新建时经 specs 写 behavior（存在不覆盖）。
    pub async fn ensure_session_neuron(&self, system_type, behavior, opts) -> AppResult<Neuron>;
    pub fn get_session_behavior(&self, neuron_id) -> AppResult<SessionBehavior>;   // → self.specs
    pub fn update_behavior_for_admin(&self, id, behavior) -> AppResult<Neuron>;    // → self.specs
    pub fn list_session_specs(&self) -> AppResult<Vec<SystemPromptStatus>>;        // → self.specs
}
```

约束：`behavior` 只允许挂在 `system_type IS NOT NULL` 的神经元上（否则回收/候选池隔离失效）；behavior 的写路径统一收敛到 `SessionSpecManager`（`update_content_for_admin` 不触碰 behavior，避免双写）；对外单门面 `NeuronManager`，规格命令与神经元命令统一经其转发。

### 执行引擎（新建 core/call_service.rs）

```rust
/// 模型调用抽象：生产用 ProviderRegistry，测试注入替身。
#[async_trait]
pub trait ModelCaller: Send + Sync {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse>;
}

/// 会话级运行态；存于 conversation.extra.session.state。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    #[serde(default)]
    pub last_selected_neuron_id: Option<String>,
    #[serde(default)]
    pub last_intervention_at: Option<u128>,
    #[serde(default)]
    pub intervention_neuron_ids: Vec<String>,
}

pub struct NeuronCallService {
    model_caller: Arc<dyn ModelCaller>,   // ProviderRegistry 实现；测试注入替身
    neuron_manager: Arc<NeuronManager>,   // 读规格（get_session_behavior）与业务神经元
    store: ConversationStore,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    // 不持有 topic_store；课题相关副作用由调用方（AssistantMode）负责。
}

impl NeuronCallService {
    pub fn new(
        model_caller: Arc<dyn ModelCaller>,
        neuron_manager: Arc<NeuronManager>,
        store: ConversationStore,
        tool_registry: Arc<RwLock<ToolRegistry>>,
    ) -> Self;

    /// 基于规格神经元发起新会话：创建 Conversation，写入
    /// conversation.extra.session = { spec_neuron_id, state: {} }。
    pub async fn open_session(
        &self,
        spec_neuron_id: &str,
        mode: ConversationMode,
    ) -> AppResult<Conversation>;

    /// 一轮完整执行（新命令入口）：读会话绑定规格 → resolve → execute。
    pub async fn converse(
        &self,
        session_id: &str,
        input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse>;

    /// 阶段一：resolve 规格 → role 解析/选型（含单候选短路）→ 工具授权，写入 ctx。
    /// 替代 SelectNeuronBeforeHook + authorize_tools。
    pub async fn resolve_round(&self, ctx: &mut AssistantRoundContext) -> AppResult<()>;

    /// 阶段二：装配 + call_model + 单次工具执行 + 落库 + 按 selection 内 switching 写回会话态。
    /// 替代 run_core 主体（保留现有落库与 ChatResponse 语义）。
    pub async fn execute_round(
        &self,
        ctx: &mut AssistantRoundContext,
        model: &ChatModelSelection,
    ) -> AppResult<ChatResponse>;
}
```

### `resolve_round` 内部流程（含短路与切换）

```text
1. spec_neuron = neuron_manager.get(会话绑定的 spec_neuron_id)；behavior = neuron_manager.get_session_behavior(spec_neuron.id)
2. role_system = spec_neuron.content（候选）
3. match behavior.selection:
   - None        → role = spec_neuron（自身）；ctx.selected_neuron = None
   - Fixed{id}   → role = get(id)；state.last_selected = Some(id)（蕴含永不切换）
   - Global{.., switching} / Neighborhood{.., switching} →
       if switching == Fixed && state.last_selected 已存在:
           直接复用 last_selected，不装配候选池、不调选型模型
       else:
           scope = 依 last_selected 取 neighborhood_default / global_default
           candidates = select_assistant_candidates(scope)
           if candidates.len() == 1:
               role = candidates[0]; mark_used(role.id)   // 硬规则短路：n=1 不调选型模型
           else:
               role = select_one_from_with_history(&candidates, &ctx.messages)
           state.last_selected = Some(role.id)
4. ctx.system_prompt = role.content；ctx.selected_neuron = Some(role)
5. match behavior.tools:
   - None         → ctx.authorized_tool_ids = []
   - FromNeuron   → filter_authorized_tool_ids(registry, role.tool_ids)
   - Allowlist(v) → filter_authorized_tool_ids(registry, v)
6. state 按 selection 内 switching 决定是否保留 last_selected：
   - 非动态（None / Fixed）→ 不涉及切换
   - Fixed / Reelect → 保留（Reelect 下一轮重选；Fixed 下一轮复用）
   - Conditional     → 若 ctx.switched_session（match_topic 已切）→ 清空 last_selected；否则保留
7. 写回 conversation.extra.session.state
```

### `execute_round` 内部流程（沿用 run_core 现有语义）

```text
1. user_input / nudge 落库（逻辑与现 run_core 一致）
2. 拼接组合矩阵（template 与内容有无是正交两维；template 由内容类型推导，不进配置）：
   role_system = role.content          // 角色 → System 首条（replace_system，空则丢弃）
   insert = InsertCatalog::require(behavior.insert_id)（insert_id 有值时）

   | role.content | insert | template | User 消息体                          | 语义             |
   |--------------|--------|----------|--------------------------------------|------------------|
   | 有           | 无     | Neuron   | 【神经元】+ 本轮输入                 | 助手 run_core 现状 |
   | 有           | 有     | Manual   | 【操作说明书】+ 契约段 + 待处理输入  | 规格「角色+契约」  |
   | 无           | 无     | Neuron   | 【神经元】+ 本轮输入（System 空丢弃）| 空规格，可达但低价值 |
   | 无           | 有     | Manual   | 【操作说明书】+ 契约段 + 待处理输入  | hooks 现状        |
   禁止组合：insert 有 + Neuron（契约被渲染为「角色与能力」，框架错位）；
             insert 无 + Manual（说明书框架无正文，无意义）。
   messages = ModelCallInput::assemble(ctx.messages, role_system, insert_or_empty, user_input, template)
   // 位置：角色 → System 首条；insert 正文（有则）与 user_input → User 消息体（契约段 + 输入段）
3. tools = definitions_for(ctx.authorized_tool_ids)（为空则 None）
4. providers.call_model(ModelCallRequest { ... })
5. 单次工具执行与落库（tool_msg / result_msg / assistant_msg），授权校验沿用现状
6. 返回 ChatResponse
```

### 助手接入（assistant_mode.rs）

- `SelectNeuronBeforeHook` 删除，`process_step_request` 中选型阶段替换为 `service.resolve_round(&mut ctx)`；`authorize_tools` 逻辑并入 resolve_round。
- `run_core` 主体替换为 `service.execute_round(&mut ctx, model)`；`persist_selected_neuron`（写 topic）替换为 service 内会话态写回。
- 会话态读取：`read_assistant_state` 的 `last_selected_neuron_id` 改读 `conversation.extra.session.state`（缺失时回退读旧 `topic.extra.assistant` 一次，写入走新位置）；`poll_count` 与干预窗口字段仍读 `topic.extra.assistant`。
- 触发链保持：score → match_topic → **resolve_round** → execute_round → complete_scope；`ctx.switched_session` 语义沿用。

### 内建规格

| system_type | content | behavior | 对应 |
|---|---|---|---|
| `session.assistant_dialogue` | 空占位（动态选型时 role 来自业务神经元） | `{selection: Neighborhood(default, switching: Reelect), tools: FromNeuron}` | 助手主对话 |
| `session.chat`（预留） | 空 | `{selection: None, tools: None}` | Engine Chat（Phase 2 接入） |
| `session.agent`（预留） | 空 | `{selection: None, tools: Allowlist(all)}` | Engine Agent（Phase 2 接入） |

注册方式：`NeuronManager::ensure_session_neuron`（内部经 `specs` 写 behavior）懒创建（bootstrap 时调用），缺失即建、存在不覆盖 behavior。

### Gateway / lib.rs 命令

```rust
#[tauri::command]
pub async fn open_session(
    app: AppHandle,
    spec_neuron_id: String,
    mode: Option<ConversationMode>,   // 默认 Assistant
) -> AppResult<Conversation>;

#[tauri::command]
pub async fn converse_session(
    app: AppHandle,
    session_id: String,
    input: String,
    model: ChatModelSelection,
) -> AppResult<ChatResponse>;

#[tauri::command]
pub async fn list_session_specs(app: AppHandle) -> AppResult<Vec<SystemPromptStatus>>;
// 列出 system_type LIKE 'session.%' 的神经元 + behavior 摘要（供前端「管理好后发起会话」）
```

## 待确认（阻塞实现前拍板；默认按括号执行）

1. **`SessionState` 落点**：`conversation.extra.session.state`（默认）。备选：单独 `session_states` 表（更重，本期不做）。
2. **助手 hooks 是否本期迁移**：否（默认）。`call_system_prompt_json` 保持现状；`run_core` 先行验证 service。若验收希望 hooks 也规格化，则并入 Phase 1。
3. **`Conditional` 触发源**：`match_topic` 发生 switch/创建时清空 `last_selected`（默认，规则版）；LLM 判定版 Phase 2。
4. **规格神经元是否参与业务选型/回收**：不参与（默认，依赖 `system_type IS NOT NULL` 既有隔离）。
5. **n=1 短路**：不可配置硬规则（默认），落在 service 内部 `resolve_round`（`candidates.len() == 1` 直接使用，不调选型模型），`SessionBehavior` 无开关参数；不改 `select_one_from_with_history` 签名。若 hooks 保留期也想收益，可在该函数内加一行 guard（可选项，需单独同意）。
6. **`converse` 的 role 与落库语义**：与助手 run_core 完全一致（默认）。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是——管理面（规格神经元）+ 执行面（service）+ 运行态（会话级）三层齐备。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：方案 D（分层）已获用户确认出方案文档；本 spec 固化其契约与分阶段边界；按评审意见收敛行为模型（switch 折叠进 selection、删除模板/短路参数）。
- 当前核心目标：行为收敛到 `NeuronCallService`，规格 = 带 behavior 的系统神经元；助手 run_core 作为第一个调用方。
- 当前进度：Phase 1 实现完成（models/neuron_store 行为列迁移 → SessionSpecManager → NeuronCallService → assistant_mode 收敛 → gateway 命令 → 测试全绿）。
- 下一步 1（已完成）：models/neuron_store 行为列迁移 + `SessionBehavior` 序列化（`SelectionPolicy`（含 `Switching`）/ `ToolPolicy` / `insert_id`）。
- 下一步 2（已完成）：`NeuronCallService`（resolve/execute）+ 会话态读写；`resolve_round` 内实现 n=1 硬规则短路。
- 下一步 3（已完成）：助手 run_core 收敛 + gateway 命令 + 测试（8 项新增全通过）。
- 涉及文件 / 模块：`models.rs`、`neuron_store.rs`、`neuron_manager.rs`、新建 `spec_manager.rs`、`assistant_mode.rs`、新建 `call_service.rs`、`gateway.rs` / `lib.rs`、`compactor.rs`（测试构造补 `extra` 字段）。
- 风险：会话态双写（topic↔conversation）不一致；`Conversation.extra` 旧文件兼容；助手行为回归（run_core 收敛）。均已通过既有 158 项测试验证。
- 验证方式：单元测试、`cargo fmt --all`、`cargo check --lib`、`cargo test --lib`（158 passed）。
- Execution Approval: 已批准并执行完成。

## Change Log

- 2026-08-09：初版方案文档。基于方案 D 分层设计，固化规格神经元 + NeuronCallService + 会话级 SessionState 契约；划定 Phase 1/2 边界（hooks、Engine 迁移留待 Phase 2）。
- 2026-08-09（修订）：按评审意见收敛 `SessionBehavior`：`SwitchPolicy` 折叠为 `SelectionPolicy` 内 `Switching`（仅 Global/Neighborhood 携带，None/Fixed 蕴含永不切换，消除死配置与命名撞车）；删除 `template` 枚举字段（拼接以 template × 内容有无的 2×2 矩阵为完备约束，标注禁止组合；template 由内容类型推导，不进配置）；保留 `insert_id`（契约正文来源）；删除 `single_candidate_shortcut` 参数（n=1 跳过选型模型改为不可配置硬规则）。
- 2026-08-09（修订 2）：规格管理抽离为 `SessionSpecManager` 子组件（新建 `core/spec_manager.rs`）：作为 `NeuronManager` 内部持有的组合字段（Rust 组合，类比子类），仅共享 `store`、不反向依赖 NeuronManager；`ensure_session_neuron` 由 NeuronManager 编排（`ensure_system_neuron` 本体创建 + specs 写 behavior）；`get_session_behavior` / `update_behavior_for_admin` / `list_session_specs` 为 NeuronManager 薄转发；`NeuronCallService` 不再单独持有 spec 依赖，经 `NeuronManager` 读规格（对外单门面）；behavior 写路径统一收敛到 `SessionSpecManager`（`update_content_for_admin` 不触碰 behavior）。
- 2026-08-09（实现反写）：Phase 1 落地。实际实现偏差：①`NeuronCallService` 以 `Arc<dyn ModelCaller>` trait 抽象持有模型调用（`ProviderRegistry` 实现，测试注入替身），替代 spec 初稿的 `providers: ProviderRegistry`；②`update_behavior_for_admin` 按「只写 behavior、不触碰 content」落地；③`RoundTrigger` / `AssistantRoundContext` 定义随收敛迁移至 `call_service.rs`（`spec_neuron_id` / `behavior` 字段由 resolve_round 填充）；④`Conversation.extra` 追加连带 `compactor.rs` 测试构造补字段；⑤测试覆盖：call_service 新增 8 项（behavior 序列化、工具过滤、会话态往返、n=1 短路、Fixed/Reelect、工具三策略、converse 端到端），全量 158 项通过。
- 2026-08-09（Phase 2 对齐反写）：Phase 2 方案确认「系统神经元一元化」语义重构，本文档相关定义（`SelectionPolicy` 语义、`Switching` / `Conditional` / `switched_session`、`session.assistant_dialogue` 默认行为、规格管理命令形态）以头部警示块与 [Phase 2 spec](2026-08-09_16-40_neuron-call-service-phase2.md) 为准。

## Validation

- Self-check：已审阅既有 spec 格式与相关调用点（run_core / call_system_prompt_json / SelectNeuronBeforeHook / ensure_system_neuron / conversation 存储）；接口契约与实际代码签名对齐（`ModelCaller` trait、`update_behavior_for_admin` 只写 behavior 已反写）。
- Static checks：`cargo fmt --all`、`cargo check --lib` 通过（无 error / warning）。
- Runtime / Test：`cargo test --lib` 全量 158 passed（含 call_service 新增 8 项）。
- Human confirmation：待用户验收实现结果。
- 结果汇总：Phase 1 已实现并通过验证。
- 核心目标是否已由证据证明完成：是（管理面 + 执行面 + 运行态三层落地，助手 run_core 已收敛，测试全绿）。
- 若未完成，当前剩余差距：Phase 2（hooks / Engine 迁移、Conditional LLM 判定、config.json 策略、前端规格管理界面）。
- 剩余风险：见 Checkpoint Summary「风险」。

## Resume / Handoff

- 当前状态：Phase 1 实现完成，测试全绿，spec 已反写。
- 当前卡点：无。
- 下一步唯一动作：用户验收；如需可继续 Phase 2（hooks / Engine 迁移）。
- 下一轮核心目标：run_core 收敛到 service 且助手行为零回退（已验证）；单候选短路全局生效（已验证）。
