# Spec: 修复空 System 消息导致模型请求被拒

## Goal

- 修复 `Send failed: [invalid_input] Model message content cannot be empty`：无系统提示的会话（chat/agent/assistant 通用）在历史非空后再次发送时，组装出的模型请求包含一条 `content=""` 的 System 消息，被 `providers.validate_request` 拒绝。
- 保证组装结果中**不存在空 content 的非 tool_call 消息**。

## 根因

[model_call_input.rs `replace_system`](file:///home/lab/Documents/trae_projects/new-start-wt/packages/agent-app/src-tauri/src/core/model_call_input.rs#L32-L44)：`assemble` 传参 `role_system` 为空（会话历史无 system 消息）且 `history` 非空时，`replace_system` 仍构造 `content=""` 的 System 消息并 prepend 到历史最前；随后 `providers.validate_request` 对非 tool_call 消息强制 content 非空 → 整次请求被拒。

触发条件（与本 bug 一致的完整链路）：
- `assemble` 两条路径：历史为空 → 用户输入折进 System（非空，成功）；历史非空 → `replace_system(history, "")` → 插入空 System（失败）。
- `ModelAppendTemplate::Neuron` 仅是用户消息排版模板，与真实神经元数据无关；engine.chat 对 chat/agent 统一复用该常量。

## Done Contract

- `replace_system(history, "")`：不插入空 System 消息；顺手移除历史中残留的空 content System 消息（防御存量），**保留非空 System**（如压缩摘要，避免破坏上下文）。
- `replace_system(history, 非空)`：行为不变（替换或 prepend 非空 System）。
- 新增单测：空 system_prompt 不产生空 System 消息；空 system_prompt 时保留非空 System。
- `cargo test` 通过。

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/model_call_input.rs` | `replace_system` 开头对空 `system_prompt` 提前返回：过滤掉空 content 的 System 消息，其余原样返回 |

## 兼容性

- 唯一调用方为 `assemble`（engine chat/agent + assistant_mode 共用），一处修复三处生效；存量会话立即恢复发送。
- 非空 system_prompt 路径零行为变化；现有单测（`replace_system_on_empty_history_prepends` 等）不受影响。

## Validation

- `cargo test` 通过（含新增单测）。
- 手动：复现会话（agent 模式、历史非空后发送）不再报 `Model message content cannot be empty`。

## Change Log / Validation（2026-08-09）

- `cargo test`：147 passed, 0 failed（原 145 + 新增 2 个 replace_system 空 system_prompt 单测）。
- 实现摘要：
  - `model_call_input.rs replace_system`：`system_prompt` 为空时提前返回——过滤掉历史中空 content 的 System 消息、保留非空 System（如压缩摘要），不再 prepend 空 System 消息；非空路径行为不变。
  - 新增单测：`replace_system_with_empty_prompt_never_inserts_empty_system`（空提示不插入空 System）；`replace_system_with_empty_prompt_drops_empty_system_keeps_nonempty`（清理残留空 System、保留压缩摘要）。
- 影响面：`assemble` 唯一调用方为 engine chat/agent + assistant_mode，一处修复三处生效；存量会话（历史非空、无系统提示）发送立即恢复。
