# Spec: 对话历史操作栏（复制 + 评价）

## Goal

- 要解决什么问题：对话历史（ChatArea 消息列表）没有复制与人工评价入口。用户希望每条消息提供操作栏：**复制**该消息原文；assistant 回复额外提供**评价**（-5..5 权重打分），打分的副作用与模型自主打分一致——调整上游节点和边的权重。
- 验收结果：消息 hover 显示操作栏；提问消息可复制、模型回复可复制+评价、工具调用/工具回复/系统消息无操作栏；评价复用模型打分（`ScoreFeedbackBeforeHook`）的窗口语义，直接累加 delta。

## Done Contract

- 什么算完成：
  1. 后端抽取共享评分方法 `AssistantMode::apply_score_feedback(topic_id, delta)`，`ScoreFeedbackBeforeHook` 与人工命令共用。
  2. 新增 Tauri 命令 `score_feedback(conversation_id, score)`：解析 topic → 校验 score（整数，-5..=5 且非 0）→ 窗口非空 → 应用 delta → emit `StateChange::Neurons`。
  3. `core/events.rs` 新增 `Neurons` 事件；前端 `dataStore` 增加 `neurons` kind 刷新（`neuronsVersion`），`NeuronManager` 监听后重拉。
  4. `ChatMessage.svelte` 增加 hover 操作栏：user=复制、assistant=复制+评价、tool_call/tool_result/system=无。
  5. 评价交互：hover 评价按钮弹出 -5..5 小面板，点击即打分（0 置灰不可点，与模型约束一致），打分后关闭面板并累加 delta。
  6. 复制使用 `navigator.clipboard.writeText(message.content)`，复制该消息原始 markdown 文本。
  7. i18n 中/英标签就位（复制 / 评价 / 已复制）。
- 由什么证明：`cargo build` + `read_lints` 0 错误；手动在 App 内：hover 出现操作栏、复制成功、评价后神经元面板权重变化。
- 哪些情况仍算未完成：按消息盖章神经元（本轮明确不做，采用最新窗口语义）；评分历史持久化/回放；复制格式自定义（如整轮 Q+A 组合）；TUI/CLI 端操作栏。

## Scope

- In：`assistant_mode.rs` 评分逻辑抽取、`lib.rs` 新命令、`events.rs` 新事件、前端 `ChatMessage / ChatArea / dataStore / NeuronManager / i18n / types`。
- Out：Message 结构变更（不盖章神经元）；评分历史持久化；TUI/CLI；复制格式自定义。

## Facts / Constraints

- **模型打分逻辑（已确认）**：`ScoreFeedbackBeforeHook`（`assistant_mode.rs:1084-1195`）读 topic 状态 `intervention_neuron_ids` → 对每个介入神经元 `adjust_weight(delta)` + 关联边 `adjust_connection_weight(delta)` + lineage 归因 `accumulate_variant_delta(父变体, delta)` + `maybe_evolve_creator_variants()`（失败仅 warn）。
- **窗口语义（已确认，用户批准接受）**：`intervention_neuron_ids` 只保留**最新一轮**的介入窗口，不是按消息存储。人工评价复用该窗口：评最新一条回复准确，评历史旧消息会错位到最新窗口——接受此限制，不做按消息盖章。
- **会话 ↔ 课题**：assistant 模式下 `conversation_id == session_id`；topic 通过 `topic_store.find_by_session_id` 解析（`topic_store.rs:118`）。
- **score 约束与模型一致**：整数，闭区间 -5..=5，且**非 0**。
- **前端现状**：`ChatMessage.svelte` 纯展示、无操作栏；`dataStore` 只管理 topics/conversations/poller/sessions，无 neurons；`NeuronManager` 在 onMount 本地加载 `list_neurons`，无事件刷新。
- `Message` 结构（`models.rs:14-27`）无 neuron 元数据，本次不动。

## 接口契约设计

### 后端

```rust
// core/assistant_mode.rs —— 从 ScoreFeedbackBeforeHook 提取，模型与人工共用
impl AssistantMode {
    /// 读取 topic 的干预窗口；窗口为空返回空 Vec（由调用方决定跳过或报错）。
    pub fn intervention_window(&self, topic_id: &str) -> AppResult<Vec<String>>;
    /// 对窗口内每个介入神经元应用 delta：节点权重 + 关联边 + lineage 归因 + 变体演进。
    /// 窗口为空时调用方不应调用（此处防御性跳过）。
    pub async fn apply_score_feedback(&self, topic_id: &str, delta: f64) -> AppResult<()>;
}

// core/events.rs
pub enum StateChange {
    Topics,
    Conversations,
    Poller { status: PollerStatus },
    Sessions,
    Neurons, // 新增：前端刷新神经元面板
}

// lib.rs 新命令
#[tauri::command]
async fn score_feedback(
    assistant: State<'_, Arc<AssistantMode>>, // 注意：manage 注册的是 Arc<AssistantMode>，State 类型必须一致
    state_emit: State<'_, StateEmitter>,
    conversation_id: String,
    score: i64,
) -> TauriResult<()>;
```

`score_feedback` 命令流程：
1. `assistant.topics()?.find_by_session_id(&conversation_id)` → 无绑定 topic → `AppError::ConversationNotFound("no topic bound to session")`。
2. 校验 `score == 0 || !(-5..=5).contains(&score)` → `AppError::InvalidInput`。
3. `assistant.intervention_window(&topic_id)?` → 窗口为空 → `AppError::InvalidInput("no intervention window to score")`。
4. `assistant.apply_score_feedback(&topic_id, score as f64).await?`。
5. `state_emit.inner()(StateChange::Neurons)`。

`ScoreFeedbackBeforeHook` 改造：窗口为空跳过逻辑保留；模型取得 score 后改调 `apply_score_feedback`，删除内联的权重循环（行为不变）。

### 前端

```ts
// lib/types.ts —— StateEventKind / StateChangePayload 增加 "neurons"
export type StateEventKind = "topics" | "conversations" | "poller" | "sessions" | "neurons";

// lib/stores/dataStore.svelte.ts
// state 增加 neuronsVersion: number（初值 0）
// handleStateChanged: kind === "neurons" → state.neuronsVersion++
// 新 action:
async function scoreFeedback(conversationId: string, score: number): Promise<void> {
  await invoke("score_feedback", { conversationId, score });
}

// lib/components/ChatMessage.svelte —— 新增 props，保持纯展示
let {
  message,
  onCopy,
  onRate,
  canRate,
}: {
  message: Message;
  onCopy?: (msg: Message) => void;
  onRate?: (score: number) => void;
  canRate?: boolean;
} = $props();
```

操作栏显隐规则：
- `isToolResult` / `isSystem` → 不渲染操作栏。
- `isUser` → 仅复制。
- `isAssistant` → 复制 + 评价（`canRate` 为 false 时评价按钮隐藏）。

`ChatArea.svelte` 接线：

```ts
import { dataStore } from "$lib/stores/dataStore.svelte";

const activeConversationId = () => dataStore.state.activeConversationId;
const canRate = $derived(
  dataStore.state.topics.some(
    (t) => t.session_id === dataStore.state.activeConversationId
  )
);

async function handleCopy(msg: Message): Promise<void> {
  await navigator.clipboard.writeText(msg.content);
  // 本地 transient "已复制" 反馈（ChatMessage 内部 state 或 ChatArea 传递）
}
async function handleRate(score: number): Promise<void> {
  if (!dataStore.state.activeConversationId) return;
  await dataStore.scoreFeedback(dataStore.state.activeConversationId, score);
}
```

`NeuronManager.svelte` 刷新：把 onMount 的加载逻辑抽为 `reload()`，追加

```ts
$effect(() => {
  dataStore.state.neuronsVersion; // 依赖读取触发
  void reload();
});
```

### 评价面板交互

- 操作栏 CSS `opacity: 0`，`.message:hover .actions { opacity: 1 }`。
- 评价按钮 hover → 弹出小面板（绝对定位在按钮上方/下方），含 11 个按钮 `-5..5`，**0 置灰不可点**（与模型约束一致：score 非 0）。
- 点击某分值 → `onRate(score)` → 面板关闭；错误（如无窗口）经 `formatInvokeError` 提示。

## Open Questions

- [ ] 复制按钮点击后是否要有"已复制"反馈：默认有（本地 transient 文案），不额外持久化。
- [ ] 评价成功后是否需要 toast 提示"已评价 +N"：默认无 toast，仅关闭面板；权重变化在神经元面板可见。

## Restated Understanding

- 我理解当前任务是：给对话历史每条消息加 hover 操作栏——user 消息可复制原文，assistant 消息可复制原文 + hover 弹出 -5..5 打分面板；打分副作用与模型 `score_feedback` 完全一致（调整上游节点与边权重 + lineage 归因），触发入口不同（人工分数替代模型分数），采用最新干预窗口语义、直接累加。
- 当前核心目标是：交付可用的复制 + 评价闭环，后端逻辑复用不重写，前端按消息类型区分操作栏。
- 当前边界是：不做按消息盖章神经元、不做评分持久化、不动 TUI/CLI。
- 暂不处理：评分历史、复制格式自定义、TUI/CLI 操作栏。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：对话历史操作栏（复制 + 评价），评价复用模型打分窗口语义。
- 当前核心目标：后端抽共享评分 + 新命令 + Neurons 事件；前端操作栏 + 打分面板 + 复制；刷新神经元视图。
- 当前进度：已实现并静态验证通过（cargo build / svelte-check / vite build），待 App 内手动 UI 验证。
- 下一步 1：用户 App 内验证 hover 操作栏、复制、评价打分与神经元权重变化。
- 下一步 2：按反馈微调 UI/交互细节。
- 验证方式：`cargo build`、`pnpm run check`、`pnpm run build` 已通过；App 内手动验证待用户。
- Execution Approval: 已批准（2026-08-05）

## Change Log

- 2026-08-05: 初始 micro-spec。决策：评价复用模型打分窗口语义（不做按消息盖章）；操作栏按消息类型区分（user=复制、assistant=复制+评价、tool/system=无）；重复打分直接累加。
- 2026-08-05（UI 反馈修订）：① 评价面板改为 `top: 100%` 无间隙 + 面板内部 padding 承担视觉间距，鼠标从按钮滑到面板不再消失；② 操作栏改为图标按钮（内联 SVG：复制/已复制勾/星级评价，沿用项目 24 viewBox + stroke=currentColor 惯例），补 title/aria-label；③ 面板默认左对齐 `left: 0`，user 消息侧 `right: 0`，避免超出可见区域。
- 2026-08-05（UI 反馈修订 2）：① 面板按视口底部空间动态向上/向下弹出（`openRating` 测量按钮位置，空间不足时 `bottom: 100%`），避免被输入框遮住，z-index 提升至 100；② 评分按钮去掉 0，`-5..-1,1..5` 一行 10 个。
- 2026-08-05（Bug 修复）：`score_feedback` 命令的 `State` 类型修正为 `State<'_, Arc<AssistantMode>>`（`app.manage` 注册的是 `Arc<AssistantMode>`，此前类型不匹配导致运行时 "state not managed for field `assistant`"）；命令失败路径补 `tracing::warn` 日志便于排查。

## Validation

- Self-check：实现完成。后端抽取 `intervention_window` / `apply_score_feedback`（`assistant_mode.rs`），`ScoreFeedbackBeforeHook` 改为调用共享方法；新增 `score_feedback` Tauri 命令（`lib.rs`）；`events.rs` 新增 `Neurons`。前端 `ChatMessage` 操作栏（user=复制、assistant=复制+评价、tool/system=无）、`ChatArea` 接线、`dataStore` 新增 `neurons` kind + `scoreFeedback`、`NeuronManager` 监听 `neuronsVersion` 刷新、i18n zh/en 标签。
- Static checks：`cargo build` 通过（0 error，仅既有 warning：AtomicUsize / ConversationMode / cli unused_mut，均非本次改动）；`pnpm run check` 0 error（44 既有 warning，未新增）；`pnpm run build` 通过。
- Runtime / Test：待 App 内手动验证（hover 出现操作栏、复制成功、评价后神经元面板权重变化）。
- Human confirmation：micro-spec 已获用户批准后实现；手动 UI 验证待用户进行。
- 结果汇总：实现已完成，静态验证通过；运行时 UI 验证待用户确认。
- 核心目标是否已由证据证明完成：后端链路 + 前端交互已落地且通过编译/类型检查；UI 手感需人工确认。
- 若未完成，当前剩余差距：无代码差距；仅剩 App 内手动 UI 验证。
- 剩余风险：`navigator.clipboard` 在 Tauri webview 的权限（如失败走 `errorMessage` 提示）；最新窗口语义对历史旧消息错位（已批准接受）。

## Resume / Handoff

- 当前状态：实现完成，静态验证通过，待 App 内手动 UI 验证。
- 当前卡点：无。
- 下一步唯一动作：用户 App 内验证复制/评价交互与神经元权重变化。
- 下一轮核心目标：如有 UI 细节问题，按反馈微调。
