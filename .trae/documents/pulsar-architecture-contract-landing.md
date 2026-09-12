# Pulsar 后端架构契约落地计划

## Context（为什么做）

架构设计文档 `docs/specs/2026-09-06_13-55_pulsar-backend-architecture-redesign.md` 声明"M1-M5 全部落地"，但核查代码后确认：**契约层（类型依赖方向、端口 trait、测试）落地了，结构层没有**。缺口：

1. **物理位置不符**：`src/core/` 一锅端——核心、应用层、扩展实现全部同目录，违反 spec §六"具体实现位于契约之外，由组合根装配"。
2. **核心直接引用扩展**：round_service（原 conversation_runner）import `conversation_store::{now_ms, ConversationStore}`（stores）、`hook::defs::HookRegistry`（policies）、`openai_compat`（providers）；round_executor import `tool_registry::ToolRegistry`（tools）、`openai_compat`（providers）；round_resolver import `neuron_manager`（policies）。**目录树成立的前提是先做"核心去扩展依赖"抽象**。
3. **契约类型缺失**：`Preparation` / `PersistedOutcome` / `SessionSnapshot` / `ModelRef` / `ToolDescriptor` 不存在；`ModelRequest`/`ModelResponse`/`AuthorizedToolCall`/`ToolResult` 仅 type alias。
4. **命名不一致**：`ConversationStore` 端口与 JSON 实现同名、实现 RoundService 的文件叫 conversation_runner.rs、`RegistryToolExecutor` 不体现 spec §5.4"Capability Adapter"。
5. **过渡字段**：`RoundRequest` 携带 `model`/`tool_override`/`thinking_override`（spec M1 记录待收编）。
6. **流式未端口化**：`send_model_message_stream` 直走 `run_round_stream`，绕过驱动（spec Open Questions 遗留）。

**目标**：代码位置、引用、命名三者与设计契约一致；wire 命令名 / JSON 字段 / `.pulsar/` 布局 / 消息顺序 / 工具配对 / 输入先落库语义（spec §七）保持不变。

## 落地进度

| 批次 | 内容 | 状态 |
|---|---|---|
| 阶段一批1 | 核心稳定类型收敛（ResponseFormatSpec/StreamChunk→models、now_ms→infra/time、PollerStatus→events） | ✅ 代码完成（`cargo check --all-targets` 0 错 0 警告） |
| 阶段一批1 | hook 协议/业务分层（defs 留 core，registry+instances+judgement+store+compaction→application/hook） | ✅ 决策固化（spec §四/§5.2 已修订），物理迁移随批次7 |
| 阶段一批2 | sinks（`StateEventSink` + app_log/log_redact → sinks/） | ✅ 完成（0 错 0 警告） |
| 阶段一批3 | tools + CapabilityAdapter + 核心 `ToolCatalog` 只读端口 | ✅ 完成（0 错 0 警告） |
| 阶段一批4 | providers（含 `impl ModelCaller for ProviderRegistry` 迁出 core） | ✅ 完成（0 错 0 警告） |
| 阶段一批5 | stores（4 模块迁移 + runner store 字段端口化） | ✅ 完成（0 错 0 警告） |
| 阶段一批6 | policies（compactor + neuron/ + 删 neuron 兼容别名） | ✅ 完成（0 错 0 警告） |
| 阶段一批7 | application（9 模块 + hook 业务归位，core 只剩协议） | ✅ 完成（0 错 0 警告） |
| 阶段一批8 | config→infra + `conversation_runner.rs`→`round_service.rs` + 入口收尾 | ✅ 完成（0 错 0 警告） |
| 阶段一验收 | 455 基线 + 文档反向同步 | ✅ 完成（455 通过 / 0 失败，四入口编译） |
| 阶段二·2.1 | 契约类型真实更名（删 4 个 alias + JsonConversationStore） | ✅ 完成（455 通过） |
| 阶段二·2.2a | 端口形状对齐 §3.5（SessionSnapshot / PersistedOutcome / load / append_input / append_outcome） | ✅ 完成（455 通过） |
| 阶段二·2.2b | `RoundRequest` 去过渡字段 + 核心默认策略 `round_policy`（RoundContext 形状 / ModelRef / ToolDescriptor / Preparation 未落地，见「已裁决结论」待办） | ✅ 完成（455 通过；形状类待办见下） |
| 阶段三 | 流式路径端口化（`run_stream` + `StreamDelta` / `DomainEvent::Delta` 契约化 + 会话层收敛） | ✅ 完成（455 通过） |

## 已裁决结论（2026-09-12）

阶段二·2.2b 与阶段三此前卡在同一个 spec 级缺口，**已由用户裁决并据此落地**（不再阻塞）：

1. **模型选型 = A1+（会话模型口径）**：模型选型归会话——**会话有自己的 model**（落库 `extra.session.state.model`，应用侧 `set_session_model` 写入）。核心 `load_context` 从会话运行态读取本轮模型：它首先作用于主对话的调用模型，其次作为上下文传入 hook；**每个 hook 有权决定用对话带过来的 model 还是自己约定的 model**。`None` → 核心返回领域错误（核心不依赖 providers、**不新增模型解析端口**）。据此 `RoundRequest` 已去过渡字段，收窄为 `{ session_id, input, mode }`。
2. **IP-1 边界 = B1（保留 `&mut RoundContext`，不引入 `Preparation`）**：IP-1 现有副作用（会话切换 reload、课题 / 计数等业务字段透传）本质是**业务编排而非上下文注入**，用 `&mut RoundContext` 直改更贴合；`Preparation` **未引入**。授权 / 思考默认改由核心 `round_policy` 承担——授权按 `RoundMode`（`Agent`→目录全量；`Chat` / `Assistant` / `Poller`→`None`，执行面回退神经元 `tool_ids`），思考按**触发类型**（`User` / `AgentLoop`→跟随会话 / 模型；`ManualStep` / `Poller`→显式关闭）。
3. **推而解之**：阶段三流式变体不再阻塞——`RoundService::run_stream` / `RoundDriver::run_stream` 已落地，`StreamDelta` 入 `core/round_contract.rs`、经 `DomainEvent::Delta { conversation_id, delta }` 发布，供应商 SSE 增量更名 `models::SseDelta`；会话层流式路径全部改走驱动 `run_stream`，`run_round_stream` 降为 `RoundService::run_stream` 的私有内部实现（应用层零直调）。

**待办（如实记录，未落地）**：`RoundContext` 形状未按 spec §3.2 换成 `selected_model: ModelRef` + `authorized_tools: Vec<ToolDescriptor>`（`ctx.model` 仍为完整 `ChatModelSelection`，承载采样 / 思考，hook 需要）；`ModelRef` / `ToolDescriptor` 类型**未引入**；`Preparation` **未引入**。留待后续迭代。

**附加说明（原「额外发现」）**：`HookHandler::AfterLoadContext` 的现有实现（`application/gateway.rs`、`application/assistant_session.rs`）会改写 `RoundContext` 的 `session_id` / `topic_id` / 计数 / `state` 等**业务字段**，超出 `Preparation{messages_to_append, model_override, tool_policy, reload_session}` 的表达范围——此即 B1 裁决的直接依据。

## 目录架构对齐基准（spec §四 五层子图 → 目录）

| spec 子图 | 目录 | 内容 |
|---|---|---|
| entries 入口适配层 | `src/lib.rs`、`src/net/`、`src/tui/`、`src/bin/`、`src/terminal/`、`src/fileops/` | 现状即达标，不动 |
| application 应用驱动层 | **`src/application/`** | gateway（组合根）、drivers、chat/agent/assistant_session、session_tracker、insert_catalog、poller/poller_step、**hook/（注册表 + 实例 + 裁决账本，业务钩子）** |
| core 封闭核心 | **`src/core/`** | RoundService 实现、会话不变量、Capability Execution 协议、结果/事件语义、**hook 插槽协议**、稳定类型 |
| ports 核心端口 | `src/core/round_contract.rs` | ModelPort / ConversationStore / CapabilityExecutor / EventSink / RoundDriver |
| extensions 扩展实现 | `src/providers/`、`src/tools/`、`src/stores/`、`src/policies/`、`src/sinks/`、`src/infra/` | 六类各归其位 |

依赖规则（spec §六）：`core/` 不引用其它目录（唯一已知例外：阶段一 round_resolver→neuron，阶段二硬截止拆除）；扩展目录只引用 `core/` 稳定类型；`application/` 组合核心与扩展；入口只引用 `application/gateway`。

## 目标目录结构

```
src/
├── core/          ← 封闭核心 + 稳定类型（只含这些）
│   round_contract.rs（端口+契约）  round_types.rs
│   round_service.rs（原 conversation_runner.rs，实现 RoundService）
│   round_executor.rs（RoundExecutor：核心执行步骤 + ModelCaller）
│   round_resolver.rs（阶段一暂留，阶段二经选型端口拆除 neuron 依赖）
│   session_coordinator.rs  context_safety.rs  model_call_input.rs
│   events.rs（StateChange / STATE_CHANGED_EVENT / PollerStatus / PollerRunState——wire 词汇）
│   models.rs（含迁入的 ResponseFormatSpec / StreamChunk / StreamChoice / StreamDelta / ToolCallWire / FunctionCallWire / Usage）
│   error.rs  log_phase.rs
│   hook/           ← hook 注入点协议（spec §5.2 核心规定的部分，**只含协议**）
│       defs.rs（InjectPointId / HookHandler / HookDef / HookRegistry / 失败策略）
├── application/   ← 应用驱动层 + 组合根
│   gateway.rs（组合根）  drivers.rs  chat_session.rs  agent_session.rs
│   assistant_session.rs（含 AssistantHooks 业务上下文）  session_tracker.rs  insert_catalog.rs
│   poller.rs  poller_step.rs
│   hook/          ← hook 业务（与 AssistantHooks 同层，零反向边）
│       registry.rs（HookInstance / HookRun / ACTIVE_HOOKS / LEGACY_HOOKS）
│       judgement.rs（裁决规则 HookDef / hook_def 查询）
│       instances/（各 IP-1～IP-5 注册实现）  compaction/（自动压缩 hook 实例）
│       store.rs（HookJudgementStore 裁决账本）
├── providers/     ← Provider Adapters
│   providers.rs  openai_compat.rs（import core 稳定类型）
├── tools/         ← Tool Registry + Capability Adapter
│   tool_registry.rs  tool_config.rs  mcp.rs  dynamic_tool.rs  cmd_exec.rs
│   current_time.rs（GetCurrentTimeTool，原 core/current_time.rs）
│   capability_adapter.rs（自 round_executor.rs 迁出，RegistryToolExecutor→CapabilityAdapter）
├── stores/        ← JSON / SQLite Stores
│   conversation_store.rs（实现 JsonConversationStore）  storage.rs
│   topic_store.rs  topic_manager.rs
├── policies/      ← Policy Registry + Neuron（策略；hook 实例在 application/hook/）
│   compactor.rs  neuron/
├── sinks/         ← Tauri / SSE / WS / Log Sinks
│   state_sinks.rs（StateEventSink：EventSink 真实实现，自 round_contract.rs 迁出）
│   app_log.rs  log_redact.rs
└── infra/         ← 通用基础设施（无业务语义）
    config.rs  time.rs（now_ms；原 conversation_store.rs）
```

> 说明①：`GetCurrentTimeTool` 是 Tool Registry 成员，归 `tools/`；`now_ms` 是无业务语义的时钟助手，归 `infra/time.rs`。两者职责不同，不共用 `current_time.rs` 文件名。
> 说明②：hook 分层由用户 2026-09-12 拍板——**协议入 core、业务入 application**。`HookRun::Before/After` 的第一参是 core 的 `RoundContext`（协议输入），第二参是 application 的 `AssistantHooks`（业务输入）；把注册表与实例放在 application 侧、与 `AssistantHooks` 同层，反向边自然消除，无需引入 `HookContext` 抽象。spec §四/§5.2 已同步修订（Change Log 2026-09-12）。
> 说明③（落地调整）：`StateEmitter`（`Arc<dyn Fn(StateChange)>`）**留在 `core/events.rs`** 而非迁 sinks——它是跨层注入的回调型稳定类型，下沉会造成 stores→sinks 的「扩展→扩展」边；迁入 sinks 的只有 `StateEventSink`（`EventSink` 真实实现）。
> 说明④（落地调整）：`ToolCatalog` 只读端口提前到阶段一批3 落地（原计划在阶段二）——`round_executor` 的授权决策需要读工具目录，若等阶段二，阶段一全程都会留着 core→tools 反向边。端口三方法：`tools_with_tag` / `list_definitions` / `definitions_for`，由 `impl ToolCatalog for RwLock<ToolRegistry>` 实现，`RoundExecutor::new(model_caller, catalog, capability)` 全端口化。
> 说明⑤（落地调整）：`ConversationStore` 端口在阶段一批5 增补 **4 个同步原语**（`require_conversation` / `add_message` / `update_message_at` / `save_conversation`），与既有异步契约面（`load` / `append` / `update_text_at` / `save`）并存。原因：核心管线实际走同步原语，只抽象能力字段而不抽象方法集则 runner 无法去具体类型；异步契约面按 spec §3.5 保留。

## 命名对齐表（总纲，落地时以此为唯一命名基准）

| 类别 | spec 术语 | 代码现状 | 目标 |
|---|---|---|---|
| 契约-服务 | `RoundService`（run/cancel） | trait ✓ | 不变 |
| 契约-输入 | `RoundRequest{session_id,input,mode}` | +过渡字段 | 去过渡字段 |
| 契约-上下文 | `RoundContext{session,messages,input,selected_model,authorized_tools}` | 旧形状 | 对齐形状，迁入 round_contract.rs |
| 契约-选型覆盖 | `Preparation{...}` | 不存在 | 新增 |
| 契约-会话快照 | `SessionSnapshot` | 不存在 | 新增；wire/JSON 布局不变 |
| 契约-模型引用 | `ModelRef` | 不存在 | 新增；`state.model` JSON 键不动 |
| 契约-工具描述 | `ToolDescriptor` | 不存在 | 新增 |
| 契约-落库投影 | `PersistedOutcome` | 不存在 | 新增 |
| 契约-结果 | `RoundOutcome{...}` | ✓ | 不变 |
| 契约-模型端口 | `ModelPort` / `ModelRequest` / `ModelResponse` | 后两者 alias | 真实更名，删除 alias |
| 契约-能力端口 | `CapabilityExecutor` / `AuthorizedToolCall` / `ToolResult` | 后两者 alias | 真实更名，删除 alias |
| 契约-存储端口 | `ConversationStore{load,append_input,append_outcome}` | 四原语 | 对齐 spec 形状，保留 update_text_at/save |
| 契约-事件端口 | `EventSink` / `DomainEvent{Fact,Delta}` | ✓ | 不变 |
| 驱动契约 | `RoundDriver` + 四驱动 | ✓ | 不变 |
| 实现-模型 | Provider Adapters | `ProviderRegistry` | 不变，→ providers/ |
| 实现-工具 | Tool Registry + Capability Adapter | `ToolRegistry` / `RegistryToolExecutor` | →**`CapabilityAdapter`**，→ tools/ |
| 实现-存储 | JSON / SQLite Stores | `conversation_store::ConversationStore` | →**`JsonConversationStore`**，→ stores/ |
| 实现-事件 | Tauri / SSE / WS / Log Sinks | `StateEventSink`（round_contract.rs）、`StateEmitter` | → sinks/；StateChange/PollerStatus 留 core |
| 实现-策略 | Hook / Policy Registry | `HookRegistry`（core 协议） + 业务实例 | 协议留 core/hook/defs.rs；注册表与实例归 application/hook/ |
| 文件 | 实现 RoundService 的模块 | conversation_runner.rs | →**round_service.rs** |

## 核心去扩展依赖（目录树成立的前置抽象，均保行为不变）

目录树要成立，必须先拆除 core 对扩展的直接引用：

1. **round_service 存储字段**：`Arc<...conversation_store::ConversationStore>`（具体类型）→ `Arc<dyn round_contract::ConversationStore>`（端口；JSON 实现已直接 impl 端口）。
2. **now_ms**：自 conversation_store.rs 迁入 `infra/time.rs`。✅ 已完成
3. **round_executor 能力字段**：`capability: RegistryToolExecutor`（具体类型）→ `Arc<dyn CapabilityExecutor>`（端口）；`RoundExecutor::new` 改为接收 `Arc<dyn CapabilityExecutor>`，删除 `tool_registry::ToolRegistry` import。
4. **ResponseFormatSpec + 流式协议簇**：自 openai_compat 迁入 `core/models.rs`（`ResponseFormatSpec` / `ToolCallWire` / `FunctionCallWire` / `Usage` / `CompletionTokensDetails` / `StreamChunk` / `StreamChoice` / `StreamDelta`）。✅ 已完成
5. **PollerStatus + PollerRunState**：自 poller.rs 迁入 `core/events.rs`。✅ 已完成
6. **hook 协议与业务分离**（2026-09-12 用户拍板）：`defs.rs`（InjectPointId / HookHandler / HookDef / HookRegistry / 失败策略）留 `core/hook/`；`registry.rs` + `instances/` + `judgement.rs` + `store.rs` + `compaction/`（业务，`HookRun` 签名依赖 `AssistantHooks`）归 `application/hook/`，与 Assistant 会话同层——反向边由同层共处自然消除，不引入 `HookContext` 抽象。`compactor.rs` + `neuron/` 归 `policies/`。
7. **round_contract.rs**：删除 `pub use super::round_executor::RegistryToolExecutor` 再导出（消费者改从 tools/capability_adapter 引用）；删除 StateEventSink 定义与其测试（迁 sinks/）。
8. **current_time.rs**：`GetCurrentTimeTool` 随工具系迁 `tools/current_time.rs`（原 core/current_time.rs 位置空出，避免与 infra/time.rs 同名混淆）。

## 阶段一：目录物理分离 + 核心去扩展依赖 + 文件更名（保行为不变）

按依赖拓扑分批，每批：类型收敛/抽象 → 移动文件 → 改 `use` → 删 `core/mod.rs` 对应再导出与兼容别名（`hook_judgement_store`/`neuron_config`/`neuron_manager`/`neuron_model`/`neuron_store`/`spec_manager`）→ 改外部入口 → `cargo check` → 批次末 `cargo test --all-targets`（455 基线）。**不用 shim，编译器即校验器**。

1. **核心稳定类型收敛（先行）**：ResponseFormatSpec/流式协议簇 → models.rs ✅；now_ms → `infra/time.rs` ✅；PollerStatus/PollerRunState → core/events.rs ✅；hook 协议/业务分层决策固化 ✅（物理迁移随批次7）。
2. **sinks**：StateEmitter + StateEventSink → `crate::sinks::state_sinks`（round_contract.rs 定义与测试迁走）；app_log/log_redact → sinks/；round_contract.rs 删 `pub use RegistryToolExecutor`。
3. **tools + CapabilityAdapter**：RegistryToolExecutor（round_executor.rs）→ `tools/capability_adapter.rs` 更名 `CapabilityAdapter`；`RoundExecutor::new(Arc<dyn CapabilityExecutor>, ...)`；tool_registry/tool_config/mcp/dynamic_tool/cmd_exec/current_time → tools/（引用方含 drivers、gateway、fileops、gitops、lib.rs、net/rpc）。
4. **providers**：providers/openai_compat → providers/（引用方含 gateway、hook/instances、neuron/model 的 `DefaultNeuronModelCaller`）。
5. **stores**：conversation_store/storage/topic_store/topic_manager → stores/；round_service store 字段改 `Arc<dyn round_contract::ConversationStore>`（引用方含 gateway、assistant_session、net/mod.rs 测试）。
6. **policies**：compactor.rs + neuron/ → policies/。**已知例外**：round_resolver→neuron 反向边暂留，阶段二选型端口硬截止拆除。
7. **application**：gateway/drivers/三 session/session_tracker/insert_catalog/poller/poller_step → application/；hook 业务（registry/instances/judgement/store/compaction）→ application/hook/（含 `AssistantHooks` 同层归位）。
8. **外部入口**：lib.rs（`crate::core::{...}` 全量替换）、net/mod.rs、net/sse.rs、net/rpc.rs、tui/app.rs、terminal 3 文件、runtime/script_engine.rs、server_runtime.rs、bin/ 三文件改为新路径。

**阶段一验收**：`cargo test --all-targets` 455 通过 / 0 失败；wire 与存储行为零改动。

## 阶段二：契约类型补全 + 类型更名

1. **真实更名（删除 alias）**：`ModelCallRequest`→`ModelRequest`、`ModelCallResponse`→`ModelResponse`、`ToolCall`→`AuthorizedToolCall`、`ToolResultItem`→`ToolResult`、conversation_store 具体实现→`JsonConversationStore`。serde 字段与 JSON 布局不变。
2. **SessionSnapshot**（round_contract.rs）：`{ id, mode, seed, state, messages }`，核心提供 `from_conversation(&Conversation)` 投影；端口 `load(id)->SessionSnapshot`；`round_service::load_context` 改消费快照。
3. **ModelRef / ToolDescriptor**：`{provider_id, model_id}` 与 `ChatModelSelection` 互转；`{name, description, parameters}` 自 ToolDefinition 投影。`RoundContext.selected_model: ModelRef`、`authorized_tools: Vec<ToolDescriptor>`。
4. **RoundContext 对齐**：spec §3.2 形状迁入 round_contract.rs；runner 内部扩展字段收敛为私有实现上下文。
5. **Preparation + IP-1**：`HookHandler::AfterLoadContext` 返回 `AppResult<Preparation>`；核心统一应用（reload → 覆盖选型 → 追加消息 → tool_policy 与 context_safety 下限求交）；选型锚点写回从 hook 移入核心（IP-1 后、persist_input 前，保持"发送前落锚点"时序）。
6. **PersistedOutcome**：核心 persist_outcome 阶段组装，`store.append_outcome` 落库；端口方法对齐 `load/append_input/append_outcome` + 保留 `update_text_at/save`。
7. **RoundRequest 去过渡字段** → `{session_id, input, mode}`。默认策略落核心（新 `core/round_policy.rs`）：
   - 选型：`state.model` → None 回退全局默认；gateway `send_chat_message` 仍收 model 参数（wire 不变），run 前写 state.model。
   - 授权：核心定义只读端口 `ToolCatalog`（tools 层实现）；RoundMode 默认：Agent=全量、Chat/Assistant=神经元 tool_ids+mode tags ∩ 注册表、Poller=同 Assistant；Preparation.tool_policy 覆盖。
   - thinking：Poller→disabled；其余跟随模型/会话（None 路径）。

**阶段二验收**：455 测试 + drivers 4 契约测试全过；选型/授权决策上移与现 executor 逻辑逐位一致；`state.model` JSON 键不动。

## 阶段三：流式路径端口化

- `RoundService` 增 `run_stream(request, on_delta)`；`RoundDriver` 增同名变体；AgentDriver 循环内共享回调（上移 agent_session.rs 的 Arc<Mutex> 逻辑）。
- Delta 契约化：`StreamDelta` 收敛为 `DomainEvent::Delta` 唯一载荷；on_chunk 累积 → publish Delta；Gateway 经 EventSink 映射 `StateChange::MessageDelta`。
- 收敛直调：chat_session / agent_session / assistant_session 的 `run_round_stream` 全改走 `driver.run_stream`；`run_round_stream` 降为 RoundService 内部实现。

**阶段三验收**：MessageDelta 事件序列与 `done:true` 收敛语义不变（GUI/SSE 消费方依赖）；455 测试全过。

## 文档反向同步（Reverse Sync，先文档后代码）

- ✅ 2026-09-12：spec §四架构图 + §5.2 物理落点 + Change Log（hook 协议/业务分层）；本落地计划目标目录结构。
- 阶段一末：`docs/pulsar/architecture.md` 模块表与文件路径章节。
- 阶段二末：spec 状态行 + Change Log 增条目；`msgs-lifecycle.md` / `session-message-architecture.md` 补 SessionSnapshot/PersistedOutcome 命名与端口形状；architecture.md §3 契约块、§5 IP-1 Preparation 说明。
- 阶段三末：spec §5.1 驱动流式变体、Open Questions「流式端口化」关闭、Checkpoint Summary；architecture.md 时序图。

## 验证与风险

- 每批 `cargo check`；每批末 `cargo test --all-targets`（455 基线）；每阶段末四入口编译冒烟（GUI + 三 bin）。
- 风险最大在阶段一（round_service store 字段与 RoundExecutor 字段抽象为端口、hook 协议/业务拆分——行为必须逐位一致）与阶段二（授权决策上移）、阶段三（Delta 事件序列）。
- 类型更名风险控制：更名并删 alias 后立刻 `cargo check` 全量兜底；serde 序列化字段逐处核对（spec §七 兼容面）。
- 顺序依赖：一→二→三严格串行。

## 验收标准（对齐 spec §十）

- 目录、引用、**命名**三者与「目录架构对齐基准」及「命名对齐表」一致（无兼容 shim、无别名、无反向边，除阶段一 round_resolver 已知例外）。
- 核心文件（round_contract/round_service/round_executor/session_coordinator/context_safety/hook/defs）不引用任何扩展目录。
- 新增 Driver/Provider/Tool/Store/Entry 时 RoundService 无需修改。
- `cargo test --all-targets` 455 通过 / 0 失败；四入口冒烟通过。
- spec 状态行如实标注"契约层+结构层+命名全部落地"。
