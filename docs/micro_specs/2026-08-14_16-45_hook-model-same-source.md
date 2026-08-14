# Spec: Assistant hook 与主对话模型同源（方案 A）

## Goal

- 要解决什么问题：assistant 模式下，hook 裁决类（score_feedback / match_topic / complete_scope）用 `default_model_or_error()`（配置默认模型），而主对话用前端传入的 model。默认模型余额不足时，match_topic 第一个模型调用即 402，整个 round 失败，即使主对话选的模型正常。
- 验收结果：三个 hook 与主对话使用同一个 model（用户所选），不再读配置默认；`default_model_or_error` 删除。

## Done Contract

- 什么算完成：
  1. `RoundContext` 增加 `model: ChatModelSelection`
  2. `load_context` 接收 `model` 参数并写入 ctx；`reload` 保持 ctx.model 不变
  3. `score_feedback` / `match_topic` / `complete_scope` 改用 `&ctx.model`
  4. `default_model_or_error` 删除（仅被这 3 处使用）
  5. cargo test 全部通过
- 由什么证明：cargo test 通过
- 哪些情况仍算未完成：任一改动未完成、测试失败、残留 default_model_or_error 引用

## Scope

- In:
  - `conversation_runner.rs` — `RoundContext.model`、`load_context` 签名、`run_round` 传参
  - `assistant_session.rs` — 3 个 hook 改用 ctx.model、删除 default_model_or_error
- Out:
  - 选型模型（NeuronModelCaller）不变
  - 方案 B（hooks 独立模型配置）不做

## Facts / Constraints

- `ChatModelSelection` 已 `derive(Clone)`，可安全存入 RoundContext
- `run_round` 已接收 `model: &ChatModelSelection`，只是未传入 hooks
- `reload`（会话切换）只重建 seed/state/messages，model 应保持本轮所选

## Restated Understanding

- 我理解当前任务是：让 assistant 的 hook 裁决调用与主对话共用用户选择的模型，修复默认模型余额不足导致整体失败的问题。
- 当前核心目标是：完成方案 A，cargo test 通过。
- 暂不处理：方案 B 独立模型配置。

## Checkpoint Summary

- 当前任务理解：方案 A，hook 模型同源
- 当前核心目标：修复 402 根因
- 当前进度：spec 落盘中
- 下一步 1: 用户批准
- 下一步 2: 实现（conversation_runner → assistant_session）
- 下一步 3: cargo test
- 涉及文件：conversation_runner.rs / assistant_session.rs
- 风险：低（仅模型来源变更）
- 验证方式：cargo test
- Execution Approval: `Pending`

## Change Log

- 2026-08-14: 初版 spec
- 2026-08-14: 实现完成
  - `RoundContext` 新增 `model` 字段；`load_context` 接收并写入（`reload` 保持不变）
  - `score_feedback` / `match_topic` / `complete_scope` 改用 `&ctx.model`，不再读配置默认
  - 删除 `default_model_or_error`、`AssistantSession.providers` 字段、gateway 对应传参

## Validation

- Static checks: cargo build 通过（无未使用字段/import 警告）
- Runtime / Test: cargo test 209 passed, 0 failed
- Human confirmation:
- 结果汇总：Done Contract 5 项全部完成
- 核心目标是否已由证据证明完成：是，209 测试通过

## Resume / Handoff

- 当前状态：spec 草稿
- 下一步唯一动作：用户批准后实现
- 下一轮核心目标：完成方案 A 并测试通过
