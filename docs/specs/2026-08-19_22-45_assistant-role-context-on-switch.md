# Spec: assistant-role-context-on-switch

## Goal

- 要解决什么问题：assistant 模式非首轮每轮注入 RoleContext（与角色是否变化无关），以 User 文本回灌模型重复消耗上下文。
- 验收结果：角色未切换的轮次不再注入；切换轮正常注入；首轮 System、裁决调用、契约段行为不变；`cargo test -p pulsar-app` 全量通过。

## Done Contract

- 完成 = 复用轮 wire 无该轮 RoleContext，切换轮仍注入，既有测试更新后全绿，本 spec 回写结论。
- 由什么证明 = `cargo test -p pulsar-app` 通过 + 人工核对更新后的断言语义。
- 哪些情况仍算未完成 = 首轮 System / 裁决注入 / 契约段拼接任一行为回归。

## Scope

- In：`RoundResolver::attach_role` 注入条件（仅角色切换时注入）+ 受影响测试更新 + 文档回写。
- Out：前端展示、选型频率、锚点复用、RoleContext wire/落库/回灌协议、裁决调用逻辑。

## Facts / Constraints

- 已确认事实：
  - `attach_role` 在 `resolve` 末尾无条件执行（`src/core/round_resolver.rs`：`resolve` → `Self::attach_role(old_messages, neuron.as_ref())`）：历史空 → System；历史非空 → RoleContext（`[当前角色]` 前缀 + 契约段按 `already_has_contract` 判重）。
  - 复用锚点轮 `neuron.id` 恒等于 `last_selected`；选型轮结果可能相同或不同。
  - 裁决调用 `call_judgement` 传 `last_selected: None` + `reselect: true`（`src/core/assistant_session.rs`）→ 锚点缺失视为「角色不同」仍注入，契约段保住。
  - 首轮 System 注入不受影响；Fixed 策略角色恒不变 → 首轮后不再注入（历史 System 常驻，语义正确）。
- 技术/业务约束：`attach_role` 保持纯函数；判定以 `neuron.id == last_selected` 为准；不落库、回灌协议不变。
- 已知风险：`last_selected` 存在但历史无角色声明的组合（锚点来自历史注入记录，理论边界，可接受，不入实现范围）。

## Open Questions

- 无（Q1 判定口径、Q2 契约段边界均已确认：Q1 = 用户 2026-08-19 确认「仅角色切换且前后神经元不同才注入」；Q2 = 裁决调用 `last_selected: None` 视为角色不同仍注入，契约段保住，历史无契约段 + 角色未变的组合仅理论边界）。

## Restated Understanding

- 我理解当前任务是：将「每轮重申角色」改为「角色实际切换才注入」——判定口径为本轮选中 `neuron.id` vs 上一轮锚点 `last_selected_neuron_id`。
- 当前核心目标是：消除复用轮的重复角色声明，同时保住切换轮注入与契约段行为。
- 当前边界是：只改注入条件；选型频率、锚点复用、wire/落库/回灌协议、前端展示均不动。
- 暂不处理：前端「角色切换」块弱化/折叠样式；非 assistant 模式的其它注入路径。

## 接口契约设计

```rust
// src/core/round_resolver.rs
impl RoundResolver {
    // 签名：resolve 透传 last_selected；非首轮仅「角色切换」时注入 RoleContext
    fn attach_role(
        old: Vec<Message>,
        neuron: Option<&Neuron>,
        last_selected: Option<&str>,   // 新增：上一轮锚点 id
    ) -> Vec<Message> {
        let Some(neuron) = neuron else { return old };
        if old.is_empty() { return push_system(old, neuron); }       // 首轮：不变
        if Some(neuron.id.as_str()) == last_selected { return old; } // 角色未变：不注入（核心）
        push_role_context(old, neuron)                                // 角色切换：注入（契约段规则不变）
    }
}
// resolve 内调用点（唯一）：Self::attach_role(old_messages, neuron.as_ref(), last_selected)
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：仅角色实际切换（前后神经元不同）时注入 RoleContext。
- 当前核心目标：消除复用轮的重复角色声明，保住切换轮与契约段行为。
- 当前进度：需求与方案已确认并落盘（本 spec）。
- 下一步 1：改 `attach_role` 签名 + 注入条件（含调用点透传锚点）。
- 下一步 2：更新两处测试断言（`attach_role_later_round_*`、conversation_runner 复用轮断言反转）；`cargo test -p pulsar-app` 验证。
- 下一步 3：回写本 spec（Change Log / Validation → done）。
- 涉及文件 / 模块：`packages/pulsar-app/src-tauri/src/core/round_resolver.rs`、`conversation_runner.rs`（测试）。
- 风险：低（单点条件判断；已核实裁决调用与首轮不受影响）。
- 验证方式：`cargo test -p pulsar-app`。
- Execution Approval: `Approved`（2026-08-19 用户批准后执行）

## Change Log

- 2026-08-19 22:45: 需求确认（用户：仅角色切换且前后神经元不同才注入）；方案按 sdd-riper-one-light（spec-lite）单份落盘 `docs/specs/`。
- 2026-08-19 22:5x: 执行完成——`round_resolver.rs` `attach_role` 新增 `last_selected: Option<&str>` 参数，非首轮 `neuron.id == last_selected` 时不再注入 RoleContext（`resolve` 调用点透传锚点）；更新 5 处既有测试调用 + 新增 `attach_role_later_round_same_role_skips_injection`；`conversation_runner.rs` 复用轮断言反转（不携带 RoleContext）+ 新增切换轮注入断言；同步过时注释（round_resolver / models / conversation_runner）。

## Validation

- Self-check: 通过——判定口径与 spec 契约一致；首轮 System、裁决调用（`last_selected: None` → 仍注入）、契约段 `already_has_contract` 逻辑均未触碰。
- Static checks: 通过——无新增编译警告。
- Runtime / Test: **`cargo test -p pulsar-app` 294 passed; 0 failed**（新增 1 条「角色未变不注入」单测 + 反转后的复用轮/切换轮端到端断言全绿）。
- Human confirmation: 已批准（2026-08-19）。
- 结果汇总：复用轮不再注入 RoleContext；切换轮（锚点不同 / 缺失）正常注入；首轮 System 与裁决调用行为不变。
- 核心目标是否已由证据证明完成：**是**（测试全绿 + 断言语义人工核对）。
- 剩余风险：`last_selected` 存在但历史无角色声明的组合（理论边界，spec 已列，不入实现范围）。

## Resume / Handoff

- 当前状态：spec 已落盘，等待执行批准。
- 当前卡点：无。
- 下一步唯一动作：用户批准后执行改动清单。
- 下一轮核心目标：复用轮不再注入 RoleContext，全量测试通过。
