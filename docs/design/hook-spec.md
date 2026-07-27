# Hook 设计规范

## 1. 定义

Hook 是对话模式流水线上的通用决策节点。协议由 `docs/design/hook-spec.md` 统一约定，具体业务只能实现该协议，不能各自发明一套输入输出形状。

- **PreHook**：在主处理之前触发，可修改输入、注入 prompt/context、跳过或终止本轮。
- **AfterHook**：在主处理之后触发，可评估结果并返回状态变更、完成、回滚等后续动作。

适用对象包括 Chat、Agent、Assistant 等对话模式。不同模式可以有不同业务 Hook，但它们共享同一组 Ctx / Out 语义。

## 2. 通用原则

| 规则 | 说明 |
|------|------|
| 协议通用 | PreHook / AfterHook 的 Ctx 和 Out 语义对所有对话模式一致 |
| 业务隔离 | TopicMatch、NeuronContext、AfterRound 等属于业务 Hook，不写进通用协议类型名 |
| Ctx 由编排器构造 | Hook 不直接从 store 拉完整 Model；编排器加载数据后构造快照 |
| 副作用由编排器执行 | Hook 不写 DB、不改 store、不直接回滚会话，只返回决策 |
| Hook 结果默认不进对话 | 除非编排器明确把某个字段作为消息写入，否则 Hook 输出只影响本轮处理 |
| 显式顺序 | 多个 Hook 按对话模式流水线显式调用，不依赖隐式注册顺序 |

## 3. PreHook 规范

### 3.1 时机

```
输入 / 自动触发 → [PreHook...] → 主处理
```

### 3.2 接口

```rust
pub async fn exec_pre(
    ctx: PreCtx,
    llm: Option<&ProviderRegistry>,
) -> AppResult<PreOut>
```

业务实现可以使用更具体的函数名，例如 `assistant_neuron_context_pre`，但入参和出参必须能映射到 `PreCtx` / `PreOut`。

### 3.3 PreCtx — 处理前上下文快照

```rust
pub struct PreCtx {
    /// 本轮原始输入；Poller 等无用户输入场景可由编排器填入自动 prompt。
    pub raw_input: String,
    /// 当前对话模式。
    pub mode: ConversationMode,
    /// 当前处理对象快照，如 conversation / topic / task。
    pub target: TargetSnapshot,
    /// 本轮触发来源，如 user / poller / command。
    pub trigger: TriggerSource,
    /// 可选上下文条目，如绑定课题、候选神经元、最近消息摘要。
    pub context: Vec<ContextItem>,
}
```

- `raw_input`：本轮输入文本，Hook 可基于它判断，也可输出替换后的输入。
- `mode`：当前对话模式，便于通用编排器选择对应业务 Hook。
- `target`：摘要快照，不传完整 Model。
- `trigger`：区分用户输入、轮询、命令等触发来源。
- `context`：编排器提前加载好的上下文，值使用 JSON 字符串承载。

### 3.4 PreOut — 主处理前决策

```rust
pub enum PreOut {
    /// 继续处理，携带本轮主处理需要的输入和上下文注入。
    Proceed {
        modified_input: String,
        prompt_injections: Vec<PromptInjection>,
        context: Vec<ContextItem>,
        changes: Vec<StateChange>,
    },
    /// 跳过本轮处理。
    Skip { reason: String },
    /// 终止流水线。
    Abort { reason: String },
}
```

- `modified_input`：替换原始输入，供主处理使用。
- `prompt_injections`：向模型上下文注入 prompt，例如 Assistant 选中神经元的 system prompt。
- `context`：传给后续 Hook 或主处理的附加上下文，例如 selected_neuron_id。
- `changes`：Hook 建议的状态变更，由编排器决定何时应用。
- `Skip`：跳过本轮，不进入主处理。
- `Abort`：终止流水线并返回错误。

## 4. AfterHook 规范

### 4.1 时机

```
主处理 → [AfterHook...]
```

### 4.2 接口

```rust
pub async fn exec_after(
    ctx: AfterCtx,
    llm: Option<&ProviderRegistry>,
) -> AppResult<AfterOut>
```

业务实现可以使用更具体的函数名，例如 `assistant_after_round`，但入参和出参必须能映射到 `AfterCtx` / `AfterOut`。

### 4.3 AfterCtx — 处理后上下文快照

```rust
pub struct AfterCtx {
    /// 本轮原始输入。
    pub raw_input: String,
    /// 当前对话模式。
    pub mode: ConversationMode,
    /// 处理完成后的结果。
    pub result: ProcessResult,
    /// 当前处理对象快照。
    pub target: TargetSnapshot,
    /// PreHook 或编排器传入的上下文。
    pub context: Vec<ContextItem>,
}

pub struct ProcessResult {
    pub output: String,
    pub tool_results: Vec<String>,
    pub finish_reason: String,
}
```

### 4.4 AfterOut — 主处理后决策

```rust
pub enum AfterOut {
    /// 无需额外操作。
    Continue,
    /// 应用一组状态变更。
    Update { changes: Vec<StateChange> },
    /// 标记目标完成，可附带状态变更。
    Complete { changes: Vec<StateChange> },
    /// 回滚到指定锚点。
    Rollback { to: RollbackTarget, reason: String },
}
```

AfterHook 不直接更新 topic、conversation 或 neuron。它只返回 `StateChange` 或 `RollbackTarget`，由编排器统一应用。

## 5. 类型定义

```rust
pub enum TriggerSource {
    User,
    Poller,
    Command,
}

/// 处理对象快照（摘要，非完整 Model）
pub struct TargetSnapshot {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub extra: Vec<(String, String)>,
}

/// 上下文条目
pub struct ContextItem {
    pub key: String,
    pub value: String,
}

/// Prompt 注入
pub struct PromptInjection {
    pub role: PromptRole,
    pub content: String,
    pub source: String,
}

pub enum PromptRole {
    System,
    Developer,
    User,
}

/// 状态变更，由编排器解释和应用
pub struct StateChange {
    pub target: String,
    pub field: String,
    pub value: String,
}

pub enum RollbackTarget {
    Timestamp(u128),
    MessageId(String),
    Checkpoint(String),
}
```

## 6. 对话模式编排

### 6.1 编排器职责

**流水线编排器**持有 Hook 调用权，负责：

1. 从 store 加载数据，构造 `PreCtx` / `AfterCtx`。
2. 按当前对话模式显式调用业务 Hook。
3. 把 `PreOut::Proceed` 的 `prompt_injections` 和 `context` 传给主处理。
4. 根据 `changes` / `Rollback` 执行副作用。
5. 记录需要持久化的状态，但不默认持久化 Hook 原始输出。

### 6.2 调用方式

- 通用协议不强制定义 Hook trait。
- 业务 Hook 可以是独立函数、方法或轻量服务对象。
- 无论组织方式如何，输入输出必须遵守本规范。

```rust
impl Orchestrator {
    async fn process(&self, input: &str) -> AppResult<Output> {
        let pre_ctx = self.build_pre_ctx(input)?;
        let pre_out = assistant_hooks::neuron_context_pre(
            pre_ctx,
            Some(&self.providers),
        ).await?;

        let prepared = self.apply_pre_out(pre_out)?;
        let result = self.engine.process(prepared).await?;

        let after_ctx = self.build_after_ctx(result)?;
        let after_out = assistant_hooks::after_round(
            after_ctx,
            Some(&self.providers),
        ).await?;

        self.apply_after_out(after_out)?;
        Ok(result)
    }
}
```

## 7. Assistant 业务 Hook 映射

Assistant 模式的业务需求按本协议落地：

| 业务 Hook | 类型 | 职责 |
|-----------|------|------|
| TopicMatch | PreHook0 | 用户输入时匹配未完成课题，决定切换会话或创建课题 |
| Intervention | PreHook2 | 次生轮次用户介入检测、神经元评分、必要时请求回滚 |
| NeuronContext | PreHook1 | 准备候选神经元，LLM 7 选 1，输出 selected_neuron_id 和 system prompt 注入 |
| AfterRound | AfterHook | 检查 scope_in 完成情况，返回 progress / status 等状态变更 |

`NeuronContext` 必须通过 `PreOut::Proceed.prompt_injections` 注入选中 neuron 的 `content`，通过 `PreOut::Proceed.context` 传递 `selected_neuron_id`。它不是独立于 Hook 系统之外的 `NeuronSelect` 阶段。

## 8. 文件组织建议

```
hooks/
  mod.rs
  types.rs              # PreCtx / PreOut / AfterCtx / AfterOut / shared types
  assistant/
    mod.rs
    topic_match.rs      # PreHook0
    intervention.rs     # PreHook2
    neuron_context.rs   # PreHook1
    after_round.rs      # AfterHook
```

---

**版本**: v1.1  
**状态**: 生效
