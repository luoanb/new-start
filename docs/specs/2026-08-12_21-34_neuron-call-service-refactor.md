# NeuronCallService 重构方案：收敛为无状态单轮对话引擎

> 需求来源：[docs/pulsar/NeuronCallService.md](../pulsar/NeuronCallService.md)
> 状态：接口与方案 2 落地设计均已确认，待实现

## 1. 背景与目标

需求文档要求：

1. NeuronCallService 需要调整；聊天主链路与后续其他业务调用都要切换到 NeuronCallService。
2. 所有神经元都可以发起会话（NeuronCallService 提供默认行为：领域）。
3. 不传神经元也能发起会话（全域神经元首轮找到《神经元》，后续按领域推进）。
4. 系统神经元特例：自主选型限「无 / 固定 / 领域」，**不会走全域**；系统神经元服务于项目指定场景的提示词。

现状问题（[call\_service.rs](../../packages/pulsar-app/src-tauri/src/core/call_service.rs)）：

* 对外暴露阶段 API `build_context` / `resolve_round` / `execute_round`，上层（Gateway / AssistantMode）按模式拼装，service 沦为工具盒。

* `open_session` 强绑定「系统神经元 + behavior 非空」，普通神经元无法发起会话，也没有「不传神经元」路径。

* Chat 模式走 `execute_round` 退化形态（无 resolve，不选神经元）。

**目标**：NeuronCallService 收敛为**无状态单轮对话引擎**——只负责「一轮对话」的计算，不读会话、不写会话、不落消息；轮询、多轮循环、hooks 编排全部留在上层。service 对业务信息零感知（无触发语义、无简报、无会话）。

## 2. 设计决策（已与用户对齐）

| #   | 决策                                                                                                    |
| --- | ----------------------------------------------------------------------------------------------------- |
| D1  | NeuronCallService 只负责单轮对话；轮询 / Agent 多轮循环 / Assistant hooks 编排均在外部                                    |
| D2  | 无 `session_id`：输入全部显式传入（`RoundInput`），不依赖会话存储，无状态纯计算                                                  |
| D3  | 对外**唯一公开方法** `converse`；`start_session` 归 Gateway；`call_system_prompt` **消除**（裁决语义 = converse 的一种调用形态，收敛到 converse） |
| D4  | `resolve_round` / `execute_round` / `build_context` 移出公共面（逻辑并入 `converse` 内部）                         |
| D5  | 种子语义：`Global`（全域首轮 → 领域推进）/ `Neuron(id)`（系统神经元用 behavior / 普通神经元推导默认领域）                               |
| D6  | 系统神经元 `selection` 禁 `Global`（管理面校验 + 旧数据宽容回退 Neighborhood）                                            |
| D7  | 上层接入选型：**方案 2**——`ConversationRunner` 统一编排 + `RoundHooks` 注入；业务不进 Gateway，每个业务新开独立文件                  |
| D8  | 输入侧消息（user / nudge）落库由 **Runner 统一**（`InputRecord` 声明），业务文件零重复                                        |
| D9  | topic\_brief（课题简报）**不进 converse**：业务层拼进 `model_input` 文本，service 不感知任何业务上下文                           |
| D10 | `assistant_mode.rs` **全删迁入** `assistant_session.rs`（含 register\_polling / process\_step\_request 调度壳） |

## 3. 接口设计

### 3.1 核心类型

```rust
/// 约定直连系统类型：seed = None 时映射到它（内置 behavior：selection None / tools 由 override /
/// insert_id None），统一走 SystemType behavior 解析路径，不设特殊分支。
pub const SYSTEM_TYPE_DIRECT: &str = "session_direct";

/// 会话种子：定义「用什么发起会话」（存于 conversation.extra.session，由上层维护）
/// None = 直连（映射 `SYSTEM_TYPE_DIRECT`：无选型，Neuron 模板，role_system 空）——Chat/Agent 现状行为。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum SessionSeed {
    /// 不传神经元：首轮全域池选 1 → 写 state.last_selected；后续轮按领域推进
    Global,
    /// 任意神经元：系统神经元用其 behavior（禁 Global）；普通神经元推导默认行为（领域，锚点=它自身）
    Neuron(String),
}

/// 单轮输入（纯计算上下文，无任何业务触发语义）
pub struct RoundInput {
    pub seed: Option<SessionSeed>,    // None = 直连（映射 SYSTEM_TYPE_DIRECT）；Some = 种子分派
    pub state: SessionState,          // 运行态（last_selected 等），上层传入
    pub messages: Vec<ModelMessage>,  // 历史消息（模型侧，sanitize 后）
    pub tool_override: Option<Vec<String>>,  // 授权覆盖（None → 按 seed/behavior 推导；Agent 传全部工具）
}

/// 单轮产物（模型侧，不含业务输入消息；user/nudge 落库由上层自理）
pub struct RoundOutcome {
    pub response: String,             // 最终文本
    pub model_output: Option<String>,
    pub tool_call: Option<ToolCallInfo>,    // 本轮工具调用（含参数，上层落 tool_call 消息用）
    pub tool_result: Option<String>,
    pub selected_neuron_id: Option<String>,
    pub state: SessionState,          // 本轮后的新运行态，上层写回
}
```

`SessionState`（现定义于 call\_service.rs）保留不变。
说明：converse 不感知 UserInput / ManualStep / Poller 等触发方式；输入侧业务消息（user / nudge）落库由上层自行决定；`model_input` 即最终模型输入文本，简报/继续指令等由业务层拼好传入。

### 3.2 NeuronCallService 唯一公开方法

```rust
pub struct NeuronCallService {
    // 内部依赖：NeuronManager（领域查询/选型/行为）、ModelCaller（模型调用）、ToolRegistry（工具）
}

impl NeuronCallService {
    /// 单轮对话：种子分派选神经元 → 工具授权 → 组装 → 模型调用 → 单次工具执行。
    /// 输入全部显式给出；不读会话、不写会话、不落消息；返回模型侧产物 + 新状态。
    /// model_input 为最终输入文本（业务已拼好简报/继续指令），service 不解释语义。
    pub async fn converse(
        &self,
        input: RoundInput,
        model_input: &str,
        model: &ChatModelSelection,
    ) -> AppResult<RoundOutcome>;
}
```

### 3.3 内部种子分派（私有，原 resolve 逻辑并入）

```rust
// 私有，converse 内调用；seed = None 时映射 SYSTEM_TYPE_DIRECT（内置直连 behavior：
// selection None / tools 由 override / insert_id None），与其他 SystemType 同路径解析
fn resolve_role(&self, seed: Option<&SessionSeed>, state, messages) -> AppResult<ResolvedRole> {
    match seed {
        None => behavior = 直连（SYSTEM_TYPE_DIRECT，selection None）
            // role_system = ""（不读 content）；工具 = tool_override 或空；Neuron 模板直连
        Some(SessionSeed::Global) =>
            // 有 last_selected → 邻域选（锚点 = last_selected）
            // 无（首轮）→ 全域池选 1（Global limit）
            // 均写 state.last_selected
        Some(SessionSeed::Neuron(id)) => match self.neuron_manager.get_session_behavior(id) {
            Some(behavior) =>       // 系统神经元
                match behavior.selection {
                    None          => role_system = ""（不读 content；有 insert_id 仍拼契约段）
                    Fixed         => role = 自身 content（不写 last_selected）
                    Neighborhood   => 邻域选（首轮锚点 = id / 后续锚点 = last_selected）
                    Global{..}    => 宽容回退 Neighborhood（禁 Global 兜底）
                }
            None =>                  // 普通神经元：推导默认行为
                // { selection: Neighborhood(锚点 = id), tools: FromNeuron, insert_id: None }
                // 首轮锚点 = id；后续锚点 = last_selected
        }
    }
}
```

* 工具授权：`behavior.tools`（None / FromNeuron / Allowlist）∩ 注册表；`seed = None` 直连路径无 behavior，工具 = `tool_override` 或空；普通神经元推导默认 `FromNeuron`（本轮 role 神经元的 `tool_ids ∩ 注册表`）。

* 组装沿用 `ModelCallInput::assemble` + `InsertCatalog`（`insert_id` 有无决定 Manual / Neuron 拼接）。

* 单候选短路（候选池仅 1 个 → 跳过选型模型）为既有硬规则，保留。

## 4. 上层接入（方案 2：ConversationRunner + 业务独立文件）

### 4.1 文件划分

| 文件                                                                                      | 职责                                                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [call\_service.rs](../../packages/pulsar-app/src-tauri/src/core/call_service.rs)（改）     | 只留无状态 `converse` + `SessionState` / `RoundInput` / `RoundOutcome` / `ModelCaller` / `message_to_model` / `read·write_session_state`；删 `open_session` / `resolve_round` / `execute_round` / `build_context` / `call_system_prompt` / `RoundTrigger` / `AssistantRoundContext` |
| **conversation\_runner.rs**（新）                                                          | `ConversationRunner` + `RoundHooks` trait + `RoundContext`：统一「读会话 → before hooks → converse → after hooks → 落库」，业务无关                                                                                                                                                         |
| **chat\_session.rs**（新）                                                                 | Chat 接入：无 hooks，`run_round(InputRecord::User, None)` 直调                                                                                                                                                                                                                      |
| **agent\_session.rs**（新）                                                                | Agent 接入：`agent_loop` 迁入（全工具 `tool_override` + `AGENT_MAX_ITERATIONS` 护栏），首轮 User / 后续 None + 拼继续指令                                                                                                                                                                          |
| **assistant\_session.rs**（新）                                                            | Assistant 接入：`AssistantHooks`（score\_feedback → match\_topic → complete\_scope → 干预标记）+ converse / step / step\_poller 编排 + register\_polling / process\_step\_request 调度壳                                                                                                   |
| **poller\_step.rs**（新）                                                                  | Poller step 接入：简报构造 + `InputRecord::Nudge` + 复用 assistant step                                                                                                                                                                                                               |
| [gateway.rs](../../packages/pulsar-app/src-tauri/src/core/gateway.rs)（改）                | 只留 `start_session`（建会话 + 种子校验 + 写 `extra.session`）+ 命令薄壳 + 装配 + 按 mode 委托上述 session                                                                                                                                                                                          |
| [assistant\_mode.rs](../../packages/pulsar-app/src-tauri/src/core/assistant_mode.rs)（删） | 业务全部迁入 assistant\_session.rs 后删除                                                                                                                                                                                                                                             |

### 4.2 Runner / Hooks 契约

```rust
/// 一轮对话的上下文（runner 组装，before/after hooks 共享；不承载任何服务依赖）
pub struct RoundContext {
    pub session_id: String,
    pub mode: ConversationMode,
    pub seed: Option<SessionSeed>,        // 读自 extra.session
    pub state: SessionState,              // 本轮运行态（before 可改；after 读 outcome.state）
    pub messages: Vec<ModelMessage>,      // 模型侧历史（runner 读会话构造）
    pub model_input: String,              // 触发输入（before 可改：拼简报 / 继续指令）
    pub tool_override: Option<Vec<String>>,
    pub outcome: Option<RoundOutcome>,    // converse 后填充（after 读）
}

#[async_trait]
pub trait RoundHooks: Send + Sync {
    async fn before_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
    async fn after_round(&self, ctx: &mut RoundContext) -> AppResult<()>;
}

/// 本轮输入侧落库声明（runner 统一落库，业务文件零重复）
pub enum InputRecord {
    User(String),   // 落 user 消息
    Nudge,          // 落 nudge（系统推进）
    None,           // 不落（ManualStep / Agent 后续轮）
}

impl ConversationRunner {
    /// 统一编排：读会话 → before hooks → converse → after hooks → 落库 → ChatResponse
    pub async fn run_round(
        &self,
        session_id: &str,
        input: InputRecord,
        model: &ChatModelSelection,
        hooks: Option<&dyn RoundHooks>,   // Chat 传 None
    ) -> AppResult<ChatResponse> {
        // 1 读会话 → RoundContext（seed 读 extra.session / state 读 extra.session.state /
        //   messages 由 history 转换）
        // 2 hooks?.before_round(&mut ctx)?          // 业务副作用；可改 model_input / tool_override
        // 3 outcome = call_service.converse(
        //       RoundInput { seed, state, messages, tool_override }, &ctx.model_input, model)?
        // 4 ctx.outcome = Some(outcome.clone())
        // 5 hooks?.after_round(&mut ctx)?           // 读 outcome 做副作用（complete_scope / 干预标记）
        // 6 落库：input（user / nudge）→ outcome 产物（assistant / tool_call / tool_result）→ outcome.state
        // 7 Ok(ChatResponse { conversation_id, response: outcome.response })
    }
}
```

### 4.3 业务文件接入示意

```rust
// chat_session.rs —— 无 hooks 直连
pub async fn send(&self, session_id: &str, input: &str, model: &ChatModelSelection)
    -> AppResult<ChatResponse> {
    self.runner.run_round(session_id, InputRecord::User(input.to_string()), model, None).await
}

// agent_session.rs —— 多轮循环（护栏 + 全工具）
pub async fn agent_loop(&self, session_id: &str, input: &str, model: &ChatModelSelection)
    -> AppResult<ChatResponse> {
    // authorized = 注册表全部工具；循环 run_round：
    //   首轮 InputRecord::User(input)，后续 InputRecord::None + model_input = "Continue ..."
    //   tool_override = Some(authorized)；iterations > AGENT_MAX_ITERATIONS → Err(AgentMaxIterations)
    //   收敛判据：本轮无 tool_result
}

// assistant_session.rs —— hooks 注入
pub async fn converse(&self, session_id: &str, user_input: &str, model: &ChatModelSelection)
    -> AppResult<ChatResponse> {
    self.runner.run_round(session_id, InputRecord::User(user_input.to_string()),
        model, Some(&self.hooks)).await
}

// poller_step.rs —— nudge + 简报
pub async fn step_poller(&self, session_id: &str, model: &ChatModelSelection)
    -> AppResult<ChatResponse> {
    self.runner.run_round(session_id, InputRecord::Nudge, model, Some(&self.hooks)).await
    // hooks.before_round 内拼 topic_brief 进 model_input（不进 converse）
}
```

## 5. 迁移矩阵

| 现状                                                                                                    | 去处                                                                                                    |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `NeuronCallService::open_session`                                                                     | `Gateway::start_session(seed, mode)`：建会话 + 校验种子 + 写 `extra.session`（种子元数据）                            |
| `NeuronCallService::resolve_round`                                                                    | 删除：选型逻辑并入 `converse` 内部（种子分派）                                                                         |
| `NeuronCallService::execute_round`                                                                    | 删除：执行逻辑并入 `converse` 内部；输入分支（user / nudge / 简报）外移到 runner + 业务文件                                      |
| `NeuronCallService::build_context`                                                                    | 删除：历史读取与上下文构造移到 `ConversationRunner`                                                                  |
| `NeuronCallService::call_system_prompt`                                                               | 消除：裁决调用 = `assistant_session::call_judgement`（`ensure_system_neuron` + `converse`（系统类型 seed + 禁工具 + JSON 解析））；模型调用统一收敛到 `converse`，NeuronManager 回归纯管理面 |
| `Gateway::agent_loop`                                                                                 | 迁往 `agent_session.rs`（护栏 `AGENT_MAX_ITERATIONS` 随迁）                                                   |
| `Gateway::send_model_message` 路由                                                                      | 按 mode 委托各 session 文件（Chat → chat\_session / Agent → agent\_session / Assistant → assistant\_session） |
| `Gateway::open_session` / `converse_session`                                                          | `open_session` → `start_session(seed, mode)`；`converse_session` 删除（Chat/Assistant 走 session 文件）       |
| `AssistantMode::converse` / `step` / `step_poller`                                                    | 迁往 `assistant_session.rs`（hooks 注入 runner）                                                            |
| `AssistantMode::register_polling` / `process_step_request` / `score_feedback` / `intervention_window` | 迁往 `assistant_session.rs`，`assistant_mode.rs` 删除                                                      |
| `AssistantRoundContext` / `RoundTrigger` / `BeforeHook` / `AfterHook`                                 | 删除：由通用 `RoundContext` / `RoundHooks` / `InputRecord` 取代                                               |
| `execute_round` 内 topic\_brief / 继续指令注入                                                               | 业务层拼进 `model_input`（不进 converse）                                                                      |
| Tauri 命令 `send_chat_message` / `converse_session` / `open_session`                                    | 语义不变：命令层（Gateway）内部按 mode 委托 session 文件                                                               |

## 6. 测试计划

* **单轮语义**：`seed=None` 直连（无选型、role\_system 空、Neuron 模板）；`Global` 首轮全域选 1 并写 last\_selected / 次轮按领域推进；`Neuron(普通)` 默认领域（锚点=自身）且工具 `FromNeuron`；`Neuron(系统)` None / Fixed / Neighborhood 三态；系统神经元旧数据 `Global` 宽容回退 Neighborhood；单候选短路。

* **输出完整性**：`RoundOutcome` 覆盖 assistant / tool\_call / tool\_result 场景；`state` 反映 last\_selected 变化。

* **无状态验证**：converse 不接触 ConversationStore（构造层只注入 NeuronManager / ModelCaller / ToolRegistry），不感知 UserInput/ManualStep/Poller，无 topic\_brief。

* **Runner 编排**：读会话构上下文 → before hooks（可改 model\_input）→ converse → after hooks（读 outcome）→ InputRecord 落库顺序（user → 产物 → state）；Chat 传 None hooks 直连。

* **行为回归**：topic\_brief 从 system 侧改为 model\_input（用户侧）后，Assistant 轮询推进质量与既有系统神经元会话行为需回归验证；Agent 循环终止与护栏、Poller 调度不变。

* **迁移回归**：既有系统神经元会话（assistant\_dialogue 等）行为不变；前端发起会话/对话流程回归。

## 7. 范围外 / 待确认

* **seed 直连路径（已确认）**：`RoundInput.seed: Option<SessionSeed>`，None 映射约定系统类型 `SYSTEM_TYPE_DIRECT`（`session_direct`，内置 behavior selection None），统一走 SystemType 解析，不设特殊分支。

* **跨会话领域延续**（需求「下一次对话的选型 = 上一次对话的领域」）：属会话管理，在 `Gateway::start_session` 层实现（读最近会话 last\_selected 作种子锚点），NeuronCallService 不感知。本期默认**不做**，列为后续增强。

* **`call_system_prompt`** **归属**：已消除（实现期调整）。裁决语义与 `converse` 高度同构（选型/拼装/调模型同一套），非独立能力；懒创建系统神经元才是管理面操作，留在 `NeuronManager::ensure_system_neuron`。`assistant_session` 私有 `call_judgement` 用 `converse`（`seed=Neuron(spec)` + `tool_override=Some(vec![])` 禁工具）表达，模型调用统一收敛到 `converse` 唯一公共入口。

* `ConversationMode`（Chat/Agent/Assistant）：保留，仅作会话展示与上层路由标记，不进 service 执行逻辑。

## 8. Checkpoint

* [x] 接口契约（3.1 / 3.2 / 3.3）确认（含 seed None → SYSTEM\_TYPE\_DIRECT）

* [x] 上层接入选型（方案 2 + 业务独立文件）确认（4.1 / 4.2 / 4.3）

* [x] 落库归属（Runner 统一 InputRecord）、简报位置（业务拼 model\_input）、assistant\_mode 去留（全删）确认

* [x] 迁移矩阵（第 5 节）确认

* [x] 测试计划（第 6 节）确认

* [x] 进入实现

* [x] **实现完成**：`NeuronCallService` 收敛为无状态 `converse(RoundInput, model_input, model) -> RoundOutcome`（种子分派 + 工具授权 ∩ 注册表 + 单次工具执行）；`start_session` 归 `Gateway`；`call_system_prompt` 迁 `NeuronManager`（注入 `Arc<dyn ModelCaller>`）；业务独立文件 `chat_session.rs` / `agent_session.rs` / `assistant_session.rs` / `poller_step.rs` + `ConversationRunner` + `RoundHooks`；`assistant_mode.rs` 删除；Tauri 命令层适配（`open_session` 种子推导 / 删 `converse_session`）。

* [x] **验证状态**：`cargo check --all-targets` 零 error / 零 warning；`cargo test` 169 全绿（含单轮语义 / 无状态 / Runner 编排 / 工具授权 / Agent 护栏 / 迁移回归）。

* [ ] **待人工回归**：既有发起神经元会话行为、前端发起会话/对话流程、Poller 轮询推进在真实运行时验证。


## 9. Change Log

| 日期         | 变更                                                                                                                       |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| 2026-08-12 | 初稿：收敛为无状态单轮对话引擎，接口与迁移矩阵定稿                                                                                                |
| 2026-08-12 | 定稿方案 2：ConversationRunner + 业务独立文件（4.1-4.3）；落库/简报/assistant\_mode 三决策落地；seed None 映射约定 SystemType `session_direct`（接口定稿） |
| 2026-08-15 | 移除内建会话规格 `session.assistant_dialogue`（僵尸占位，Global 由 `resolve_role` 内联构造 behavior，从未消费）；术语统一——`session.%` 系统神经元称「系统神经元」、`spec_neuron_id` 锚点称「发起神经元」；代码注释/前端注释/活跃文档清除「规格」描述词（代码标识符不重命名）。详见 `docs/micro_specs/2026-08-15_remove-assistant-dialogue-and-terminology.md` |

## 10. Validation / Resume

* 验收：`cargo test -p pulsar-app`（src-tauri）全绿；聊天主链路（Chat/Agent/Assistant）与轮询回归。

* 断点续做：从第 8 节 Checkpoint 逐项推进。

