# Round Pipeline v2：resolve 收拢 + 真相源收敛为 Message

> 状态：设计定稿，待实现
> 需求来源：推翻 `2026-08-16_12-00_round-pipeline-split.md`（三段管道 v1）——用户连续纠偏后确认的架构级重构
> 取代：`docs/specs/2026-08-16_12-00_round-pipeline-split.md`（不再实现，作为历史存档保留）

## 1. 背景与问题

v1 三段管道（Resolver / Assembler / Executor）引入 `ResolvedRound` / `WireRound` 两个中间产物后，暴露出方向性错误：

1. **resolve 目标被误解为「身份决策」**：v1 的 `ResolvedRound` 含 5 字段（selected_neuron / role_system / behavior / inject_context / next_state），把冻结状态机、角色内容加工、状态推进都塞进 resolve。实际目标只有一个——**获取角色神经元**（behavior 与 context 都内含于神经元）。
2. **真相源分裂**：落库的是 `Message`（带 kind），发给模型的是 `ModelMessage`，而 v1 又造了 `WireRound.messages: Vec<ModelMessage>` + `role_context_message: Option<String>` 做中间层——**落库、发模型、审计三份数据各说各话**，只能靠投影反推「模型看到了什么」。
3. **B2 冻结状态机冗余**：`SessionState.stable_system_prompt` / `stable_system_frozen` + `freeze_or_replace` + `inject_context` 用跨轮状态保角色稳定。但首轮 System 落库后，**历史自带 System，稳定性天然保证**，跨轮状态是多余。
4. **组装行为外置**：拼接 messages 的职责在 `assemble_round` / `assemble_with_context`，与选型分离，产生「选型归 resolve、拼接归 assemble、落库归 runner」的三方错位。

## 2. 核心原则

1. **resolve 目标单一**：`resolve` 只管两件事——选型 + 把选中的神经元 context 拼到 old_messages（首轮 System / 后续 RoleContext）。不掺状态推进、不加工角色内容、不决定工具授权。
2. **真相源唯一**：管道内全程 `Vec<Message>`（`MessageBody` 带 kind，自描述）。落库 = 原样增量落；发模型 = 发送前 `from_message` 投影 `ModelMessage`。**不存在第二份「给模型的 msg」**。
3. **wire 即落库**：进入 wire 的消息全部落库（含 System / RoleContext / Nudge），取消 nudge_persist 例外。
4. **删跨轮状态**：B2 冻结状态机整体删除；`SessionState` 只留选型锚点 `last_selected_neuron_id`。
5. **工具授权与选型解耦**：按会话模式决定（不经过 resolve），落点在 `round_executor::execute`。

## 3. 设计决策（已与用户确认）

| # | 决策 |
|---|---|
| D1 | **resolve 新契约**：`resolve(seed, last_selected_neuron_id, old_messages: &[Message]) -> AppResult<(Vec<Message>, Option<Neuron>)>`。输入不含本轮输入消息；输出 = old_messages + 角色上下文（不含输入），以及选中的神经元 |
| D2 | **删类型**：`ResolvedRound`、`WireRound` 整体删除；`SessionState` 删 `stable_system_prompt` / `stable_system_frozen`，仅留 `last_selected_neuron_id` |
| D3 | **删冻结逻辑**：`freeze_or_replace` / `inject_context` / `next_state` / `role_system` / behavior 推导全部删除 |
| D4 | **拼接内聚于 resolve**：选中神经元时——首轮（old 为空）→ `System(neuron.content)` + 输入由 runner 追加；非首轮 → `RoleContext("[当前角色]\n" + neuron.content)`。无选中神经元 → 仅追加输入消息 |
| D5 | **输入消息 runner 管**：`InputRecord`（User / Continue / Nudge）→ `Message`（kind 自明）由 runner 构造，append 到 resolve 结果构成 wire |
| D6 | **进 wire 必落库**：`persist_input` 直接落 `wire[old.len()..]` 增量，System / RoleContext / Nudge / 输入全部落（取消 nudge_persist 例外） |
| D7 | **last_selected 发送前写回**：resolve 返回选中神经元后、模型调用前，把 id 写入 `SessionState` |
| D8 | **工具授权按会话模式**：落点在 `round_executor::execute`；`ConversationMode::tool_tags()` 标签并入机制保留，`tool_override` 参数保留（Agent 业务层显式授权） |
| D9 | **保留外部契约**：`ModelCallInput::assemble` / `replace_system` / `from_message` / `sanitize_tool_pairs` 保留（selection / evolution / compactor / neuron-model / tui / cli 6 个独立组装点仍使用，均传空历史） |
| D10 | **删 `assemble_round` / `assemble_with_context`**：主链路不再调用；`insert_id` 契约段随删除（主链路已无使用） |

## 4. 新实体与接口

### 4.1 类型（`round_types.rs`）

```rust
/// 会话级运行态：仅保留选型锚点（B2 冻结字段已删）。
pub struct SessionState {
    pub last_selected_neuron_id: Option<String>,
}

pub enum SessionSeed { Global, Neuron(String) }   // 不变

/// 单轮产物（不变）。
pub struct RoundOutcome {
    pub response: String,
    pub model_output: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_results: Vec<ToolResultItem>,
    pub selected_neuron_id: Option<String>,
}

// 删除：ResolvedRound、WireRound
```

### 4.2 RoundResolver（`round_resolver.rs`）

```rust
pub struct RoundResolver { neuron_manager: Arc<NeuronManager>, model_caller: Arc<dyn ModelCaller> }

impl RoundResolver {
    /// 选型 + 角色上下文拼接，目标单一：获取角色神经元。
    ///
    /// 选型（保留 v1 全部硬规则）：种子三路分派（Global 全域选/复用锚点、
    /// 普通 Neuron 邻域、系统 Neuron behavior 三策略+宽容回退）、reuse_selected_neuron 降频、
    /// 单候选短路。
    ///
    /// 拼接：选中神经元且 old 为空 → 追加 System(neuron.content)；
    ///       选中神经元且 old 非空 → 追加 RoleContext("[当前角色]\n" + neuron.content)；
    ///       未选中 → 原样返回 old。
    pub async fn resolve(
        &self,
        seed: Option<&SessionSeed>,
        last_selected: Option<&str>,
        old_messages: &[Message],
    ) -> AppResult<(Vec<Message>, Option<Neuron>)>;
}
```

### 4.3 RoundExecutor（`round_executor.rs`）

```rust
impl RoundExecutor {
    /// 消费完整 wire（Vec<Message>）+ 选中神经元。
    /// 发送前统一投影：from_message + 防御过滤 + sanitize_tool_pairs（原 to_model_messages 迁入）。
    /// 工具授权（落点，按会话模式）：
    ///   - Chat      → 空
    ///   - Agent     → tool_override（注册表全部，业务层传入）
    ///   - Assistant → neuron.tool_ids ∩ 注册表 ∪ mode.tool_tags()
    ///   - System    → neuron.tool_ids ∩ 注册表 ∪ mode.tool_tags()
    /// 授权工具校验 → 模型调用 → 单轮全部 tool_calls 执行 → RoundOutcome。
    pub async fn execute(
        &self,
        neuron: Option<&Neuron>,
        messages: &[Message],
        model: &ChatModelSelection,
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
    ) -> AppResult<RoundOutcome>;
}
```

### 4.5 hooks 位置与 RoundContext（`conversation_runner.rs`）

hooks 相对位置不变（before 在 resolve 前，after 在 persist_outcome 后），可见数据随真相源收敛调整：

```rust
pub struct RoundContext {
    pub session_id: String,
    pub mode: ConversationMode,
    pub seed: Option<SessionSeed>,
    pub state: SessionState,                  // 仅 last_selected_neuron_id
    pub messages: Vec<Message>,               // 真相源：before 时 = 历史；resolve+append 后 = 完整 wire
    pub model_input: String,                  // hook 填简报/输入；runner 用它构造输入消息
    pub model: ChatModelSelection,
    pub tool_override: Option<Vec<String>>,
    pub trigger: RoundTriggerKind,
    pub topic_id: Option<String>,
    pub reselect: bool,
    /// 是否将简报构造为 Nudge 消息进 wire（进 wire 必落库）。
    /// 语义由「是否落库」改为「是否生成」：false 时简报不进 wire（历史回灌自带）。
    pub nudge_persist: bool,
    /// resolve 后填充：本轮选中神经元（hooks 审计「选了谁」；产物盖章）。
    pub selected_neuron: Option<Neuron>,
    pub outcome: Option<RoundOutcome>,        // 保留
    // 删除：resolved / wire（wire 即 messages，无第二份）
}
```

生命周期：

```
读会话（old: Vec<Message>）
→ ① before_round(&mut ctx)        // resolve 前：可改 seed / reselect / model_input /
                                  //   state / topic_id / tool_override；可切换会话（reload）
→ resolve(seed, last_selected, &old) → (with_role, neuron)
→ 写回 last_selected（发送前）
→ 构造输入消息 append → ctx.messages = 完整 wire
→ persist_input（落 ctx.messages[old.len()..]，全落）
→ execute（发送前投影 ModelMessage）
→ persist_outcome
→ ② after_round(&mut ctx)         // persist 后：可读 messages(wire) / selected_neuron / outcome
```

hook 职责边界不变：before 只改「决策输入」（历史消息只读），after 只读产物。

### 4.6 ConversationRunner 编排（`conversation_runner.rs`）

```rust
impl ConversationRunner {
    /// 业务入口（签名不变）：
    /// 读会话 → before hooks → ① resolve → 写回 last_selected（发送前）
    /// → 构造输入消息 → wire = with_role + [input]
    /// → persist_input（落 wire[old.len()..] 增量，全落）
    /// → ② execute（发送前投影）→ persist_outcome（产物落库 + 盖章）
    /// → after hooks
    pub async fn run_round(session_id, input, tool_override, model, hooks) -> AppResult<ChatResponse>;

    /// 原始单轮管道（无会话）：入参适配为 Vec<Message>，不读库、不落库、不跑 hooks。
    pub async fn run_raw_round(
        &self,
        seed: Option<SessionSeed>,
        last_selected: Option<String>,  // 替代 SessionState（只取锚点）
        old_messages: Vec<Message>,     // 落库层历史
        model_input: &str,              // runner 构造输入消息
        tool_override: Option<Vec<String>>,
        tool_tags: Vec<ToolTag>,
        reselect: bool,
        model: &ChatModelSelection,
    ) -> AppResult<RoundOutcome>;
}
```

## 5. 消息流

```
Runner.run_round:
  读会话 → old = Message[]（落库真相源）
  → before hooks（可改 seed / reselect / input）
  → ① resolver.resolve(seed, last_selected, &old)
       → (with_role, neuron)          // with_role = old + [System | RoleContext]，不含输入
  → 写回 SessionState.last_selected_neuron_id（发送前）
  → input_msg = 按 trigger 构造（User / Continue / Nudge → Message，kind 自明）
  → wire = with_role + [input_msg]
  → persist_input：落 wire[old.len()..]（角色上下文 + 输入，全部落库）
  → ② executor.execute(neuron, &wire, model, tool_override, tool_tags)
       └ from_message 投影 + 防御过滤 + sanitize_tool_pairs → ModelCallRequest
  → persist_outcome：产物落库（tool_call/tool_result 或 assistant text，盖章 neuron_id）
  → after hooks
```

## 6. 行为变化（相对现状）

| # | 变化 |
|---|---|
| B1 | Poller **进 wire 必落库**：`nudge_persist` 语义由「是否落库」改为「是否构造 Nudge 进 wire」；false（复用简报轮）时简报不进 wire，历史回灌自带上轮简报 |
| B2 | `last_selected_neuron_id` **发送前写回**（原随 next_state 发送后写） |
| B3 | 工具授权：`behavior.tools` 三策略 → 按会话模式（Assistant/System 取 `neuron.tool_ids`）；标签并入与 override 机制不变 |
| B4 | 首轮 System 落库内容 = `neuron.content`（不再经 `stable_system_frozen` 判断，该字段已删） |
| B5 | 角色上下文识别靠 `MessageBody::RoleContext`（kind），不再依赖 `[当前角色]` 前缀推断（前缀保留为自描述内容，供模型与审计阅读） |

## 7. 迁移矩阵

| 现状 | 去处 |
|---|---|
| `ResolvedRound`（5 字段） | 删除：选型输出 = `Option<Neuron>`；角色上下文并入 `Vec<Message>` |
| `WireRound`（messages / role_context_message） | 删除：真相源 = `Vec<Message>` |
| `SessionState.stable_system_prompt` / `stable_system_frozen` | 删除（首轮 System 落库即稳定） |
| `resolve(seed, state, history: &[ModelMessage], reselect) -> ResolvedRound` | `resolve(seed, last_selected, old: &[Message]) -> (Vec<Message>, Option<Neuron>)` |
| `freeze_or_replace` / `inject_context` / `next_state` / `role_system` | 删除 |
| `assemble_round` / `assemble_with_context` | 删除（拼接进 resolve + runner 追加输入） |
| `to_model_messages`（runner） | 迁入 `round_executor::execute`（投影 + 防御 + sanitize） |
| `persist_input`（三态落库 + nudge_persist 例外） | 落 `wire[old.len()..]` 增量，全落 |
| 会话态写回 `resolved.next_state` | `last_selected_neuron_id` 发送前写回 |
| `ModelCallInput::assemble` / `replace_system` / `from_message` / `sanitize_tool_pairs` | 保留（6 个独立组装点使用） |
| 选型：种子三路分派 / reuse_selected_neuron / 单候选短路 | 保留（原样迁入新 resolve） |
| `ConversationMode::tool_tags()` | 保留（标签并入机制不变） |

## 8. 文件划分

| 文件 | 改动 |
|---|---|
| `round_types.rs` | 删 `ResolvedRound` / `WireRound`；`SessionState` 删冻结字段 |
| `round_resolver.rs` | 重写 `resolve` 契约；删冻结逻辑；拼接内聚 |
| `round_executor.rs` | `execute` 消费 `Vec<Message>` + `Option<Neuron>`；发送前投影；工具授权按模式 |
| `model_call_input.rs` | 删 `assemble_round` / `assemble_with_context`；保留 `assemble` / `from_message` / `sanitize_tool_pairs` / `replace_system` |
| `conversation_runner.rs` | `run_round` 新编排；`persist_input` 落增量；`last_selected` 发送前写回；删 `to_model_messages`；`run_raw_round` 适配 |
| `assistant_session.rs` / `chat_session.rs` / `agent_session.rs` / `gateway.rs` | 签名与调用适配（行为不变） |

## 9. 测试计划

- **Resolver 单测**：种子三路分派全分支；降频（reselect=false 且锚点 → 沿用）；拼接规则（首轮空历史 → System；非空 → RoleContext 前缀；未选中 → 原样）。
- **Executor 单测**：发送前投影（Nudge/RoleContext → User 文本）；sanitize 配对保留；工具授权按模式四象限；未授权工具拒绝；多 tool_calls 全执行。
- **Runner 集成**：persist_input 增量落库（System / RC / Nudge 全落）；last_selected 发送前写回；产物落库盖章。
- **业务回归**：Chat（无选型无工具）、Agent（override 全工具多轮）、Assistant / System（神经元工具 + 标签）行为不变；`call_judgement` 走 `run_raw_round` 不落库。
- **验收**：`cargo check --all-targets` 零 error / warning；`cargo test` 全绿；v1 中断言 `next_state.stable_system_frozen` / `role_context_message` 的用例重写为断言 messages 内容。

## 10. 范围外 / 待确认

- 跨会话领域延续（下次会话选型 = 上次会话领域）：沿用 v1 结论，本期不做。
- hooks 对 wire 的干预（before 后改写消息）：本期只做可观测（ctx 可读 wire），不做可改写。
- 工具授权精确映射（D8）已在对话中确认：保留 `tool_override` + 按模式取 `neuron.tool_ids` + 标签并入，不再问询。

## 11. Checkpoint

- [x] resolve 目标对齐：只做「选型 + context 拼接」，不掺状态推进 / 内容加工 / 工具授权
- [x] 真相源收敛：管道内全程 `Vec<Message>`，落库原样增量，发送前投影 `ModelMessage`
- [x] 删 B2 冻结状态机：首轮 System 落库即稳定，`SessionState` 仅留锚点
- [x] 三个行为变化确认：进 wire 必落库 / last_selected 发送前写回 / 工具授权按会话模式
- [x] 保留边界确认：`assemble` / `from_message` / `sanitize_tool_pairs` / `replace_system` / 会话读写 / 选型硬规则
- [ ] 进入实现（r1→r9，待用户批准）

## 12. Change Log

| 日期 | 变更 |
|---|---|
| 2026-08-16 | 初稿：推翻三段管道 v1，resolve 收拢（选型+拼接合一），真相源收敛为 `Vec<Message>`，删 B2 冻结状态机，工具授权按会话模式 |

## 13. Validation / Resume

- 验收：`cargo check --all-targets`（src-tauri）零 error / warning；`cargo test` 全绿；Chat / Agent / Assistant / System 与 `call_judgement` 回归。
- 断点续做：按 r1→r9 执行——r1 类型层 → r2 resolver → r3 model_call_input → r4 executor → r5 runner → r6 调用点 → r7 测试 → r8 编译与测试 → r9 文档同步（msgs-lifecycle / session-message-architecture / micro_spec / B2 spec）。
