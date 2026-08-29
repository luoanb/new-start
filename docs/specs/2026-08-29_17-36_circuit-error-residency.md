# 熔断错误驻留：对话 + 课题（Circuit Error Residency）

- 日期：2026-08-29
- 状态：执行中
- 来源：poller 熔断链路错误只进日志与内存状态机（`SessionFailureState`），对话与课题均无持久痕迹，用户无法感知课题为何停止推进。

## 目标

熔断/失败发生时，把错误「驻留」到两个持久层：

1. **对话**：落一条 `role=system` + `MessageBody::Error` 消息，前端可渲染；**不回灌模型输入**（`from_message` 跳过，历史=wire 不变式保持）。
2. **课题**：`Topic.extra.last_error = { class, message, at, consecutive_failures }`（读改写合并，不覆盖 extra 其他键）；成功恢复时清除。

## 范围与触发点

| 路径 | 对话落 Error 消息 | 课题 last_error |
|---|---|---|
| Poller 推进失败（assistant_session `PollAll` Err 分支） | ✅ 仅「首次失败」与「进入 COOLDOWN（paused）跃迁」两点落库，防刷屏 | ✅ 每次失败覆盖更新；成功清除 |
| 用户手动 converse / converse_stream / step 失败 | ✅ 失败即落（用户输入已由 persist_input 落库，错误补一条说明） | ✅ 经 reset_failure_state 清除；失败不新增（避免手动重试刷屏） |

## 涉及文件

- `core/models.rs`：`MessageBody` 新增 `Error { content, error_class }`；`text()` / `map_content()` 补分支。
- `core/model_call_input.rs`：`from_message` → `Error => None`（不回灌）。
- `core/context_safety.rs`：`ErrorClass::as_str()`。
- `core/assistant_session.rs`：poller Err/Ok 分支、converse/converse_stream/step 错误包装、`reset_failure_state` 清课题错误、辅助函数。
- 前端：`types.ts`（error body 类型）、`ChatMessage.svelte`（错误卡片渲染）、`translations.ts`（i18n key）、`TopicPanel.svelte`（last_error 标记）。

## 关键设计

- 错误消息文案：简述 + code（`AppError::code()`），完整串截断 200 字符；细节留日志。
- `error_class` 序列化为字符串（`transient` / `permanent` / `context_length_exceeded`）。
- Topic `extra.last_error` 经 `find_by_session_id` 定位课题；`TopicStore::update` 自带变更广播，前端刷新免费获得。
- 手动推进成功（`reset_failure_state`）与 poller 成功均清除 `last_error`。

## Done Contract

- 完成证明：`cargo test -p pulsar-app`（src-tauri）通过；新增 `from_message` 跳过 Error 的单测通过；前端 `svelte-check`/`vite build` 无新增错误。
- 未完成情形：Error 消息被投影进模型输入；连续失败每 tick 刷一条错误消息；extra 其他键被覆盖。

## Change Log / Validation

### 实现（2026-08-29）

按「范围与触发点」全量落地，实际改动与设计一致：

- `core/models.rs`：`MessageBody::Error { content, error_class }`（`error_class` 带 `serde(default)` 兼容存量数据）；`text()` / `map_content()` 补分支。
- `core/model_call_input.rs`：`from_message` 对 `Error` 返回 `None`（不回灌）；新增单测 `error_residency_message_is_not_refilled_into_model_input`（`from_message` 为 None + `project_history` 输出为空）。
- `core/context_safety.rs`：`ErrorClass::as_str()`（transient / permanent / context_length_exceeded）。
- `core/assistant_session.rs`：
  - `persist_error_message` / `set_topic_last_error`（读改写合并 extra）/ `clear_topic_last_error` 三个辅助方法；`error_brief` 截断 200 字符。
  - PollAll 失败分支：记录跃迁（首次失败 / 进入 paused），仅在跃迁点落对话错误消息；课题 last_error 每次失败覆盖更新；成功分支清除 last_error。
  - `converse` / `converse_stream` / `step`：失败即落一条 Error 消息后原样传播错误；`reset_failure_state` 扩展为同时清除课题 last_error。
- 前端：`types.ts` error body 变体；`ChatMessage.svelte` 错误卡片（`chatMessage.error` 标签，错误色卡片，无操作栏）；`TopicPanel.svelte` 状态行 ⚠ 徽章（title 悬停详情）+ 展开详情「最近错误」行；i18n 三语（type/en/zh）补 `chatMessage.error`、`topicPanel.lastError`。

### 验证

- `cargo test`：419 passed / 0 failed（含新增单测）。（命令尾部 exit code 1 为沙箱管道限制，非测试失败。）
- 前端：`npm run check`（svelte-check）被沙箱 snap 权限问题阻断（环境问题）；改用 IDE 诊断——`ChatMessage.svelte` / `TopicPanel.svelte` / `types.ts` / `translations.ts` 均无诊断错误。
- Done Contract 核对：Error 不进模型输入 ✅（编译期 match 强制 + 单测）；连续失败不刷屏 ✅（仅跃迁点落库）；extra 其他键不覆盖 ✅（读改写合并）。

### 结论

核心目标已由证据证明完成。遗留（非本任务范围）：svelte-check 需在非沙箱环境复跑一次作为补充确认。
