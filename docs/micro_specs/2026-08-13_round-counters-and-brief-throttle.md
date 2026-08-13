# Spec: 轮次记录维度 + 简报/选型降频

## Goal

- 新增课题级轮次记录维度：**总轮次** / **用户接入轮次** / **当前轮询轮次**（用户接入后到当前）。
- 课题简报生成降频：非用户（推动）轮次每 **3** 轮生成一次（hook 每轮都被调用，在 hook 内判断）。
- 主对话神经元选型降频：每 **5** 轮做一次 LLM 选型，中间轮沿用上次选中锚点。

## Done Contract

- `topic.extra.assistant` 扩展 `AssistantTopicState`：

  | 字段 | 语义 |
  |---|---|
  | `total_rounds` | 总轮次（User + ManualStep + Poller，**成功跑完即计**） |
  | `user_rounds` | 用户接入轮次 |
  | `poll_count` | 距上次用户接入的推进轮次（User 轮归零；简报 3 轮 / 选型 5 轮频率的基准） |
  | `brief_cache` | 上份课题简报缓存（推进轮复用，避免每轮重喂长简报） |
  | `last_brief_round` | 上次生成简报时的 `poll_count`（距上次 ≥ 3 轮才因频率刷新） |

- 计数规则（`apply_round_counter`，成功跑完即计，不要求有实质进展）：
  - 每轮 `total_rounds` +1；
  - User 轮 `user_rounds` +1，且 `poll_count` / `last_brief_round` 归零（重新起算频率）；
  - Manual/Poller 轮 `poll_count` +1。
- 简报刷新三条件 **OR**（`should_refresh_brief`，`before_round` 推进分支每轮判断，任一命中即刷新并写缓存）：
  1. **频率兜底**：`poll_count - last_brief_round ≥ 3`；
  2. **课题有变化**：`fresh ≠ brief_cache`（字符串比较自动覆盖进度/scope/切换/新增）；
  3. **上轮非工具调用结束**：模型需课题状态锚定，屏除轮次限制。
  - 未命中时复用缓存简报，不重喂模型。
- 选型降频：引擎只认**本轮意图** `reselect: bool`——`true` 按 seed/behavior 原规则走 LLM 选型；`false` 不选型，优先沿用 `last_selected_neuron_id` 锚点（锚点缺失仍回退选型）。`reselect` 仅影响真正调 LLM 的分支（Global / 邻选），**Fixed / None 策略不感知**（按原规则执行）。频率计算留在业务层：`assistant_session` 的 before_round 推进分支算 `poll_count % SELECTION_EVERY_N_ROUNDS(5) == 0` 后传 `reselect`；User 轮与裁决调用传 `true`（每轮选型）。引擎不持有任何轮次/频率概念。

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/src/core/assistant_session.rs` | `AssistantTopicState` 扩展；`apply_round_counter`（原 `bump_poll_count` 语义改造）；`should_refresh_brief` 三条件；`before_round` 推进分支写入简报缓存 + 按 `poll_count % 5` 算 `reselect`（`SELECTION_EVERY_N_ROUNDS` 常量居此）；`after_round` User/Manual/Poller 分支接入计数 |
| `src-tauri/src/core/call_service.rs` | `RoundInput.reselect: bool`；`resolve_role` 三处 select 分支插入 `reuse_selected_neuron` 降频（Fixed/None 分支不感知） |
| `src-tauri/src/core/conversation_runner.rs` | `RoundContext.reselect: bool`（默认 true = 每轮选型）透传给 `RoundInput` |

## 兼容性

- 旧 `topic.extra.assistant` 数据缺失新字段 → serde `#[serde(default)]` 回落 0 / None。
- 会话运行态（`last_selected_neuron_id` 等）仍在 `conversation.extra.session.state`，本改动未迁移。
- `selection_round: None` 旧语义（每轮选型）等价于新 `reselect: true`，存量调用方/裁决调用无需感知频率。

## Validation

- `cargo test --lib`：186 passed, 0 failed（含新增：计数语义 / 简报三条件 / 选型降频 / 状态序列化兼容）。
- 行为验证：推动轮第 3 轮必刷简报；课题调整后下一轮简报即更新；非重选轮 selector 不调用（selector_calls 为 0）。

## Change Log / Validation（2026-08-13）

- 方案经用户确认：简报**三条件 OR**；选型**主对话降频（每 5 轮）**；计数口径**成功跑完即计**。
- 实现完成：`cargo test --lib` 186 passed, 0 failed。
