# Round Pipeline：取消 NeuronCallService，三段管道（Resolver / Assembler / Executor）

> **已取代（2026-08-16）**：三段管道 v1 被 [2026-08-16_18-00_round-resolver-message-truth.md](./2026-08-16_18-00_round-resolver-message-truth.md)（Round Pipeline v2）推翻——删 `ResolvedRound` / `WireRound` 中间层，`Vec<Message>` 为唯一真相源，resolve 收拢选型+拼接，删 B2 冻结状态机。本文保留为决策记录，实现以 v2 spec 与代码为准。
> 状态：设计定稿，待实现
> 需求来源：与用户讨论 NeuronCallService 边界问题（message 生产/消费紧耦合）后的重构方案

## 1. 背景与问题

NeuronCallService 收敛为无状态 `converse` 后仍存在边界问题：**它持有 messages（`RoundInput.messages`），内部又有修改 messages 的需求（追加神经元角色 / 首轮系统提示词 / B2 context），而修改结果（wire）不对外可见**。

现状 message 的生产点与消费点：

| 生产点 | 位置 | 产出 |
|---|---|---|
| 历史投影（sanitize tool 配对 + 滤空 assistant） | `conversation_runner.rs` `to_model_messages` | 模型侧历史 |
| 单条映射（Nudge/RoleContext 回灌） | `call_service.rs` `message_to_model` | `Message` → `ModelMessage` |
| 选型决策 | `call_service.rs` `resolve_role` | `role_system` / 选中神经元 / behavior / 新 state |
| wire 组装（角色追加 / 系统提示词 / B2 context） | `call_service.rs` `converse` 内 `assemble_with_context` | **最终 wire（黑盒）** |

| 消费点 | 位置 | 消费什么 |
|---|---|---|
| 发模型 | `converse` 内 `ModelCallRequest.messages` | wire（内部） |
| 落库 | `conversation_runner.rs` `persist` | 产物 + `role_context_message` 投影 |
| 收敛判据 | `last_message_is_tool_result` | 会话最后一条消息类型 |
| hooks / 审计 | before/after round | 只看 `model_input` 与 `outcome`，看不到 wire |

另有 6 处独立组装点绕过 service 直接调 `ModelCallInput::assemble`：`neuron/selection.rs`（2）、`neuron/model.rs`（1）、`neuron/evolution.rs`（1）、`compactor.rs`（1）、`pulsar-cli.rs`（1）、`tui/app.rs`（1）。

**核心问题**：
1. wire 的产生在 service 内部黑盒，消费（落库/审计）在外层，只能靠 `role_context_message` 投影反推「模型看到了什么」→ 内外双向耦合。
2. 选型决策（选了谁、为什么）不可见，hooks/落库/审计无法消费。
3. 组装知识被复制到 6 个独立组装点，无单一归属。

## 2. 设计目标与原则

1. **消息语义单一归属**：sanitize、角色追加、系统提示词、B2 context 注入只在一处定义，产出显式 wire。
2. **数据即边界**：内外握手通过显式数据契约（`ResolvedRound` / `WireRound`），不再靠黑盒 + 投影。
3. **选型与执行分离**：`resolve`（决策）与 `execute`（模型+工具）解耦，决策可被 hooks/落库/审计消费。
4. **业务零感知**：Chat / Agent / Assistant / Poller 业务方仍只面对 `Runner + hooks`；`assistant_session::call_judgement` 走 runner 原始管道。
5. **组装点收敛**：6 个独立组装点统一走 Assembler。

## 3. 设计决策（已与用户确认）

| # | 决策 |
|---|---|
| D1 | **取消 `NeuronCallService`**：Runner 直接组合 Resolver / Assembler / Executor；黑盒语义不复存在 |
| D2 | **独立 `RoundResolver`**：不并入 NeuronManager（选型与神经元管理解耦，独立组件）；依赖 NeuronManager（领域查询/选型）+ ModelCaller（LLM 选型） |
| D3 | **全部收敛**：主链路 + 6 个独立组装点（selection/model/evolution/compactor/cli/tui）本期统一走 Assembler |
| D4 | wire 一等产物：`WireRound { messages, role_context_message }` 显式产出；`RoundOutcome` 移除 `role_context_message`（由 WireRound 承担）与 `state`（落库状态改读 `ResolvedRound.next_state`；测试断言按新来源调整） |
| D5 | `SYSTEM_TYPE_DIRECT` 无使用处，随 `call_service.rs` 拆除一并删除 |
| D6 | runner 新增原始管道 `run_raw_round`（读库/落库无关，纯三段计算），供 `call_judgement` 等内部非对话调用复用；业务会话仍走 `run_round` |
| D7 | 消息映射 `message_to_model` 迁入 Assembler；会话元数据读写（`session_seed` / `read/write_session_state`）留 `conversation_runner.rs`；`SessionState` / `SessionSeed` / `RoundOutcome` 迁入 `round_types.rs` |

## 4. 新实体与接口

### 4.1 类型（`round_types.rs`，自 `call_service.rs` 迁入）

```rust
/// 会话级运行态（原定义不动）
pub struct SessionState { last_selected_neuron_id, stable_system_prompt, stable_system_frozen }

/// 会话种子（原定义不动）
pub enum SessionSeed { Global, Neuron(String) }

/// ① 本轮身份决策：wire 组装所需的一切，显式可见
pub struct ResolvedRound {
    pub selected_neuron: Option<Neuron>,
    pub role_system: String,            // 首轮系统提示词（含神经元角色）
    pub behavior: Option<SessionBehavior>,
    pub inject_context: Option<String>, // B2 冻结角色 content（稳定冻结后 = selected_neuron.content）
    pub next_state: SessionState,       // 新运行态（上层写回）
}

/// ② 本轮完整模型输入（一等产物）
pub struct WireRound {
    pub messages: Vec<ModelMessage>,        // 完整 wire：发模型 / 审计 / 测试共用
    pub role_context_message: Option<String>, // `[当前角色]\n{ctx}`（assemble 直接产出，runner 落库）
}

/// 单轮产物（移除 role_context_message / state）
pub struct RoundOutcome {
    pub response: String,
    pub model_output: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_results: Vec<ToolResultItem>,
    pub selected_neuron_id: Option<String>,
}
```

### 4.2 RoundResolver（`round_resolver.rs`）

```rust
pub struct RoundResolver { neuron_manager: Arc<NeuronManager>, model_caller: Arc<dyn ModelCaller> }

impl RoundResolver {
    /// 种子分派：None 直连 / Global 全域→领域推进 / Neuron(普通) 默认邻域 / Neuron(系统) 按 behavior
    /// （None 清锚点 / Fixed 读自身 / Neighborhood 锚点规则 / Global 宽容回退 Neighborhood）；
    /// B2 冻结复用、单候选短路等既有硬规则保留。
    pub async fn resolve(
        &self,
        seed: Option<&SessionSeed>,
        state: &SessionState,
        history: &[ModelMessage],
        reselect: bool,
    ) -> AppResult<ResolvedRound>;
}
```

（原 `resolve_role` 整体迁入：`scope_for_selection` / `reuse_selected_neuron` / `neuron_manager.select_role` 调用随迁。）

### 4.3 MessageAssembler（升级 `model_call_input.rs`）

```rust
pub struct MessageAssembler;   // 保留纯静态；底层纯函数（replace_system/append/insert_at/
                              // with_user_input_for_append/sanitize_tool_pairs）保留

impl MessageAssembler {
    /// 统一入口：由 ResolvedRound 推导 template（insert_id Some → Manual / None → Neuron），
    /// 追加角色/首轮系统提示词/B2 context → WireRound。空历史产出 System(role_system) + User(body)
    /// （分开，与落库顺序一致）。
    pub fn assemble(round: &ResolvedRound, history: &[ModelMessage], user_input: &str) -> WireRound;

    /// 独立组装点用：无 seed/state 语义，直接给角色与契约段。
    pub fn assemble_direct(role_system: &str, insert_id: Option<&str>, user_input: &str) -> WireRound;

    /// Message → ModelMessage 投影（原 message_to_model 迁入；Nudge/RoleContext 回灌为
    /// User 文本，落库顺序 = wire 注入顺序，历史 = wire）。
    pub fn from_message(message: &Message) -> Option<ModelMessage>;
}
```

### 4.4 RoundExecutor（`round_executor.rs`）

```rust
pub struct RoundExecutor { model_caller: Arc<dyn ModelCaller>, tool_registry: Arc<RwLock<ToolRegistry>> }

impl RoundExecutor {
    /// 工具授权（override 优先 → behavior.tools 三策略，标签并入，∩ 注册表）→
    /// 模型调用（wire.messages + tools）→ 授权校验 → 单轮全部 tool_calls 执行 → RoundOutcome。
    pub async fn execute(
        &self,
        round: &ResolvedRound,
        wire: &WireRound,
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
    ) -> AppResult<RoundOutcome>;
}
```

### 4.5 ConversationRunner 编排（`conversation_runner.rs`）

```rust
impl ConversationRunner {   // 依赖改为 resolver + assembler(纯函数) + executor
    /// 现有业务入口不变：读会话 → before hooks（可改 model_input/reselect/state/seed）→
    /// resolve → assemble → execute → persist → after hooks
    pub async fn run_round(session_id, input: InputRecord, tool_override, model, hooks) -> AppResult<ChatResponse>;

    /// 原始管道：不读库、不落库，纯三段计算（call_judgement 等内部调用用）
    pub async fn run_raw_round(
        &self,
        seed: Option<SessionSeed>,
        state: SessionState,
        history: Vec<ModelMessage>,
        model_input: String,
        tool_override: Option<Vec<String>>,
        model: &ChatModelSelection,
    ) -> AppResult<(ResolvedRound, WireRound, RoundOutcome)>;
}

/// RoundContext 增加中间产物（hooks 可观测）：
pub struct RoundContext {
    // ...现有字段不动
    pub resolved: Option<ResolvedRound>,   // resolve 后填充（before 无值；after 可读）
    pub wire: Option<WireRound>,           // assemble 后填充
}
```

persist 变化（拆分两段，见 [conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L300-L455)）：
- `persist_input`（发送前）：首轮 System → `wire.role_context_message` → 输入消息（user / nudge / ManualStep·AgentLoop 不落）。wire 组装后即可确定，模型调用失败/超时不丢用户消息。
- `persist_outcome`（发送后）：产物（tool_call/tool_result 或 assistant text）读 `ctx.outcome`；盖章 `selected_neuron_id` 读 `ctx.resolved.selected_neuron`（outcome 侧读 `outcome.selected_neuron_id`）。
- 会话态写回改读 `ctx.resolved.next_state`（替代 `outcome.state`）。

## 5. 消息流

```
Runner.run_round:
  读会话 → 历史 ModelMessage（sanitize）
  → before hooks（可改 model_input / reselect / state / seed）
  → ① resolver.resolve(seed, state, history, reselect) → ResolvedRound
  → ② assembler.assemble(&resolved, history, model_input) → WireRound
  → persist_input（发送前：首轮 System → role_context_message → 输入消息）
  → ③ executor.execute(&resolved, &wire, model, tool_override, tool_tags) → RoundOutcome
  → persist_outcome（发送后：产物 + resolved.next_state）
  → after hooks（读 resolved / wire / outcome）
```

## 6. 迁移矩阵

| 现状 | 去处 |
|---|---|
| `NeuronCallService::converse` | 删除：三段分别由 Resolver / Assembler / Executor 承担 |
| `NeuronCallService::resolve_role`（含 scope_for_selection / reuse_selected_neuron） | `RoundResolver::resolve`（整体迁入） |
| `converse` 内工具授权 + 模型调用 + 工具执行 | `RoundExecutor::execute` |
| `converse` 内 `assemble_with_context` + `role_context_message` 生成 | `MessageAssembler::assemble` → `WireRound` |
| `RoundInput` | 删除：字段分配到 resolve（seed/state/history/reselect）/ assemble（user_input）/ execute（tool_override/tool_tags） |
| `RoundOutcome.role_context_message` / `RoundOutcome.state` | 移除：分别由 `WireRound` / `ResolvedRound.next_state` 承担 |
| `SYSTEM_TYPE_DIRECT` | 删除（无使用处） |
| `message_to_model` | `MessageAssembler::from_message` |
| `session_seed` / `read/write_session_state` | 留在 `conversation_runner.rs` |
| `SessionState` / `SessionSeed` / `RoundOutcome` / `ToolResultItem` | 迁入 `round_types.rs` |
| `ConversationRunner::run_round` | 编排改三段；persist 改读 resolved/wire |
| `ConversationRunner` 新增 `run_raw_round` | `assistant_session::call_judgement` 改用（不落库裁决调用） |
| `assistant_session` 依赖 `Arc<NeuronCallService>` | 改为依赖 runner（或 resolver+executor 注入） |
| `gateway` 装配 `NeuronCallService` | 改为装配 Resolver + Executor，注入 runner / assistant_session |
| 独立组装点 selection(2)/model(1)/evolution(1)/compactor(1)/cli(1)/tui(1) 调 `ModelCallInput::assemble` | 统一走 `MessageAssembler::assemble_direct`（或 assemble） |

## 7. 文件划分

| 文件 | 职责 |
|---|---|
| `round_types.rs`（新） | `SessionState` / `SessionSeed` / `ResolvedRound` / `WireRound` / `RoundOutcome` / `ToolResultItem` |
| `round_resolver.rs`（新） | `RoundResolver` + `resolve`（原 resolve_role 迁入） |
| `round_executor.rs`（新） | `RoundExecutor` + `execute`（工具授权/模型调用/工具执行） |
| `model_call_input.rs`（改） | 升级为 `MessageAssembler`：`assemble` / `assemble_direct` / `from_message` + 底层纯函数保留 |
| `call_service.rs`（删） | 拆除：能力与类型迁出后删除 |
| `conversation_runner.rs`（改） | 三段编排 + `run_raw_round` + persist 改造 + ctx 增 resolved/wire |
| `assistant_session.rs`（改） | `call_judgement` 改用 `run_raw_round`；依赖替换 |
| `gateway.rs`（改） | 装配 Resolver + Executor |
| `neuron/selection.rs` / `neuron/model.rs` / `neuron/evolution.rs` / `compactor.rs` / `pulsar-cli.rs` / `tui/app.rs`（改） | 组装调用收敛到 `MessageAssembler` |

## 8. 测试计划

- **Resolver 单测**：种子分派全分支（直连 / Global 首轮+推进 / 普通神经元默认邻域 / 系统神经元 None/Fixed/Neighborhood / Global 宽容回退 / B2 冻结复用 / 单候选短路）；`next_state` 正确性（原 resolve 相关断言迁移）。
- **Assembler 单测**：wire 组装（角色追加 / 首轮空历史 System+User 分开 / B2 context 注入 / 模板推导）；`role_context_message` 产出（首轮 None、次轮 Some）；`from_message` 回灌（Nudge/RoleContext → User 文本）；sanitize 保留（原 model_call_input 测试）。
- **Executor 单测**：工具授权（override/三策略/标签并入/∩注册表）、未授权工具拒绝、多 tool_calls 全执行、响应拼接（原 execute 相关断言迁移）。
- **Runner 集成**：run_round 三段编排 + hooks（before 改 model_input 生效）+ persist（输入→RoleContext→产物→next_state 落库）；run_raw_round 不落库。
- **业务回归**：`call_judgement` JSON 裁决（`run_raw_round` 形态）；Chat/Agent/Assistant/Poller 既有行为不变（业务文件零感知验证）。
- **验收**：`cargo check --all-targets` 零 error/warning；`cargo test` 全绿；既有 169 测试迁移后不缩水。

## 9. 范围外 / 待确认

- **跨会话领域延续**（需求「下一次会话选型 = 上一次会话领域」）：仍列后续增强，本期不做。
- **hooks 是否新增 wire 干预能力**（before 后可改 wire）：本期只做可观测（ctx.wire），不做可改写；有真实需求再扩。
- `ConversationMode`（Chat/Agent/Assistant）：保留仅作展示与路由标记，不进管道。

## 10. Checkpoint

- [x] 背景与问题盘点（生产/消费点全景）确认
- [x] 三段管道设计（Resolver / Assembler / Executor）+ 数据契约确认
- [x] 取消 NeuronCallService、独立 RoundResolver、全部收敛（D1-D3）确认
- [x] 迁移矩阵与文件划分（第 6 / 7 节）确认
- [x] 进入实现

## 11. Change Log

| 日期 | 变更 |
|---|---|
| 2026-08-16 | 初稿：取消 NeuronCallService，三段管道（Resolver/Assembler/Executor），wire 一等产物，6 独立组装点收敛 |

## 12. Validation / Resume

- 验收：`cargo test -p pulsar-app`（src-tauri）全绿；聊天主链路（Chat/Agent/Assistant/Poller）与 `call_judgement` 回归。
- 断点续做：从第 10 节 Checkpoint 逐项推进；先拆 `round_types.rs` 与三段组件，再改 runner，最后收敛独立组装点。
