# Requirements / 需求文档: assistant-streaming-thinking

## Restated Understanding / 需求复述

- 我理解当前需求是：assistant 轮次响应从「阻塞等待 → 一次落库 → 前端整段刷新」改为「流式增量渲染」，同时采集推理模型（DeepSeek 等）的思维链 `reasoning_content`：**出参提取**（不再丢弃）、**落库持久化**（消息体字段隔离，落库与 wire 同源）、**前端折叠展示**（默认收起）、**入参按需回灌**（仅 DeepSeek 且有工具调用轮回传，防 400）。
- 当前核心目标是：三条轨道落地——①后端数据闭环（A2 字段 + B1 回灌）；②流式管道（`call_model_stream` 接入 runner、增量落库、增量事件）；③前端增量渲染（思考折叠块 + 流式 markdown）。
- 当前边界是：协议层 SSE 与 `reasoning_content` 解析已就绪（`openai_compat.rs` `chat_stream` / `StreamDelta.reasoning_content`）；`call_model_stream` 已在 `providers.rs` 但 `dead_code`，`round_executor` 未接入；`parse_chat_response` 只取 content、丢弃 reasoning；`ConversationStore` 仅 append 无 update；事件通道为完成时一次 `StateChange::Conversations` 全量重拉。
- 暂不处理：远程模式（HTTP server）下的模型流式执行；代码高亮 / markdown 渲染器升级；流式中断的断点续传；多 provider 思考字段（`reasoning_details` / signature 等）统一。

## Scope / 范围

### In

1. **出参提取**
   - `ModelCallResponse` 新增 `reasoning: Option<String>`；`parse_chat_response` 从 `ResponseMessage.reasoning_content` 提取（非流式），流式走 `chat_stream` 已累积的 reasoning。
   - `RoundOutcome` 透传 `reasoning`；`persist_outcome` / `persist_model_decl` 落库时写入消息体。

2. **落库形态（统一类型，Q1 已关闭）**
   - **模型返回统一为一个变体**：`MessageBody::Text` 扩展为 wire `ResponseMessage` 的完整镜像——`Text { content, reasoning: Option<String>, tool_calls: Option<Vec<ToolCall>> }`，**删除 `ToolCall` 变体**（用户决策：模型返回本就是一条消息，一个类型就够了）。
   - 字段隔离，非字符串拼接（理由：落库与 wire 同源，回灌需机器可读拆分）；`Message::text()` 仍只返回 content（reasoning 不进正文统计）。
   - 兼容：存量 JSON 无 `reasoning` / `tool_calls` 键 → `serde(default)` 解析为 None；旧 `kind="tool_call"` 数据经 `#[serde(alias)]` 映射进 `Text` 变体。

3. **入参回灌（B1，用户已确认）**
   - 按需回传：仅当消息携带 `reasoning` 且满足回灌条件（provider 支持 + 该轮存在工具调用）时，投影 `ModelMessage.reasoning_content`（现有 `with_reasoning` 落点）。
   - 无工具调用轮不注入 reasoning（DeepSeek 官方：未工具调用轮传入会被忽略；OpenAI 等忽略未知字段，行为无害）。

4. **流式管道**
   - `ModelCaller` trait 扩展 `call_model_stream`（回调 `on_chunk`）；`round_executor` 增加流式入口；`conversation_runner` 增加流式轮路径。
   - 增量落库：`ConversationStore` 新增 update 语义（定位最后一条 assistant 消息 + 节流写盘），避免每 chunk 全量写。
   - 增量事件：`StateChange` 新增 delta 变体（携带 conversation_id / message_index / content / reasoning / done），前端订阅增量更新，完成时收敛为全量重拉。

5. **前端增量渲染（C1，用户已确认）**
   - `MessageBody` 前端类型同步（`text` / `tool_call` 增 `reasoning?`）。
   - 新增 `ThinkingBlock` 折叠组件（默认收起，参照 ToolCallBlock / NudgeBlock 模式）。
   - 流式期间消息气泡内同时增量更新思考块与正文；markdown 渲染节流 + 未闭合代码块兜底。

6. **远程模式流式（Q2 已关闭：一起做）**
   - net server 复用同一 `Gateway`、SSE 复用同一 `StateChange` 广播通道——后端流式落地后远程前端经 SSE 自动收到增量事件。
   - 增量工作：RPC `send_chat_message` 改为流式入口（spawn + 快速返回 + `MessageDelta` 事件）；前端 httpClient 已订阅 `STATE_CHANGED_EVENT`，`dataStore` 处理 `message_delta` 即对两种模式透明。

### Out

1. markdown 渲染器升级（语法高亮 / 复制按钮）——本期只做流式正确性兜底。
2. 非 DeepSeek 推理模型的思考回传合规（`reasoning_details` / thinking blocks + signature 等）——本期仅抹平「出参提取 + 存储」，回灌仅 DeepSeek 路径。
3. 多轮流式状态机的取消 / 断点续传 / 会话恢复（本轮结束后再评估）。
4. TUI（ratatui）展示。

## User Interaction / 用户交互

- 触发入口：现有 Chat / Assistant 主对话，无新入口。
- 用户操作路径：
  1. 发送消息 → 等待窗口出现思考折叠块（标题 + 展开按钮）→ 思考文本与正文随流式增量出现 → 结束后思考块默认收起，正文完整展示。
  2. 推理模型多轮工具调用（DeepSeek）：思考内容完整回传，无 400 报错，工具调用链正常。
- 系统反馈：流式期间事件增量推送；结束后 `StateChange::Conversations` 收敛刷新。
- 异常/边界交互：
  - 模型只出 reasoning 不出正文（推理模型典型行为）：正文区保持空/loading，思考块完整展示，不触发「空响应防御」误报。
  - 流式中断/失败：已增量落库的部分保留（节流写盘），失败轮不产生孤儿记录。
  - 旧消息（无 reasoning 键）：思考块不渲染，行为与现状一致。
- 不应发生的交互：
  - 思考文本混入正文（wire 投影必须字段隔离）。
  - DeepSeek 有工具调用轮回灌缺失 → 400（B1 必须覆盖 ToolCall 变体，见开放问题 Q1）。
  - 流式期间整条消息反复全量重拉（造成滚动跳动与选中丢失）。

## Acceptance Criteria / 验收标准

1. **出参提取与落库**
   - [ ] 推理模型响应后，`reasoning` 提取并随消息体落库；存量消息无 reasoning 时解析兼容。
   - [ ] 模型只出 reasoning 不出正文时，不触发空响应防御误报；思考块正常展示。

2. **回灌（B1）**
   - [ ] DeepSeek 多轮 + 工具调用路径：`reasoning_content` 正确回传，无 400；OpenAI 等 provider 不注入该字段（行为无害）。
   - [ ] 无工具调用轮不注入 reasoning。

3. **流式管道**
   - [ ] 本地模式：assistant 响应增量呈现（正文 + 思考分块更新），期间不整条重拉、不丢选区。
   - [ ] 远程模式：SSE 收到 `message_delta` 增量事件并正确渲染（与本地模式同一事件结构）。
   - [ ] 增量落库：流式中断后部分内容仍在会话文件中；完成后最终内容落库一致。
   - [ ] `StateChange` delta 事件与收敛刷新正确；既有 `Conversations` 事件路径不回归。

4. **前端折叠展示（C1）**
   - [ ] 思考块默认收起、可展开/收起；展开后显示完整思考文本；无思考时零渲染。
   - [ ] 正文 markdown 渲染在流式下不跳动（未闭合代码块兜底 + 节流），结束后渲染稳定。

5. **回归**
   - [ ] 非推理模型路径（无 reasoning）行为与现状一致；工具调用链（工具声明与结果独立落库、`kind="tool_call"` 存量数据可解析）不回归；Assistant 模式 / Poller / Nudge 路径不回归。
   - [ ] `cargo test -p pulsar-app` 全量通过；前端 `pnpm check` 无新增 error。

## Constraints / 约束

- 业务约束：
  - `Spec is Truth`：文档与代码冲突时，先修正文档再修代码；`Reverse Sync`：发现偏差先回写文档。
  - 落库与 wire 同源（现有 Nudge / RoleContext 注释确立的投影原则）：消息体字段必须与 wire 字段一一对应。
  - 思考内容属过程信息，默认折叠不抢占版面；不污染正文统计（`Message::text()` 语义不变）。
- 技术约束：
  - 协议层 `chat_stream` / `StreamDelta` 已就绪，不改协议层聚合逻辑；只接管道。
  - 增量落库复用 `ConversationStore` 现有 JSON 文件存储（会话文件全量读写），节流控制写盘频率，不引入新存储。
  - 事件沿用 `STATE_CHANGED_EVENT` 统一通道（Tauri emit / SSE 共用事件名），增量 delta 为新增 kind，不破坏既有 kind 解析。
  - 前端双客户端（tauriClient / httpClient）订阅同一事件，delta 事件对两种模式透明（远程模式不执行流式，但事件结构一致）。
  - `ModelCaller` trait 扩展为可测试（测试替身同步实现）。

## Referenced Designs / 引用设计稿

> 无。本迭代不涉及 Figma / 视觉稿。

## Open Questions / 开放问题

- [x] Q1 落库形态是否统一为单一类型？（ToolCall 有自己的 reasoning 吗）
  - 触发来源：A2 拆分 Text / ToolCall 两个变体，B1 回灌条件（有工具调用轮）恰好落在 ToolCall 变体，需对称加字段。
  - 用户回答/确认：2026-08-19 01:5x 用户确认——**模型返回本就是一条消息（wire 层 `content` / `reasoning_content` / `tool_calls` 平级），落库统一为一个变体**：`Text { content, reasoning, tool_calls }`，删除 `ToolCall` 变体（`#[serde(alias = "tool_call")]` 兼容存量）。reasoning 只属于 Text，无对称字段问题。
  - 状态：已关闭。
- [x] Q2 远程模式流式是否一起做？
  - 用户回答/确认：2026-08-19 01:5x 用户确认——**一起做**。影响面评估：net server 复用同一 `Gateway` + 同一 `StateChange` 广播（SSE 全量推送），后端流式落地后远程前端自动收到增量；仅需 RPC `send_chat_message` 改流式入口。
  - 状态：已关闭。
- [x] Q3 增量落库节流策略？（时间节流 ~150ms + 完成时最终写，方案内给定推荐；实现细节，不强制用户确认）
  - 状态：方案内已定（见 API Design）。

## Requirement Decisions / 需求决策

- 2026-08-19 01:47:
  - 决策：落库形态统一为单一类型（Text 扩展为 wire 镜像 + 删除 ToolCall 变体）；回灌 B1——按需回传（仅 DeepSeek 且有工具调用轮）；展示 C1——默认折叠块；范围 D2——与流式一起做；远程模式一起做。
  - 原因：字段隔离保住落库与 wire 同源；模型返回一条消息一个类型，避免为 reasoning 做变体对称；按需回传符合 DeepSeek 官方协议且避免 token 浪费；折叠块是业界惯例；流式是增量渲染的前置，D2 一次到位避免返工；远程复用同一 Gateway 使增量事件自动覆盖。
