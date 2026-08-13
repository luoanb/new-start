# Spec: 消息盖章评分（评分区间数据依据重构）

> 前置：`2026-08-05_20-00_chat-history-action-bar.md`（评价 UI 与共享评分方法已就位）。
> 本文修订其评分数据依据：废弃"干预窗口（滚动累积）"，改为"消息盖章 + 介入区间推导"。

## Goal

* 要解决什么问题：现有评分依赖会话态干预窗口（`intervention_neuron_ids`，滚动累积、评分即消费），导致：① 无法对历史消息评分，评历史会错位到最新区间；② 窗口是瞬态，与"每轮归属哪个神经元"脱节；③ 需要维护一套窗口状态机。

* 验收结果：每条 assistant 消息盖章其所属神经元（`Message.neuron_id`）；评分改为**定位消息所在介入区间 → 收集区间内盖章神经元（去重）→ 调权**；模型自动打分与人工评分共用同一推导；用户可**随时对任意 assistant 消息评分**（无消费/锁定约束）；废弃干预窗口机制。

## 关键决策（2026-08-13 用户确认）

1. **消息盖章**：`Message` 增加可选 `neuron_id`，每轮 assistant 产物（tool\_call / text 回复 / nudge 简报）落库时从 `outcome.selected_neuron_id` 写入；旧消息默认 `None` 兼容。
2. **区间定位**：介入边界 = `role=user` 且 `body=Text` 的消息。评第 i 条消息 → 区间 = 上次介入（不含）之后、下次介入（不含）之前的所有消息；收集区间内盖章神经元去重作为评分目标。与 requirements"上一轮用户介入到本次用户介入区间内"一致。
3. **允许重复评分**：无消费标记、无区间锁定；同一区间可多次评（每次都是真实反馈）。
4. **统一推导**：模型自动打分（用户介入 beforehook）用同一推导函数取"最后一段区间"；删除 `intervention_neuron_ids` / `last_intervention_at` / `mark_user_intervention` / `accumulate_interval_neuron` / `intervention_window`。

## Done Contract

* 什么算完成：

  1. `Message` 增加 `neuron_id: Option<String>`（`#[serde(default, skip_serializing_if)]`），落库处（tool\_call / text / nudge）盖章本轮选中神经元。
  2. 纯函数 `interval_neuron_ids(messages, anchor_index) -> Vec<String>`（介入边界推导 + 区间内盖章去重）+ 单元测试（首段/中段/末段/无介入/去重）。
  3. `apply_score_feedback(topic_id, neuron_ids, delta)` 改为显式接收神经元集合；模型打分 hook 与人工命令共用。
  4. Tauri 命令改为 `score_feedback(conversation_id, message_index, score)`：校验 score → 解析绑定 topic → 读会话消息 → index 越界校验 → 推导区间 → 空区间报错（中文）→ 调权 → `emit Neurons`。
  5. 模型打分 hook（用户介入 beforehook）：推导最后一段区间，空则 skip，非空则调用模型打分后 `apply_score_feedback`。
  6. 废弃干预窗口：删除 `SessionState.intervention_neuron_ids` / `last_intervention_at`、`mark_user_intervention`、`accumulate_interval_neuron`、`accumulate_interval_ids` 及相关测试；`resolve_role` 保留 `last_selected_neuron_id`（选型锚点，不受影响）。
  7. 前端：`canRate` 恢复为"会话绑定 topic"（所有 assistant 消息 hover 显示评价按钮）；`handleRate` 携带消息 index；`dataStore.scoreFeedback(conversationId, index, score)`；移除干预窗口判断。
  8. 文档同步：requirements.md 与旧 micro\_spec 的窗口语义改为消息盖章推导。

* 由什么证明：`cargo test --lib` 全绿（含 interval\_neuron\_ids 单测）；`pnpm check` 0 errors；App 内对任意 assistant 消息评分、神经元权重变化、历史区间定位正确。

* 哪些情况仍算未完成：评分历史持久化/回放；复制格式自定义；TUI/CLI 端操作栏。

## 接口契约

```rust
// models.rs —— Message 盖章
pub struct Message {
    pub role: MessageRole,
    pub body: MessageBody,
    pub timestamp: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neuron_id: Option<String>,
}

// assistant_session.rs —— 区间推导（纯函数）+ 评分
/// 介入边界 = role=user 且 body=Text 的消息；返回 [上次介入+1, 下次介入) 内盖章神经元去重。
fn interval_neuron_ids(messages: &[Message], anchor_index: usize) -> Vec<String>;

/// 显式神经元集合评分；模型 hook 与人工命令共用。
pub async fn apply_score_feedback(&self, topic_id: &str, neuron_ids: Vec<String>, delta: f64) -> AppResult<()>;

// lib.rs
#[tauri::command]
async fn score_feedback(
    assistant: State<'_, Arc<AssistantSession>>,
    state_emit: State<'_, StateEmitter>,
    conversation_id: String,
    message_index: usize,
    score: i64,
) -> TauriResult<()>;
```

模型打分 hook（用户介入 beforehook）：

1. 推导最后一段区间：`interval_neuron_ids(&messages, messages.len())`（列表末尾哨兵；用户输入在 before hook 之后才落库，本次介入尚不在消息内）。
2. 空区间 → skip；非空 → `call_judgement` 取分 → `apply_score_feedback(topic_id, neuron_ids, score)`。

## Restated Understanding

* 评分数据依据从"会话态干预窗口"改为"消息盖章 + 介入区间推导"；用户可对任意 assistant 消息随时评分，评分目标 = 该消息所在介入区间内所有盖章神经元（去重）。

* 干预窗口相关状态与 hook 全部移除；`last_selected_neuron_id` 作为选型锚点保留。

* 模型打分与人工评分统一为同一推导 + 同一 `apply_score_feedback`。

