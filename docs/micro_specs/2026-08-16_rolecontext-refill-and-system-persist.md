# Spec: RoleContext 回灌 + 首轮 System 落库（历史=wire，缓存稳定）

> B2 落库补充的修订方案（方案 C）。用户审阅 `docs/specs/2026-08-14_16-00_neuron-stable-system-prompt-b2.md` 后确认：RoleContext 必须回灌；首轮不允许 System 冻结值与 RC₁ 内容重复。

## Goal

- 消除「落库 ≠ wire」的割裂：RoleContext 落库消息**参与后续模型输入组装**（回灌），store 历史顺序与 wire 注入顺序完全一致。
- 缓存稳定：多轮 wire 呈**严格前缀累积**（轮 N+1 wire = 轮 N wire + 尾部增量），服务端前缀缓存可命中。
- 首轮冻结的角色声明以 `role=System` 消息落库（历史第一条），满足「首轮系统神经元也需要落库」。
- 禁止首轮 wire 中 System 冻结值与 RC₁ 内容重复出现（角色信息每轮只出现一次）。

## 现状与问题

当前 B2 落库设计存在三个问题：

1. **RoleContext 不回灌**：`message_to_model` 对 RoleContext 返回 `None`，store 历史与 wire 不一致；角色信息依赖每次组装重新注入，从 store 无法还原模型实际看到的上下文。
2. **落库顺序与 wire 注入顺序不一致**：wire 中 RC 注入在「历史之后、用户输入之前」；persist 落库却在「用户输入之后、产物之前」。若直接回灌，上轮注入的 RC 在历史中位置翻转（U 前 → U 后），**每轮前缀缓存都失效**，缓存反而更差。
3. **首轮重复**：当前实现空历史首轮也带出 `role_context_message`（内容 = 冻结的 System 值）。若回灌，首轮 wire 将出现 `System(冻结值)` 与 `RC₁([当前角色]冻结值)` 双重角色声明——用户判定为设计不合理。

## 方案 C

角色信息每轮只出现一次：**首轮在 System，后续轮在 RoleContext**。

### 时序

```
首轮：   wire  = System(A) + U₁                      落库 = [System(A), U₁, A₁]
第二轮： wire  = System(A) + U₁ + A₁ + RC₂ + U₂      落库 = [System(A), U₁, A₁, RC₂, U₂, A₂]
第三轮： wire  = System(A) + U₁ + A₁ + RC₂ + U₂ + A₂ + RC₃ + U₃
```

- 首轮：角色冻结进 System（B2 不变），以 `role=System, body=Text{A}` 消息落库为历史第一条；**不落 RC₁**（无重复）。
- 后续轮：本轮选中神经元以 RC 消息注入 wire（历史后、U 前），落库在用户输入之前（与 wire 一致），`message_to_model` 回灌。
- 每轮 wire = 上一轮 wire + 尾部增量 → 严格前缀，缓存命中。

### 机制

1. **`message_to_model` 回灌**：`RoleContext` 与 `Nudge` **均**回灌（`Some(ModelMessage{role: User, content})`）。用户 2026-08-16 确认：落库消息一律回灌（简报每轮生成，条数由「生成一次落库一次」控制；角色每轮一次）。
2. **首轮 System 落库**：`persist_input`（发送前）在「历史为空 且 wire 首条为 System」时，直接取 wire 首条 System 消息落为历史第一条 `role=System, body=Text{content}`（wire 组装后即可确定，不依赖 outcome；无需新增 `frozen_this_round`——resolve 侧 B2 概念不外泄）。`replace_system` 用冻结值替换历史第一条 System（内容相同，无冲突）。
3. **两段式落库**：`persist_input`（发送前）顺序 = `[首轮 System] → [RoleContext] → 输入`；`persist_outcome`（发送后）顺序 = `[产物] → [会话态]`。两段合起来与 wire 注入位置一致；拆开后模型调用失败/超时不丢输入侧消息。
4. **`role_context_message` 恢复空历史排除**：`input.messages.is_empty()` 时返回 `None`（首轮不落 RC），非空历史 + 冻结才带出 `[当前角色]\n{ctx}`。
5. **`stable_system_prompt` 冻结机制保留**：`resolve_role` / `freeze_or_replace` 不变。

### 严格前缀验证

- 轮 1 wire `System(A)+U₁` 是轮 2 wire `System(A)+U₁+A₁+RC₂+U₂` 的前缀 ✅
- 轮 2 wire 是轮 3 wire 的前缀 ✅
- 回灌顺序 = 落库顺序（System 在首、RC 在 U 前）→ 组装时无位置翻转 ✅

## Done Contract

- `models.rs`：无改动（`MessageBody::RoleContext`、`MessageRole::System` 已存在）。
- `call_service.rs`：`message_to_model`：`RoleContext => Some(...)` 回灌（User 角色）；`Nudge => Some(...)` 回灌（User 角色）。
  > round-pipeline-split 重构后落位：`MessageAssembler::from_message`（`model_call_input.rs`）；首轮 System 落库由 `persist_input` 判「历史为空 且 wire 首条为 System」直接取 wire 内容，无需 `frozen_this_round`。
  > `role_context_message` 空历史排除在 `assemble_with_context`（`model_call_input.rs`）。
- `conversation_runner.rs`：落库拆两段——`persist_input`（发送前）：`[首轮 System] → [RC] → 输入`；`persist_outcome`（发送后）：`[产物] → [会话态]`。
- 前端：无改动（首轮 System 消息走既有 system 渲染；RC 已支持）。
- 测试更新：`converse_frozen_round_carries_context_message`（首轮 `role_context_message=None`；次轮 `Some`）；`from_message` 测试（Nudge/RoleContext 均回灌）；persist 集成测试（首轮 System 消息 + 次轮 RC 在 U 前 + 回灌 wire 严格前缀）。
- `docs/specs/2026-08-14_16-00_neuron-stable-system-prompt-b2.md` Reverse Sync（落库/回灌描述同步）。

## 兼容性

- **wire 行为变化（预期内）**：后续轮历史中回灌 RC → wire 多出角色消息（连续角色轨迹）。这是本次需求的直接目的（缓存稳定、历史=wire）。
- 首轮 wire 不变：`System(A) + U₁`（RC₁ 不注入），落库多一条 System 消息（审计/展示用）。
- `Nudge` / `RoleContext` 均回灌（用户确认，覆盖初版的"Nudge 不回灌"）。
- 旧会话数据：历史为空、无 System 消息的旧会话，首轮 wire 若无 System 则 `persist_input` 不落 System 消息，无破坏。

## Validation

- `cargo test`（221+ 用例）全部通过；新增用例覆盖：首轮 System 落库、次轮 RC 落库位置、回灌 wire 严格前缀、Nudge/RoleContext 均回灌。
- 前端 `pnpm check` 无新增错误。

## 改动点

| 文件 | 改动 |
|---|---|
| `src/core/model_call_input.rs` | `from_message` RoleContext/Nudge 均回灌；`assemble_with_context` 空历史排除 RC |
| `src/core/conversation_runner.rs` | 落库拆两段：`persist_input`（发送前：首轮 System → RC → 输入）+ `persist_outcome`（发送后：产物 + 会话态） |
| `src/core/conversation_runner.rs`（tests） | 上述行为断言更新 + 新增回灌/落库用例 |
| `docs/specs/2026-08-14_16-00_neuron-stable-system-prompt-b2.md` | Reverse Sync 落库/回灌章节 |

## Change Log

- 2026-08-16: 初版方案（方案 C，待批准后实现）。
- 2026-08-16（实现修订）：用户确认 **Nudge 同样回灌**（"落库的也需要给模型发送过去" + "RoleContext也要啊"）——推翻初版「Nudge 不回灌」，`Nudge` / `RoleContext` 均回灌为 User 文本；首轮 System 落库与 persist 顺序重排按方案 C 落地（在 round-pipeline-split 重构后的三段管道中实现，见上）。
- 2026-08-16（实现修订）：用户提出 **分开落库**（"为什么不分开落库 发送给模型前，应该可以落库了呀？"）——`persist` 拆为 `persist_input`（发送前，wire 组装后即可确定：首轮 System → RC → 输入）与 `persist_outcome`（发送后：产物 → 会话态），模型调用失败/超时不再丢用户消息。
