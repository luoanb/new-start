# Spec: 轮询简报落库（nudge kind）

## Goal

- 把轮询推进时发给模型的输入（课题简报 topic_brief）落库为会话对话记录，新增 `kind=nudge` 消息类型。
- 解决历史缺失：以前轮询的模型输入只临时拼入本轮调用、不落库，导致对话历史里只有产出（assistant/tool）没有输入，因果链不完整、不可审计。

## Done Contract

- `MessageBody` 新增 `Nudge { content }`（serde tag = `nudge`），存量 JSON 无迁移。
- 轮询（`RoundTrigger::Poller`）实际执行时，简报以 `role=User, kind=nudge` 落库，插入位置早于该轮产出消息。
- 落库的 nudge **不参与后续模型输入组装**（`message_to_model` 过滤），避免历史简报反复进 context。
- 前端按 `kind === "nudge"` 灰显折叠渲染。
- `cargo test` 通过；前端 `svelte-check` / 构建通过。

## 背景事实（决定落库判据）

- Poller 每 `base_interval * assistant_interval_ticks`（默认 5s）**触发**一次 PollAll，但 `step_guard`（try_lock）串行丢弃在途请求，**实际执行频率**取决于 step_poller 模型调用耗时（秒~分钟级），远低于 5s。
- 现有 `run_core` 对模型返回**无条件**落 assistant 消息（无空输出跳过）——本 spec 不改动该行为，仅新增 nudge 落库。

## 落库判据

- `run_core` 中，当 `ctx.trigger == RoundTrigger::Poller` 时，将本轮 `user_input`（即 topic_brief，含 fallback 文案）落库为一条 `role=User, kind=nudge`。
- 时机：模型调用前（`user_input` 计算完成后、`assemble` 前），保证 nudge 位于该轮产出之前。
- 模型调用失败时 nudge 仍保留（记录"发起过推进"），属预期审计行为。

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/models.rs` | `MessageBody` 新增 `Nudge { content }`；`Message::text()` 补分支 |
| `src-tauri/src/core/assistant_mode.rs` | `run_core` Poller 分支落 nudge；`message_to_model` 的 `Nudge` → `None`（不拼回） |
| `src-tauri/src/core/engine.rs` | `message_to_model_message` 补 `Nudge` → User 文本（engine 会话不会实际产生 nudge，仅穷尽性） |
| `src/lib/types.ts` | `MessageBody` union 加 `{ kind: "nudge"; content: string }` |
| `src/lib/components/NudgeBlock.svelte` | 新增折叠块组件（仿工具块样式）：默认收起，点击展开全文 |
| `src/lib/components/ChatMessage.svelte` | `kind === "nudge"` 渲染 `NudgeBlock`，容器左对齐、bubble 透明 |

## 兼容性

- 存量 JSON：无 nudge 字段，读取不受影响。
- 模型协议：nudge 不发给模型（组装层过滤）。
- 压缩：`compaction_prompt` 按 `role` + `Message::text()` 处理，nudge 自然作为 `[user]` 参与摘要（`text()` 补分支后无需其他改动）。
- `matches!` 类方法（`is_tool` / `summary_of` / `tool_calls`）：nudge 自然返回 false/None，无需改动。

## Validation

- `cargo test --lib` 通过（含新增 nudge 相关单测）。
- 前端 `pnpm --filter agent-app build` 通过。
- 手动/日志验证：Poller 推进后会话历史出现 `kind=nudge` 消息且早于产出。

## Change Log / Validation（2026-08-08）

- `cargo check` 通过；`cargo test`：145 passed, 0 failed。
- `pnpm --filter agent-app check`：0 errors（47 warnings 均为既有，与本次改动无关）。
- `pnpm --filter agent-app build`：构建成功。
- 实现摘要：
  - `models.rs`：`MessageBody::Nudge { content }` + `Message::text()` 分支。
  - `assistant_mode.rs`：`run_core` 在 `RoundTrigger::Poller` 时落库 `role=User, kind=nudge`（简报与模型实际输入一致，含 fallback）；`message_to_model` 的 `Nudge` → `None`（不拼回模型输入）。
  - `engine.rs`：`message_to_model_message` 补 `Nudge` → User 文本（穷尽性，engine 会话不会实际产生）。
  - 前端：`types.ts` union；`NudgeBlock.svelte` 折叠块组件（仿 ToolCall/ToolResult 样式，默认收起、展开看全文）；`ChatMessage.svelte` 分发渲染；i18n 文案（zh `轮询推进` / en `polling advance`）。
- 未纳入（与本次诉求无关的独立议题，留待后续）：PollAll 跳过已运行会话（对话期间轮询介入的并发竞态）；run_core 空输出跳过 assistant 落库。
