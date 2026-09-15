# Lifecycle: 思考模式强度管理（reasoning effort）落地

## 迭代时间线

- 2026-09-19 10:30: 创建迭代。
  - 背景：OpenAI 协议思考强度调研（`docs/research/openai-reasoning-effort-support.md`）确认官方支持 7 档 `reasoning_effort`，并明确「Not all reasoning models support every value」。
  - 核实代码事实：`apply_thinking` 以 `extra` 透传 `reasoning_effort`（`providers.rs:1152`）；`ThinkingEffort` 仅 `Low/High/Max`（`models.rs:349`）；`ThinkingCapability` 无档位白名单（`models.rs:367`）；`resolve_thinking` 未校验 effort 合法性（`providers.rs:1032`）。
  - 产出：`technical-plan.md`（P0-P2 改动点、影响范围、风险、测试用例、分步顺序、决策点）。

## 状态

- Spec：定稿（初版）
- Execution Approval：`Pending`（等待用户确认决策点 1-3）

## 待用户确认的决策点

1. `none` 与 `enabled=false` 的优先级。
2. 是否本期对齐 7 档（目标服务商支持度）。
3. Responses API 是否纳入近期规划（影响内部抽象形态）。
