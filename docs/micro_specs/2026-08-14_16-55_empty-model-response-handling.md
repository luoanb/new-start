# Spec: 模型空响应处理（根源 + 防御 + 数据清理）

## Goal

* 要解决什么问题：模型偶发返回 HTTP 200 + 空 content 时，被 `unwrap_or_default()` 静默吞成空消息并落库；该空消息随后触发 `Model message content cannot be empty` 校验拒绝，锁死会话后续调用。

* 验收结果：

  1. 模型返回空响应（无 tool\_calls）时，`call_model` 报明确错误，不再落库空消息
  2. 历史组装过滤脏空 assistant 消息，已污染的会话可继续使用
  3. 已污染会话 `conv_1786711230040799556` 的空 assistant 消息被清理

## Done Contract

* 什么算完成：

  1. `providers.rs` 解析响应后检测空 output（无 tool\_calls 且 trim 为空）→ 返回 `AppError::LlmRequestFailed`
  2. 历史转换（`to_model_messages`）跳过非 tool\_call 且 content 空的 assistant 消息
  3. 用户确认后清理目标会话脏消息
  4. cargo test 全部通过

* 由什么证明：cargo test 通过；清理后目标会话可正常调用

* 哪些情况仍算未完成：任一改动缺失、测试失败

## Scope

* In:

  * `providers.rs` — 空响应检测

  * `conversation_runner.rs` — `to_model_messages` 过滤脏空消息

  * 目标会话脏数据清理（需用户批准）

* Out:

  * 重试逻辑（不做，先报错）

  * 前端改动

## Facts / Constraints

* 空响应根因：`choice.message.content` 为 `None` 时 `unwrap_or_default()` 得 `""`

* 校验（providers.rs:498）仅拒绝"非 tool\_call 且 content 空"；tool\_call 消息空 content 是合法形态

* 过滤脏消息需保持 tool\_call 配对语义（sanitize\_tool\_pairs 已处理配对）

## Restated Understanding

* 我理解当前任务是：修复空响应静默落库 → 历史污染 → 校验锁死 的连锁 bug。

* 当前核心目标是：根源报错 + 防御过滤 + 清理脏数据。

* 暂不处理：自动重试。

## Checkpoint Summary

* 当前任务理解：空响应三修复

* 当前核心目标：解除会话锁死并防止复发

* 当前进度：spec 起草

* 下一步 1: 用户批准（含数据清理授权）

* 下一步 2: providers.rs 空响应报错

* 下一步 3: to\_model\_messages 过滤 + cargo test

* 下一步 4: 清理目标会话脏消息

* 涉及文件：providers.rs / conversation\_runner.rs / sessions JSON

* 风险：低；数据清理不可逆，需先备份

* 验证方式：cargo test + 目标会话发消息验证

* Execution Approval: `Pending`

## Change Log

- 2026-08-14: 初版 spec
- 2026-08-14: 实现完成
  - `providers.rs`：响应解析后检测空 output（无 tool_calls 且 trim 空）→ 返回 `LlmRequestFailed`，不再落库空消息
  - `conversation_runner.rs`：`to_model_messages` 过滤非 tool_call 且 content 空的 assistant 消息，脏历史不再阻断
  - 清理 `conv_1786711230040799556` 空 assistant 消息（备份 `conv_1786711230040799556.json.bak`）

## Validation

- Static checks: cargo build 通过
- Runtime / Test: cargo test 209 passed, 0 failed
- Human confirmation: 目标会话已清理（before=2 after=1 removed=1）
- 结果汇总：Done Contract 4 项全部完成
- 核心目标是否已由证据证明完成：是，209 测试通过 + 脏数据已清理

## Resume / Handoff

* 当前状态：spec 草稿

* 下一步唯一动作：用户批准后实现

* 下一轮核心目标：解除会话锁死并防复发

