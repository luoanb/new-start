# Spec: 修正「至多一次工具执行」语义——一轮内多工具全执行

## Goal

- 修正「至多一次」的理解与实现偏差：该约束指**每个轮次最多一轮工具执行**（由 `run_round` 结构保证，无循环），而**一轮内模型声明的多个 tool_calls 应全部执行、全部落库**（并行工具调用受支持）。
- 当前实现 `take(1)` 只执行首个 tool_call，丢弃其余声明，是语义偏差。

## 现状与问题

`call_service.rs` service_converse（模型返回后）：

```rust
let tool_calls = model_response.tool_calls
    .as_ref()
    .map(|calls| calls.iter().take(1).cloned().collect::<Vec<_>>());
```

- 模型声明 N 个 tool_calls（并行调用），引擎只执行并落库第一个，其余声明被截断丢弃。
- 「至多一次」被误读为「一次只执行一个工具」。正确语义：一轮 = 一次 model 调用 + 一次工具阶段；工具阶段内 N 个声明全执行。
- 佐证：agent 模式已有循环（`agent_session` 按「本轮是否产生 Tool 结果」决定是否进入下一轮 run_round），即每轮已天然限定「最多一轮工具执行」，工具本身的数量不受限。

## 方案

`RoundOutcome.tool_result: Option<String>` → `tool_results: Vec<ToolResultItem>`（每项 = 一条已执行工具的结果）；service 遍历全部 tool_calls 执行；runner 落库为 1 条 Assistant 声明 + N 条 Tool 结果（`tool_call_id` 配对）。

## Done Contract

- `call_service.rs`：
  - 新增 `pub struct ToolResultItem { tool_call_id, tool_name, content }`（与 RoundOutcome 同处）。
  - `RoundOutcome.tool_result: Option<String>` → `pub tool_results: Vec<ToolResultItem>`；注释改为「本轮全部工具执行结果（一轮内多工具并行执行）」。
  - service_converse 工具段：删除 `take(1)`；遍历全部 `tool_calls`：
    - 逐个授权检查——任一未授权 → `InvalidInput`（声明了就必须全执行，保持「未授权拒绝」语义）；
    - 逐个 `tool.execute(args)`，结果收集进 `tool_results`；
    - `output` 逐条拼接 `[tool:{name}] {result}`（空 output 时以首条结果开头）。
  - `tool_calls` 产物携带全部声明（不再截断）。
- `conversation_runner.rs` 落库：Assistant `ToolCall` 落全部声明（已有 `tool_calls.clone()`，不变）；遍历 `outcome.tool_results` 为每条落一条 `Tool` `ToolResult{tool_call_id, tool_name, content}`；`stored_as` 判断不变（tool_calls 非空即 tool_call + tool_result）。
- `assistant_session.rs` complete_scope hook：`outcome.tool_result` → `outcome.tool_results`（JSON 传全部结果，供裁决模型参考）。
- 测试：
  - `call_service.rs` L1315 断言：`outcome.tool_result` → `outcome.tool_results`。
  - 新增用例：mock 模型一次返回多个 tool_calls（如 core_echo ×2）→ 断言全部执行、`tool_results.len()==N`、授权拒绝任一即整轮报错。
  - 保持既有标签注入断言不变。
- 注释同步：service_converse 单次语义注释改为「单轮单次工具阶段：模型可能声明多个 tool_calls（并行调用），引擎**全部执行**；产物携带全部声明与全部结果，落库后 assistant 声明与 tool 结果一一配对（sanitize 要求每个声明都有对应结果）」。

## 兼容性

- `sanitize_tool_pairs`（model_call_input）已按「assistant 声明的全部 tool_call_id 都被 tool 消息应答才保留声明」配对——全部声明 + 全部结果落库后天然完整，无需改动。
- OpenAI wire：每条 Tool 消息带对应 `tool_call_id`，满足「tool must be a response to preceding tool_calls」。
- agent 循环收敛判定 `last_message_is_tool_result` 按 role=Tool 判断，多结果（多条 Tool 消息）仍返回 true，行为不变。
- `RoundOutcome` 为内存结构，无序列化迁移。

## Validation

- `cargo test`（218+ 用例 + 新增多工具用例）全部通过。
- 可选冒烟：真实服务（deepseek-v4-flash）一次提示触发多工具声明，确认 N 条结果全部落库。

## 改动点

| 文件 | 改动 |
|---|---|
| `src/core/call_service.rs` | `ToolResultItem` 新增；`RoundOutcome` 字段替换；service_converse 遍历执行全部 tool_calls；测试断言更新 + 新增多工具用例 |
| `src/core/conversation_runner.rs` | 落库循环 tool_results；注释 |
| `src/core/assistant_session.rs` | complete_scope JSON 传全部工具结果 |
| `docs/pulsar/e2e-assistant-test-cases.md` 等 | A-06 语义描述修正（Reverse Sync） |

## Change Log / Validation（2026-08-16）

- 实现摘要：
  - `call_service.rs`：新增 `ToolResultItem{tool_call_id, tool_name, content}`；`RoundOutcome.tool_result: Option<String>` → `tool_results: Vec<ToolResultItem>`；service_converse 删除 `take(1)`，遍历全部 tool_calls 逐个授权（任一未授权 → `InvalidInput`）+ 逐个执行；产物携带全部声明与全部结果。
  - `conversation_runner.rs`：落库 Assistant 声明携带全部 tool_calls；遍历 `tool_results` 逐条落 Tool 消息（`tool_call_id` 配对）。
  - `assistant_session.rs`：complete_scope 裁决 JSON 的 `tool_result` → `tool_results`（全部结果）。
  - 测试：mock `EchoCaller` 支持多个 tool_calls；新增 `converse_executes_all_declared_tools`（两条声明全执行、结果配对）；原授权/拒绝用例语义保持。
  - `architecture.md` L279 节点措辞同步为「单轮工具阶段：多个 tool_calls 全部执行」。
- 验证：`cargo check` 通过；`cargo test`：219 passed, 0 failed（新增多工具用例通过）。
