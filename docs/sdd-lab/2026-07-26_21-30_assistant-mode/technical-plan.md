# Technical Plan / 技术方案: Assistant 运行模式

> 2026-07-28 Reverse Sync：当前技术方案已被执行，但 Hook 分层不符合最新需求。本文件作为修正版技术方案，保留可复用的数据层、Engine、Poller 设计，重写 Hook 协议落点：`docs/design/hook-spec.md` 是对话模式通用 Hook 协议，Assistant 业务 Hook 必须按该协议实现。当前状态从 executing 回退到 planned，待用户确认后再进入代码重构。

## 1. ConversationMode 变更

### models.rs
```rust
pub enum ConversationMode {
    Chat,
    Agent,
    Assistant,
}
```

Conversation JSON 文件无变更（`mode` 字段已是 String）。

---

## 2. Topic 关联会话

### Topic 模型变更

```rust
pub struct Topic {
    pub id: String,
    pub name: String,
    pub status: TopicStatus,
    pub description: String,
    pub scope_in: Vec<ScopeInItem>,
    pub progress: u8,
    pub session_id: Option<String>,  // ← 新增
    pub extra: Option<Value>,
    pub created_at: u128,
    pub updated_at: u128,
}
```

### SQLite 迁移
```sql
ALTER TABLE topics ADD COLUMN session_id TEXT;
```

### TopicStore 变更
- `create_topic()` — `session_id` 参数默认 `None`
- `TopicUpdate` 加 `session_id: Option<Option<String>>`
- `list_incomplete_topics()` — status IN (Todo, InProgress, Paused)

---

## 3. Engine::assistant_mode() — 一次工具调用

| 环节 | Agent | Assistant |
|------|-------|-----------|
| LLM 调用 | 循环直到自然回复 | 一次 |
| 工具执行 | 每次 LLM 回复后检查 | 一次 |
| 结果拼接 | 每轮执行后再次调 LLM | 直接返回 |

```rust
// engine.rs 新增方法
async fn assistant_mode(
    &mut self,
    input: &str,
    conversation_id: String,
    options: ChatOptions,
    system_prompt_override: Option<String>,  // 来自 PreHook1(NeuronContext)
) -> AppResult<ChatResponse> {
    let conversation = self.store.get_conversation(&conversation_id)?;
    let mut context = build_context(&conversation, ConversationMode::Assistant);

    if let Some(sp) = system_prompt_override {
        context.insert(0, ModelMessage {
            role: ModelMessageRole::System,
            content: sp,
        });
    }

    context.push(ModelMessage {
        role: ModelMessageRole::User,
        content: input.to_string(),
    });

    let request = self.build_request(options, context, Some(tool_defs))?;
    let response = self.providers.call_model(request).await?;

    let choice = &response.choices[0];
    let assistant_content = choice.message.content.clone().unwrap_or_default();
    let tool_calls = parse_tool_calls(&choice);

    // 保存 assistant 消息到对话
    self.store.add_message(&conversation_id, Message {
        role: MessageRole::Assistant,
        content: assistant_content.clone(),
        timestamp: now_ms(),
        ..Default::default()
    })?;

    // 执行工具（一次），结果拼接回对话，不继续调 LLM
    if let Some(tool_calls) = tool_calls {
        if !tool_calls.is_empty() {
            let results = self.tool_registry.execute_all(tool_calls).await?;
            for result in results {
                self.store.add_message(&conversation_id, result)?;
            }
        }
    }

    Ok(ChatResponse { conversation_id, response: assistant_content })
}
```

---

## 4. NeuronStore 扩展

NeuronStore 承担**纯数据层**的选择逻辑，不碰 LLM。LLM 调用由上层（Gateway）负责。

### NeuronStore 新增方法

```rust
impl NeuronStore {
    /// 取 top N，按 weight DESC，同权重随机抽取
    pub fn list_top_n(&self, n: usize) -> AppResult<Vec<Neuron>>;

    /// 次生轮次候选：base + BFS 邻居 + 权重补足到 N
    pub fn get_candidates(&self, prev_selected_id: &str, n: usize) -> AppResult<Vec<Neuron>>;

    /// 加减分（±1）
    pub fn adjust_weight(&self, id: &str, delta: i8) -> AppResult<()>;

    /// 神经元总数
    pub fn count(&self) -> AppResult<usize>;
}
```

### 首轮：list_top_n(7)
```
1. SELECT * FROM neurons ORDER BY weight DESC
2. 取前 N 条
3. 第 N 名及之后同权重的 → 随机抽取补齐
```

### 次生轮次：get_candidates(prev_id, 7)
```
1. 获取 base neuron (prev_id)
2. BFS depth=1 获取邻居
3. candidates = [base] + neighbors
4. 不足 7 → 按 weight 从全局补足（排除已有）
5. 同权重随机处理同上
```

### LLM 调用（PreHook1 / NeuronContext）

| 场景 | 谁调用 |
|------|--------|
| 候选数不足时创建神经元 | PreHook1(NeuronContext) → LLM → 返回待创建神经元变更 |
| 7 选 1 | PreHook1(NeuronContext) → LLM → 选 ID |

```rust
// Assistant 业务 Hook 层，遵守 docs/design/hook-spec.md 的 PreHook 协议。
// Hook 不直接访问 store；candidate_count / candidates 由编排器放入 PreCtx。
impl AssistantHooks {
    /// 候选不足时调用 LLM 生成补足神经元，并返回待应用状态变更。
    async fn create_missing_neurons(
        &self,
        missing_count: usize,
    ) -> AppResult<Vec<StateChange>>;

    /// LLM 7 选 1：返回选中 neuron id。
    async fn llm_select_one(
        &self,
        candidates: &[NeuronSnapshot],
        context: &str,
    ) -> AppResult<String>;
}
```

---

## 5. Poller — 通用轮询调度器

### 模块：`core/poller.rs`

通用调度器，支持多业务按不同间隔注册。通过 `PollHandler` trait 注入业务逻辑。

```rust
/// 业务方实现此 trait 注入逻辑
pub trait PollHandler: Send {
    fn on_tick(&mut self);
}

pub struct Poller {
    base_interval_ms: u64,   // 最小粒度（毫秒）
    tasks: Vec<PollTask>,
    tick_count: u64,
    status: PollerStatus,
    pending_trigger: bool,
}

struct PollTask {
    name: String,
    interval_ticks: u64,       // base_interval_ms × interval_ticks
    last_tick: u64,
    handler: Box<dyn PollHandler>,
}

pub enum PollerStatus { Running, Paused }
```

### 方法

```rust
impl Poller {
    pub fn new(base_interval_ms: u64) -> Self;

    pub fn register(
        &mut self,
        name: &str,
        interval_ticks: u64,
        handler: Box<dyn PollHandler>,
    );

    /// 执行一次 tick，到期的任务自动调 on_tick
    pub fn tick(&mut self);

    pub fn start(&mut self);
    pub fn pause(&mut self);
    pub fn resume(&mut self);
    pub fn trigger(&mut self);  // 下次 tick 调用所有 handler
    pub fn status(&self) -> PollerStatus;
}
```

### tick() 逻辑

```
1. if paused || 未到 base_interval → return
2. if pending_trigger → 清标记，后面执行所有 task
3. tick_count += 1
4. 遍历 tasks:
   if tick_count - task.last_tick >= task.interval_ticks:
     task.last_tick = tick_count
     task.handler.on_tick()   // ← 直接调，不返回
```

Poller 不返回任何值，不抛异常。Handler 内部自行处理错误。

### Assistant 模式使用示例

```rust
// assistant_handler.rs — 在 Gateway 内或独立文件
struct AssistantPollHandler {
    topic_store: Arc<Mutex<TopicStore>>,
    conversation_store: ConversationStore,
    session_tracker: SessionTracker,
    engine: Engine,
}

impl PollHandler for AssistantPollHandler {
    fn on_tick(&mut self) {
        let topics = match self.topic_store.lock() {
            Ok(store) => store.list_incomplete_topics().unwrap_or_default(),
            Err(_) => return,
        };
        for topic in topics {
            // ... 检查 session、调 engine
        }
    }
}

// PollHandler 在 Gateway.new() 中注册
let handler = Box::new(AssistantPollHandler {
    topic_store: Arc::clone(&topic_store),
    conversation_store: store.clone(),
    session_tracker: tracker.clone(),
    engine: engine.clone(),
});
poller.register("assistant", 3, handler);
```

### TUI 集成

TUI 以 base_interval 为粒度驱动 tick，不关心内部注册了多少任务：

```rust
// TuiApp.run()
let mut tick_interval = tokio::time::interval(
    Duration::from_millis(self.gateway.poller_base_interval_ms()),
);
loop {
    tokio::select! {
        _ = tick_interval.tick() => {
            self.gateway.poller_tick();  // Poller 内部调度
        }
        action = read_action() => {
            self.update(action).await?;
        }
    }
    terminal.draw(|frame| render(frame, self))?;
}
```

`poller_tick()` 是同步方法（`on_tick` 内如果需要 async，Handler 自行 `tokio::spawn`）。

---

## 6. Hook 系统

Hook 实现必须遵守 `docs/design/hook-spec.md`。通用协议层只定义 `PreCtx` / `PreOut` / `AfterCtx` / `AfterOut` 等类型；Assistant 业务层实现具体 Hook，不再把 Hook 写成 `Gateway` 内置随意方法。

### 文件组织

```text
core/hooks/
  mod.rs
  types.rs
  assistant/
    mod.rs
    topic_match.rs      # PreHook0
    intervention.rs     # PreHook2
    neuron_context.rs   # PreHook1
    after_round.rs      # AfterHook
```

### Assistant Hook 映射

| Hook | 协议类型 | 职责 | 主要输出 |
|------|----------|------|----------|
| TopicMatch | PreHook0 | 用户输入时检索未完成课题，匹配则切换到绑定会话，无匹配则创建课题 | `PreOut::Proceed.context` / `StateChange` |
| Intervention | PreHook2 | 次生轮次检测用户介入、情感、纠偏或重来，对关联神经元评分 | `StateChange` / `Rollback` / `Skip` |
| NeuronContext | PreHook1 | 获取候选神经元，LLM 7 选 1，准备本轮 system prompt | `PromptInjection { role: System }` + `selected_neuron_id` |
| AfterRound | AfterHook | 检查 scope_in 完成情况，重算 progress，必要时标记 Done | `AfterOut::Update` / `AfterOut::Complete` |

### PreHook1 / NeuronContext 逻辑

```
assistant_neuron_context_pre(ctx):
  1. 从 ctx.context 读取 topic、round_index、previous_selected_neuron_id、candidate_neurons
  2. 如果候选不足 7：
     - 调用 LLM 生成补足神经元
     - 返回创建神经元的 StateChange，由编排器写入 NeuronStore
  3. 首轮：按权重优先 + 同权重随机准备候选
  4. 次生轮次：以前次 selected_neuron 为起点 BFS 取邻居，不足则按权重补足
  5. 调用 LLM 从 7 个候选中选 1 个
  6. 返回 PreOut::Proceed:
     - modified_input = 本轮输入
     - prompt_injections = [System(selected.content)]
     - context = [selected_neuron_id]
     - changes = [记录本轮 selected_neuron_id 等需要持久化的状态变更]
```

PreHook1 不再是“提交参数预处理”。它是 Assistant 模式获得神经元 system prompt 的唯一入口，`NeuronSelect` 不再作为独立流水线阶段存在。

### AfterHook / AfterRound 逻辑

```
assistant_after_round(ctx):
  1. 从 ctx.result 读取本轮 assistant 输出与工具结果
  2. 从 ctx.context 读取 topic scope_in 快照
  3. 逐项用 LLM 判断 scope_in 是否完成
  4. 返回 scope_in 状态变更和 progress 变更
  5. 如果全部完成，返回 AfterOut::Complete
```

AfterHook 不直接更新 topic。它只返回 `StateChange`，由 Gateway / Orchestrator 统一应用。

---

## 7. 完整流水线编排

### Gateway / Orchestrator 入口

```
用户输入 → run_assistant_round(input)
Poller   → run_assistant_round(None)  // 自动 prompt
```

```rust
impl Gateway {
    pub async fn run_assistant_round(
        &mut self,
        topic_id: &str,
        user_input: Option<&str>,
    ) -> AppResult<ChatResponse> {
        let snapshot = self.build_assistant_round_snapshot(topic_id, user_input)?;
        let mut prepared_input = snapshot.input.clone();
        let mut prompt_injections = vec![];
        let mut hook_context = snapshot.context.clone();

        // PreHook2：仅次生轮次 + 用户输入时执行。
        if snapshot.is_secondary_round && user_input.is_some() {
            let pre2 = assistant_hooks::intervention_pre(
                self.build_pre_ctx(&snapshot, prepared_input.clone(), hook_context.clone())?,
                Some(&self.providers),
            ).await?;
            let applied = self.apply_pre_out(pre2)?;
            if applied.skip { return Ok(default_response); }
            prepared_input = applied.modified_input;
            hook_context.extend(applied.context);
            self.apply_state_changes(applied.changes)?;
        }

        // PreHook1：神经元上下文准备，输出 system prompt 注入。
        let pre1 = assistant_hooks::neuron_context_pre(
            self.build_pre_ctx(&snapshot, prepared_input.clone(), hook_context.clone())?,
            Some(&self.providers),
        ).await?;
        let applied = self.apply_pre_out(pre1)?;
        prepared_input = applied.modified_input;
        prompt_injections.extend(applied.prompt_injections);
        hook_context.extend(applied.context);
        self.apply_state_changes(applied.changes)?;

        let system_prompt_override = prompt_injections
            .iter()
            .find(|item| matches!(item.role, PromptRole::System))
            .map(|item| item.content.clone());

        let result = self.engine.assistant_mode(
            &prepared_input,
            snapshot.session_id.clone(),
            ChatOptions::default(),
            system_prompt_override,
        ).await?;

        let after = assistant_hooks::after_round(
            self.build_after_ctx(&snapshot, &result, hook_context)?,
            Some(&self.providers),
        ).await?;
        self.apply_after_out(after)?;

        Ok(result)
    }

    /// Poller 调用的 tick 入口
    pub async fn poller_tick(&mut self) -> AppResult<()> {
        let actions = self.poller.tick()?;
        for action in actions {
            match action {
                PollAction::StartSession { topic_id }
                | PollAction::ResumeSession { topic_id, .. } => {
                    self.run_assistant_round(&topic_id, None).await?;
                }
                PollAction::Skip { .. } => {}
            }
        }
        Ok(())
    }
}
```

---

## 8. 文件变更清单

| 操作 | 文件 | 说明 |
|------|------|------|
| 修改 | `core/models.rs` | ConversationMode + Topic.session_id + TopicUpdate |
| 修改 | `core/engine.rs` | assistant_mode() 方法 |
| 修改 | `core/neuron_store.rs` | 新增 list_top_n / get_candidates / adjust_weight / count |
| 修改 | `core/topic_store.rs` | session_id + list_incomplete_topics + 迁移 |
| 新增 | `core/poller.rs` | 通用调度器，无业务属性 |
| 新增 | `core/hooks/types.rs` | 通用 PreHook / AfterHook 协议类型 |
| 新增 | `core/hooks/assistant/*` | Assistant 业务 Hook：TopicMatch / Intervention / NeuronContext / AfterRound |
| 修改 | `core/gateway.rs` | run_assistant_round + assistant_poll_tick + Hook 编排与副作用应用 |
| 修改 | `core/mod.rs` | 注册 poller / hooks 模块 |
| 修改 | `tui/commands.rs` | NewAssistant / Poll 命令 |
| 修改 | `tui/app.rs` | 命令处理 + 定时 poller tick |
| 修改 | `tui/render.rs` | [Assistant] 标签 |

---

## 9. 依赖关系

```
TuiApp (事件循环 + base_interval tick)
  └── Gateway
        ├── Poller (纯调度器，无业务)
        ├── Engine
        │     ├── ProviderRegistry
        │     ├── ToolRegistry
        │     └── ConversationStore
        ├── TopicStore
        ├── NeuronStore
        ├── SessionTracker
        └── hooks
              ├── types（通用 Hook 协议）
              └── assistant（业务 Hook 实现）
```

---

## 10. 实施顺序

1. **数据层**: ConversationMode + Topic.session_id + TopicStore/NeuronStore 扩展
2. **Engine::assistant_mode**: 一次工具调用实现
3. **Hook 协议层**: core/hooks/types.rs 对齐 docs/design/hook-spec.md
4. **Assistant 业务 Hook**: TopicMatch / Intervention / NeuronContext / AfterRound
5. **Poller**: 轮询 + tick + 状态管理
6. **Gateway 编排**: run_assistant_round + Hook 调用 + 副作用应用 + poller_tick
7. **TUI**: /new_assistant + /poll 命令 + [Assistant] 标签
8. **集成测试**: 完整流水线验证
