# Technical Plan / 技术方案: assistant-streaming-thinking

## Requirement Baseline / 需求基线

- 对应需求文档：[requirements.md](file:///home/lab/Documents/trae_projects/new-start-wt/docs/sdd-lab/2026-08-19_01-47_assistant-streaming-thinking/requirements.md)
- 需求确认状态：已确认（用户 2026-08-19 01:47 批准；Q1 统一落库类型 / Q2 远程模式一起做 / B1 按需回传 / C1 折叠展示 / D2 与流式一起做 均已确认）
- 本方案覆盖范围：
  1. 出参提取：`ModelCallResponse` / `RoundOutcome` 增加 `reasoning`，`parse_chat_response` 提取。
  2. 落库（Q1 统一类型）：`MessageBody::Text` 扩展为 wire 镜像 `{ content, reasoning, tool_calls }`，**删除 `ToolCall` 变体**（`#[serde(alias = "tool_call")]` 兼容存量）；兼容存量 JSON。
  3. 回灌（B1）：投影 `from_message` 按需回传 `reasoning_content`（DeepSeek 且该轮有工具调用）。
  4. 流式管道：`ModelCaller::call_model_stream` trait 化 → `round_executor` 流式入口 → runner 流式轮 → `ConversationStore` 增量落库（节流）→ `StateChange::MessageDelta` 增量事件。
  5. 远程模式（Q2 一起做）：RPC `send_chat_message` 走流式入口；SSE 复用同一 `StateChange` 广播自动推送增量。
  6. 前端：`MessageBody` 类型同步、`ThinkingBlock` 折叠组件、ChatMessage 分区渲染、dataStore 增量合并、MarkdownRenderer 节流与未闭合代码块兜底。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - [openai_compat.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/openai_compat.rs#L311-L322)：`StreamDelta` 已含 `reasoning_content`；[openai_compat.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/openai_compat.rs#L246-L260)：`ResponseMessage.reasoning_content` 已反序列化；`chat_stream` 已实现 SSE 解析 + 聚合（`StreamResult = ChatResponse`），流式 reasoning 已在 delta 层累积（L446-448）。
  - [providers.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/providers.rs#L335-L363)：`call_model_stream` 已实现但 `#[allow(dead_code)]`，签名 `on_chunk: impl FnMut(openai_compat::StreamChunk)`，内部走 `client.chat_stream` + `parse_chat_response`。
  - [providers.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/providers.rs#L365-L401)：`parse_chat_response` 只取 `choice.message.content`，**丢弃 `reasoning_content`**（需扩展 `ModelCallResponse`）。
  - [providers.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/providers.rs#L858-L897)：`to_chat_messages` 已支持 `ChatMessage.with_reasoning(...)`（回灌序列化落点，当前入参恒 None）。
  - [round_executor.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/round_executor.rs#L24-L34)：`ModelCaller` trait 仅 `call_model`；`ProviderRegistry` 已实现。新增 `call_model_stream` 需同步 trait 化。
  - [round_executor.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/round_executor.rs#L259-L266)：`RoundOutcome` 字段（`response` / `model_output` / `tool_calls` / `tool_results` / `selected_neuron_id`）——需加 `reasoning`。
  - [conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L106-L217)：`run_round` 全阻塞（persist_input → call_model → persist_model_decl → execute_tools → persist_outcome）；[conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L416-L469)：`persist_outcome` 纯文本分支落 `Text { content }`、工具分支落 `ToolResult`；[conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L379-L411)：`persist_model_decl` 落 `ToolCall { content, tool_calls }`。
  - [conversation_store.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_store.rs#L109-L128)：仅 `add_message`（append）+ `save_conversation`（全量 JSON 写盘）——**无 update 语义**。
  - [events.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/events.rs#L19-L40)：`StateChange::Conversations { affected }` 完成时一次广播；[lib.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L1114-L1122)：`broadcast::channel<StateChange>(256)` + `STATE_CHANGED_EVENT` emit。
  - [models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L17-L42)：`MessageBody` 变体（Text 仅 `content`）；[models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L54-L65)：`Message::text()` 全变体 match（**加字段后所有构造点编译期强制适配**）。
  - [models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L507-L530)：`ModelCallRequest` / `ModelCallResponse`（无 reasoning 字段）。
  - [model_call_input.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/model_call_input.rs#L244-L290)：`from_message` 投影（Text 分支构造 `ModelMessage`；ToolCall 分支构造 assistant + tool_calls）——回灌判定落点。
  - 前端：`types.ts` `MessageBody`（[types.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/types.ts#L37-L43)）；`ChatMessage.svelte` 分支渲染（[ChatMessage.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ChatMessage.svelte#L120-L141)）；`ToolCallBlock` 折叠模式（[ToolCallBlock.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ToolCallBlock.svelte#L8-L31)）；`MarkdownRenderer` 同步全量解析（[MarkdownRenderer.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/MarkdownRenderer.svelte#L25-L32)）；`dataStore` 事件按 kind 重拉（[dataStore.svelte.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/stores/dataStore.svelte.ts#L131-L163)）；`api/types.ts` `StateChangePayload`（[types.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/api/types.ts#L14-L32)）。

- 当前实现事实：
  - 协议层流式与 reasoning 解析**已就绪**，缺口全在管道（trait / executor / runner / store / 事件 / 前端）。
  - `call_model_stream` 在 providers 已可调用，只需 trait 化暴露 + executor/runner 接入。
  - 落库是「发送前 append 输入 + 发送后 append 产物」两段式；流式需「append 占位 + update 增量」。
  - 消息无唯一 id（只有 role/body/timestamp/neuron_id）；增量定位用「会话最后一条 assistant 消息」索引即可（流式场景单会话唯一）。

- 相关接口/数据结构：
  - `MessageBody::Text { content: String, reasoning: Option<String>, tool_calls: Option<Vec<ToolCall>> }`（Q1 统一类型；删除 `ToolCall` 变体）。
  - `ToolCall` 变体现有使用点（删除后需迁移）：[conversation_runner.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L402)（`persist_model_decl` 落库）、[model_call_input.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/model_call_input.rs#L264)（投影）、[models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L60-L87)（`text()` / `is_tool()` / `tool_calls()` 访问器）、[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L1326)（测试断言）。前端 [types.ts](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/types.ts#L37-L43) 与 [ChatMessage.svelte](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src/lib/components/ChatMessage.svelte#L120-L141) 的 `hasToolCalls` 逻辑同步改为「`Text.tool_calls` 非空」。
  - `ModelCallResponse { provider_id, model_id, output, tool_calls, finish_reason, reasoning: Option<String> }`。
  - `RoundOutcome { response, model_output, tool_calls, tool_results, selected_neuron_id, reasoning: Option<String> }`。
  - `ModelCaller::call_model_stream(&self, request, on_chunk: impl FnMut(openai_compat::StreamChunk) + Send) -> AppResult<ModelCallResponse>`。
  - `ConversationStore::update_last_assistant_message(conversation_id, patch: |&mut Message|) -> AppResult<Conversation>`（节流由调用方控制）。
  - `StateChange::MessageDelta { conversation_id: String, message_index: usize, content: String, reasoning: String, done: bool }`。
  - 远程模式事实：net server 在 Tauri 进程内复用同一 `Gateway` 与 `StateChange` 广播（[net/mod.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/mod.rs#L1-L5)）；SSE 全量推送 `StateChange`（[sse.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/sse.rs#L22-L32)）；RPC `send_chat_message` 目前阻塞 await 后 emit `Conversations`（[rpc.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/net/rpc.rs#L315-L335)）。

- 约束与风险：
  - 删除 `ToolCall` 变体是编译期强制全仓适配（`Message::text()` / `is_tool()` / `tool_calls()` / 构造点 / 投影 / 测试），但使用点仅 6 处，改动面可控；存量 `kind="tool_call"` JSON 需 `#[serde(alias)]` 兼容（`gateway.rs` 测试断言一并更新）。
  - B1 回灌条件依赖「该轮有工具调用」——统一类型后 `Text.tool_calls` 即判定依据，无变体分裂问题。
  - 流式事件频率高，前端需合并节流；`broadcast::channel(256)` 容量有限，事件积压需可丢弃语义。
  - 远程模式无需新事件通道：SSE 已全量广播 `StateChange`，`MessageDelta` 自动送达。

## Open Questions / 开放问题

- [x] Q1 落库形态是否统一为单一类型？
  - 用户回答/确认：2026-08-19 用户确认——**统一为一个变体** `Text { content, reasoning, tool_calls }`，删除 `ToolCall` 变体（`#[serde(alias = "tool_call")]` 兼容存量）。理由：模型返回在 wire 层本就是一条消息（`content` / `reasoning_content` / `tool_calls` 平级），落库无需分裂；reasoning 只属于 Text，无对称字段问题。
  - 状态：已关闭。
- [x] Q2 远程模式流式是否一起做？
  - 用户回答/确认：2026-08-19 用户确认——**一起做**。影响面评估：net server 复用同一 `Gateway` 与 `StateChange` 广播，SSE 全量推送，后端流式落地后远程前端自动收到增量；仅需 RPC `send_chat_message` 改流式入口。
  - 状态：已关闭。
- [x] Q3 增量落库节流策略？（时间节流 ~150ms + 完成时最终写，方案内给定推荐；实现细节，不强制用户确认）
  - 状态：方案内已定（见 API Design）。

## Solution Options / 方案候选

### Option A / 方案 A（推荐）

- 推荐：是
- 方案摘要：五段闭环全做——①`MessageBody::Text`/`ToolCall` 加 `reasoning`（A2 + Q1 对称扩展）；②`ModelCallResponse`/`RoundOutcome` 透传 reasoning + `parse_chat_response` 提取；③`ModelCaller::call_model_stream` trait 化 + executor/runner 流式入口 + `ConversationStore::update_last_assistant_message` 节流增量落库；④`StateChange::MessageDelta` 增量事件（本地模式 spawn + state_emit）；⑤前端 ThinkingBlock 折叠 + 流式增量合并 + markdown 兜底。
- 涉及模块：`models.rs`、`providers.rs`、`round_executor.rs`、`conversation_runner.rs`、`conversation_store.rs`、`events.rs`、`lib.rs`、`model_call_input.rs`、前端 `types.ts` / `ChatMessage.svelte` / 新增 `ThinkingBlock.svelte` / `dataStore.svelte.ts` / `MarkdownRenderer.svelte`。
- 优点：思考与正文一次到位（D2）；字段隔离保住落库与 wire 同源；流式协议层复用现成能力，避免返工；增量事件与既有事件通道共用。
- 缺点：改动面横跨后端 8 模块 + 前端 5 文件，是本次评估的「中高」复杂度；增量落库/事件需要节流与收敛，易出细节 bug。
- 风险：事件积压（broadcast 256 容量）→ 可丢弃 + 前端合并；流式与工具调用组合路径复杂 → 拆阶段验证；DeepSeek 400 场景 → Q1 对称字段兜底。

### Option B / 方案 B（不采用）

- 推荐：否
- 方案摘要：本期只做思考数据闭环（出参提取 + Text 加字段 + 折叠展示 + 回灌），流式降级为后续迭代。
- 不采用原因：用户已确认 D2（与流式一起做）；拆分会导致 Text 结构二次变更（流式接入时 ToolCall 对称字段仍要动 models.rs），返工成本高于一次性闭环。

## Decision / 方案决策

- Selected / 选定方案：Option A（统一类型 + 五段闭环 + 流式管道 + 远程模式）
- Why / 选择原因：用户已确认 D2；模型返回一条消息一个类型（Q1）；远程复用同一 Gateway 使增量事件自动覆盖（Q2）；协议层已就绪使流式成本低于预期。
- Decision Owner / 决策人：user
- Decision Time / 决策时间：2026-08-19（Q1 / Q2 已关闭）

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：扩展（非破坏性，新增字段带 `serde(default)` / `Option`；`ToolCall` 变体删除属既有内部形态收敛，`#[serde(alias = "tool_call")]` 保证存量反序列化）
- 消费方：runner / executor / store / events / net(rpc) / 前端 dataStore
- 真相源文件：`models.rs`（MessageBody / ModelCallResponse / RoundOutcome）、`events.rs`（MessageDelta）、`types.ts`（前端镜像）

### `MessageBody` 扩展（Q1 统一类型）

```rust
// models.rs —— 删除 ToolCall 变体，Text 扩展为 wire ResponseMessage 的完整镜像
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageBody {
    #[serde(alias = "tool_call")]           // 兼容存量 kind="tool_call" 数据
    Text {
        content: String,
        #[serde(default)] reasoning: Option<String>,
        #[serde(default)] tool_calls: Option<Vec<ToolCall>>,
    },
    ToolResult { /* 现有，不变 */ },
    Compaction { /* 现有，不变 */ },
    Nudge { /* 现有，不变 */ },
    RoleContext { /* 现有，不变 */ },
}
```

- 存量 JSON 缺 `reasoning` / `tool_calls` 键 → `serde(default)` 解析为 None；旧 `kind="tool_call"` → `#[serde(alias)]` 映射进 Text（`tool_calls` 字段同名直接对上）。
- `Message::text()` 返回 content（reasoning 不参与正文统计）；`Message::is_tool()` 改为「`Text { tool_calls: Some(c) if !c.is_empty(), .. }` 或 `ToolResult`」；`Message::tool_calls()` 改为 Text 分支返回 `tool_calls`。
- 新增 `Message::reasoning(&self) -> Option<&str>` 访问器（Text 返回，其余变体 None）。
- 注意：`#[serde(alias)]` 需在 serde 1.0.127+ 对 internally tagged 变体生效，落地时用 cargo test 验证存量样例（gateway.rs 测试断言更新为 Text 形态）。

### `ModelCallResponse` / `RoundOutcome` 透传

```rust
ModelCallResponse { /* 现有 */ reasoning: Option<String> }   // serde(default)
RoundOutcome { /* 现有 */ reasoning: Option<String> }
```

- `parse_chat_response`：`reasoning = choice.message.reasoning_content`（流式 `chat_stream` 已在协议层聚合进 `ChatResponse`，同一解析函数自动覆盖）。
- `persist_outcome` 纯文本分支：`Text { content: outcome.response, reasoning: outcome.reasoning, tool_calls: None }`。
- `persist_model_decl`：**改为写 Text**——`Text { content, reasoning: model_response.reasoning, tool_calls: Some(model_response.tool_calls) }`（原 ToolCall 变体删除）。

### 回灌投影（B1）

```rust
// model_call_input.rs from_message —— Text 分支（原 Text + ToolCall 两个分支合并为一个）
// Text { content, reasoning, tool_calls }：
//   注入条件：reasoning.is_some()
//              && 目标 provider 支持 reasoning（capability 判定）
//              && tool_calls.is_some_and(|c| !c.is_empty())   // 仅「有工具调用轮」回传（DeepSeek 协议）
//   满足时：assistant 消息 with_reasoning(reasoning)；tool_calls 照常投影
```

- 判定落点：`ModelCallInput` 的 Text 分支直接依据 `tool_calls` 判「是否工具调用轮」——统一类型后无变体分裂问题。
- provider 能力判定：仅 DeepSeek 系列启用（OpenAI 等忽略未知字段，不注入亦无害；保守起见仅对 `reasoning_content` 兼容 provider 注入）。

### 流式管道

```rust
// round_executor.rs ModelCaller trait
#[async_trait]
pub trait ModelCaller: Send + Sync {
    async fn call_model(&self, request: ModelCallRequest) -> AppResult<ModelCallResponse>;
    async fn call_model_stream(
        &self,
        request: ModelCallRequest,
        on_chunk: Box<dyn FnMut(openai_compat::StreamChunk) + Send>,
    ) -> AppResult<ModelCallResponse>;
}
```

- `ProviderRegistry::call_model_stream`：现有实现改为接受 `Box<dyn FnMut>` 以支持 trait object（去掉 `impl FnMut` 泛型）。
- runner 新增 `run_round_stream`（或 `run_round` 增加流式参数）：流程 = persist_input（同现状）→ 占位落库（append 空 `Text`，记 message_index）→ `executor.call_model_stream` → `on_chunk` 内累积 content/reasoning/tool_calls → 节流更新占位消息（`update_last_assistant_message`）→ 每 chunk 同步 `StateChange::MessageDelta { done: false }` → 完成后 `persist_model_decl` / `persist_outcome`（现有两段式，收敛为最终落库）→ `StateChange::MessageDelta { done: true }`。
- 工具调用轮流式：finish_reason = tool_calls 时，on_chunk 累积 tool_calls 分片（协议层已拼接）；完成后走现有 `persist_model_decl` + `execute_tools`（工具执行仍阻塞，结果照常落库）。

### 增量落库

```rust
// conversation_store.rs
pub fn update_last_assistant_message(
    &self,
    conversation_id: &str,
    patch: impl FnOnce(&mut Message),
) -> AppResult<Conversation>;
```

- 定位：`conversation.messages` 最后一条 `role == Assistant` 的消息（流式场景唯一），读改写后 `save_conversation` 全量写盘。
- 节流：调用方（runner）控制——首 chunk 立即写、此后 ~150ms 节流写、完成时最终写；写失败仅 warn（不中断流式）。
- 崩溃语义：节流窗口内丢失 ≤150ms 增量；完成后最终写保证一致。

### 增量事件

```rust
// events.rs
StateChange::MessageDelta {
    conversation_id: String,
    message_index: usize,
    content: String,      // 该消息当前累积全文
    reasoning: String,    // 该消息当前累积思考全文（空串 = 无思考）
    done: bool,           // true = 本轮完成，前端收敛为全量重拉
}
```

- `done: false` 时前端**合并**（不重拉）；`done: true` 后前端 `refreshMessages()` 收敛为权威数据。
- broadcast channel 积压：前端合并节流（rAF），后端无需确认；事件被丢弃时前端以 done 后全量重拉兜底。

### 前端契约

```ts
// types.ts —— ToolCall 变体删除，text 携带 tool_calls
| { kind: "text"; content: string; reasoning?: string; tool_calls?: ToolCall[] }

// api/types.ts StateChangePayload 新增
| { kind: "message_delta"; conversation_id: string; message_index: number;
    content: string; reasoning: string; done: boolean }
```

- 新增 `ThinkingBlock.svelte`：props `{ reasoning: string; streaming?: boolean }`，默认折叠（`expanded = $state(false)`），标题「思考过程」+ ▸/▾（参照 ToolCallBlock），流式期间自动展开、结束后收起。
- `ChatMessage.svelte`：text 分支顶部渲染 ThinkingBlock（reasoning 非空时）；`hasToolCalls` 判定改为「`body.kind === "text" && body.tool_calls?.length`」，命中时正文下方渲染 ToolCallBlock；正文 MarkdownRenderer 不变。
- `dataStore.svelte.ts`：`handleStateChanged` 增加 `message_delta` 分支——本地 `state.messages` 按 `message_index` 原地更新（合并 content/reasoning）；`done` 时 `refreshMessages()`。
- `MarkdownRenderer.svelte`：流式节流（150ms 批量 set）+ 未闭合代码块/表格兜底（渲染前用 `<pre>` 兜底或仅当文本以完整块结束才解析）。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：`requirements.md` 已确认（Q1 统一类型 / Q2 远程一起做 / B1/C1/D2）；`technical-plan.md` 已批准；Open Questions 全部关闭。
- 若执行前需求、API、范围或交互规则变化：先回写文档，再动代码。

### Step 1. 后端模型层：`models.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/models.rs`

- 改动类型：修改
- 改动内容：
  1. `MessageBody::Text` 扩展为 `Text { content: String, reasoning: Option<String>, tool_calls: Option<Vec<ToolCall>> }`（`serde(default)`）。
  2. **删除 `MessageBody::ToolCall` 变体**；`#[serde(alias = "tool_call")]` 挂到 Text 变体兼容存量。
  3. `ModelCallResponse` 增加 `reasoning: Option<String>`（`serde(default)`）。
  4. `Message::text()` / `is_tool()` / `tool_calls()` 访问器适配统一类型；新增 `Message::reasoning()`。
- 验收点：`cargo check` 全仓编译通过（ToolCall 变体 6 处使用点迁移）；存量 `kind="tool_call"` JSON 反序列化单测通过。

### Step 2. 出参提取：`providers.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/providers.rs`

- 改动类型：修改
- 改动内容：
  1. `parse_chat_response`：`reasoning = choice.message.reasoning_content.clone()` 写入 `ModelCallResponse`（流式路径同一函数，协议层已聚合）。
  2. `call_model_stream`：签名从 `impl FnMut(StreamChunk)` 改为 `Box<dyn FnMut(StreamChunk) + Send>`（trait object 兼容）。
- 验收点：非流式 + 流式各一条测试：reasoning 正确提取；无 reasoning 时 None。

### Step 3. 执行层：`round_executor.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/round_executor.rs`

- 改动类型：修改
- 改动内容：
  1. `ModelCaller` trait 增加 `call_model_stream(&self, request, on_chunk: Box<dyn FnMut(StreamChunk) + Send>) -> AppResult<ModelCallResponse>`（默认实现回退 `call_model`，供测试替身）。
  2. `RoundExecutor` 增加 `call_model_stream`（复用 `call_model` 的工具授权/投影逻辑，模型调用走流式；授权校验照旧）。
  3. `RoundOutcome` 增加 `reasoning` 透传。
- 验收点：`cargo test` 既有用例不回归（trait 默认实现保证测试替身可用）。

### Step 4. 存储层：`conversation_store.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/conversation_store.rs`

- 改动类型：修改
- 改动内容：
  1. `update_last_assistant_message(conversation_id, patch: impl FnOnce(&mut Message)) -> AppResult<Conversation>`：定位最后一条 Assistant 消息 → patch → `save_conversation`。
  2. 找不到时返回 `Ok(conversation)` 不变更（容错）。
- 验收点：单测——更新最后一条 assistant 文本；非 assistant 消息不误伤；空会话容错。

### Step 5. 事件层：`events.rs` + `lib.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/events.rs`

- 改动类型：修改
- 改动内容：`StateChange` 增加 `MessageDelta { conversation_id, message_index, content, reasoning, done }`；序列化测试。

#### 文件：`packages/pulsar-app/src-tauri/src/lib.rs`

- 改动类型：修改
- 改动内容：chat 相关 command 增加流式入口（spawn task + `state_emit(MessageDelta)`）；或 runner 内部持有 StateEmitter。确认现有 chat command 的调用链后落地。
- 验收点：`cargo test` events 序列化通过；`pnpm check` 前端类型同步。

### Step 6. Runner 流式轮：`conversation_runner.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/conversation_runner.rs`

- 改动类型：修改
- 改动内容：
  1. 新增 `run_round_stream`（复制 `run_round` 骨架）：persist_input → 占位落库（空 Text + reasoning 占位，记录 message_index）→ `executor.call_model_stream` → `on_chunk` 累积 + 节流 update（~150ms）+ emit MessageDelta → 完成后 `persist_model_decl` / `execute_tools` / `persist_outcome`（工具轮）或最终 update（纯文本轮）→ emit `MessageDelta { done: true }`。
  2. 工具调用轮：`finish_reason == tool_calls` 时停止增量正文，转现有两段式（声明 + 结果）。
- 验收点：fake ModelCaller 流式测试——纯文本轮 / 思考轮 / 工具轮三条路径；done 收敛；中断时部分内容已落库。

### Step 7. 回灌投影：`model_call_input.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/core/model_call_input.rs`

- 改动类型：修改
- 改动内容：
  1. `from_message` 删除 ToolCall 分支，统一 Text 分支：`reasoning` 仅当「provider 支持 && tool_calls 非空（有工具调用轮）」时 `with_reasoning` 回灌；纯文本轮不回灌（保持现状 None）。
- 验收点：单测——工具调用轮 reasoning 回灌；纯文本轮不回灌；无 reasoning 时行为不变。

### Step 8. 远程模式：`net/rpc.rs`

#### 文件：`packages/pulsar-app/src-tauri/src/net/rpc.rs`

- 改动类型：修改
- 改动内容：`send_chat_message` 从「阻塞 await 后 emit `Conversations`」改为「spawn 流式任务 + 快速返回 ack + 任务内 emit `MessageDelta`」；复用与本地 command 相同的 `run_round_stream` 入口。
- 验收点：RPC 单测——请求快速返回；SSE 流上收到 `message_delta` 增量与 `done: true`。

### Step 9. 前端

#### 文件：`packages/pulsar-app/src/lib/types.ts` / `packages/pulsar-app/src/lib/api/types.ts`

- 改动类型：修改
- 改动内容：`MessageBody` 删除 `tool_call` 变体、`text` 增加 `reasoning?: string` 与 `tool_calls?: ToolCall[]`；`StateChangePayload` 增加 `message_delta` 变体。

#### 文件：`packages/pulsar-app/src/lib/components/ThinkingBlock.svelte`（新增）

- 改动内容：折叠块组件（props `reasoning: string`、`streaming?: boolean`；默认收起，参照 ToolCallBlock）。

#### 文件：`packages/pulsar-app/src/lib/components/ChatMessage.svelte`

- 改动内容：text 分支顶部按 `message.reasoning` 渲染 ThinkingBlock；`hasToolCalls` 判定改为「`kind === "text" && tool_calls?.length`」，命中时正文下方渲染 ToolCallBlock。

#### 文件：`packages/pulsar-app/src/lib/stores/dataStore.svelte.ts`

- 改动内容：`handleStateChanged` 增加 `message_delta`：按 `message_index` 原地合并 content / reasoning（不重拉）；`done` 时 `refreshMessages()`。

#### 文件：`packages/pulsar-app/src/lib/components/MarkdownRenderer.svelte`

- 改动内容：节流（~150ms 批量 set）+ 未闭合代码块/表格兜底。
- 验收点：`pnpm check` 通过；人工验证流式渲染无跳动（本地 + 远程两种模式）。

### Step 10. 测试与检查

#### 命令

- 运行：`cargo test -p pulsar-app`（含新增单测）。
- 前端：`pnpm check` 确认无新增 error。
- 修复：按失败用例回改，遵守 Reverse Sync。

#### 文件：`docs/sdd-lab/2026-08-19_01-47_assistant-streaming-thinking/lifecycle.md`

- 回写执行记录、实际改动摘要、验证结果、下一步状态（draft → planned → executing → done）。

## Risk And Mitigation / 风险与缓解

- 改动面横跨 9 后端模块 + 5 前端文件：
  - 缓解：Step 1（models.rs）先行编译收敛（ToolCall 删除 + Text 扩展），后续步骤逐层推进；每 Step 有独立验收点。
- `#[serde(alias)]` 对 internally tagged 变体的兼容性：
  - 缓解：serde 1.0.127+ 支持；Step 1 单测覆盖存量 `kind="tool_call"` JSON 反序列化，失败则改用自定义 `Deserialize`。
- 事件积压 / broadcast 256 容量：
  - 缓解：`MessageDelta { done: false }` 前端 rAF 合并节流；事件可丢弃，done 后全量重拉兜底。
- 增量落库与崩溃一致性：
  - 缓解：节流 ~150ms 写盘 + 完成时最终写；中断丢失 ≤150ms 增量，无孤儿记录。
- 流式 + 工具调用组合路径复杂：
  - 缓解：runner 拆纯文本轮 / 思考轮 / 工具轮三条路径分别验证；工具执行仍阻塞（现有两段式不变）。
- DeepSeek 400（工具调用轮回灌缺失）：
  - 缓解：统一类型后 `Text.tool_calls` 非空即回灌（Q1）；Step 7 单测覆盖回灌。
- 远程模式回归：
  - 缓解：net server 复用同一 Gateway 与广播通道（Q2）；Step 8 RPC 单测 + SSE 流验证；前端 httpClient 与 tauriClient 订阅同一事件，`message_delta` 双模式透明。

## Execute Checkpoint / 执行检查点

- 当前理解：五段闭环——思考出参提取与落库（Q1 统一类型：Text 扩展为 wire 镜像 + 删除 ToolCall 变体）、按需回灌（B1）、流式管道（trait 化 + 增量落库 + 增量事件）、远程模式（Q2：复用同一 Gateway 与 SSE 广播）、前端折叠展示与增量合并（C1）、markdown 流式兜底。
- 核心目标：本地 + 远程两种模式 assistant 轮流式增量渲染（正文 + 思考分块），DeepSeek 工具链多轮无 400，存量路径不回归。
- 下一步动作：等待用户批准本技术方案后进入执行（executing）。
- 风险：横跨模块多（9 后端 + 5 前端）；单测 + 分路径验证是主要手段；`#[serde(alias)]` 兼容性需 Step 1 先行验证。
