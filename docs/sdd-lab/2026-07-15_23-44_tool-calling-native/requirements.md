# Requirements / 需求文档: tool-calling-native

## Restated Understanding / 需求复述

- 我理解当前需求是：将 `Agent` 中通过 system prompt 指令 + 正则 `/\{[\s\S]*\}/` 解析 JSON 的手搓工具调用方式，替换为 OpenAI SDK 的 `chat.completions.create` 原生 `tools` 参数。
- 当前核心目标是：所有 4 个服务商（OpenAI / DeepSeek / Ollama / Custom）统一使用标准 `tools` 格式，LLM 返回结构化的 `tool_calls`，由 SDK 直接解析，不再依赖正则。
- 当前边界是：仅改造 `types.ts`、`skills.ts`、`agent.ts` 三个文件，不涉及 `gateway.ts`、`memory.ts`、`index.ts` 的接口变化。
- 暂不处理：`strict: true` 模式、Responses API 迁移、streaming、tool_choice 强制指定特定工具、tool 结果截断/分块。

## Scope / 范围

- In:
  - `Skill` 接口新增 `parameters` 字段（JSON Schema Object）
  - 三个内置 Skill（`get_current_time` / `calculate` / `echo`）补上对应的 JSON Schema
  - `SkillManager` 新增 `toTools()` 方法，将全部 Skill 转为 `ChatCompletionTool[]`
  - `Agent` 删除 `parseSkillCall()` 方法和正则逻辑
  - `Agent` 删除 `buildSystemPrompt()` 中"请返回 JSON"的大段指令
  - `Agent.process()` 改用 `tools` + `tool_choice: "auto"` 调用
  - `Agent.process()` 支持模型返回多个 `tool_calls` 时并行执行
  - 用 `role: "tool"` + `tool_call_id` 回传结果给 LLM 二次生成回复
- Out:
  - 不修改 `Gateway`、`Memory`、`CLI 入口` 的三方调用方式
  - 不引入 `strict: true`
  - 不引入 streaming
  - 不引入强制 `tool_choice`（始终用 `"auto"`）
  - 不修改旧 provider 配置逻辑

## User Interaction / 用户交互

- 触发入口：用户通过 CLI 输入问题，触发 `Gateway.sendMessage()` → `Agent.process()`。
- 用户操作路径：无变化。用户仍通过 CLI 输入问题、查看返回。
- 系统反馈：
  - 当 LLM 决定调用工具时，Agent 自动执行并返回结果，用户不会感知到"正则解析"的中间过程。
  - 当 LLM 一次返回多个 `tool_calls` 时（如同时要时间+计算），Agent 并行执行后统一回复。
- 状态变化：无。工具调用对用户透明。
- 异常/边界交互：
  - LLM 返回了无法解析的 `tool_calls` — SDK 已经做了类型保障，`tool_calls` 是结构化的，不会出现当前正则匹配失败的情况。
  - 某个不支持 `tools` 的 `custom` 提供商 — 保留回退能力（方案待技术方案阶段细化）。
- 不应发生的交互：
  - Agent 因正则匹配失败导致静默跳过工具调用。
  - Agent 因 `tool_calls` 的 `arguments` 不是标准 JSON 结构而崩溃。

## Acceptance Criteria / 验收标准

- [ ] `Skill` 接口新增 `parameters` 字段，已有 Skill 实例不因缺少该字段报错。
- [ ] `SkillManager.toTools()` 能正确输出 `ChatCompletionTool[]` 格式。
- [ ] `agent.ts` 不再包含 `parseSkillCall()` 方法和同功能函数。
- [ ] `agent.ts` 不再在 system prompt 中包含"请返回 JSON"类指令。
- [ ] `process()` 在 LLM 返回 `tool_calls` 时能正确解析并并行执行多个 Skill。
- [ ] `process()` 在 LLM 没有调工具时正常返回文本。
- [ ] 三种内置 Skill（时间 / 计算 / 回显）在改造后都能被 LLM 正确命中并执行。
- [ ] `pnpm build` 编译通过，无 TypeScript 错误。
- [ ] `pnpm dev` 启动正常。

## Constraints / 约束

- 技术约束：
  - 保持与 OpenAI SDK v4（当前 `^4.80.1`）的兼容性。
  - `tools` 参数格式必须与 DeepSeek / Ollama 兼容（即标准 JSON Schema `properties` + `required` 模式）。
  - 不引入额外 npm 依赖。
  - 不改 `Gateway` 对外接口，包内调用方式不变。
- 业务约束：
  - 所有内置 Skill 的运行时行为不变。
  - 对 CLI 用户行为无可见影响。

## Open Questions / 开放问题

- [ ] 当前无阻塞开放问题；custom 提供商不支持 `tools` 时的回退策略留到技术方案阶段细化。

## Requirement Decisions / 需求决策

- 2026-07-15 23:44:
  - 决策：将工具调用改造为 SDK 原生 `tools` 参数。
  - 原因：消除正则解析脆弱性、原生支持并行 `tool_calls`、所有 4 个服务商统一格式。
