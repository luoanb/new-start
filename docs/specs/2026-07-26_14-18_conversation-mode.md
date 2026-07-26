# Spec: 会话模式 (Conversation Mode)

## Goal

* 要解决什么问题：
  当前 `chat()` 和 `chat_with_tools()` 是两个独立方法，共享 `message_to_model_message()`，导致同一会话中混用两种模式时 tool 消息被错误转发。需要从编排层约定：**一个会话只能有一种模式**，创建时选定，运行中不可变更。

* 验收结果：

  1. Conversation 有 mode 字段（Chat / Agent），创建时必选
  2. Engine 统一入口 `chat()`，内部根据 mode 分发不同实现
  3. Chat 模式的消息构建跳过 tool 相关消息；Agent 模式的消息构建保留 tool 消息并携带 tool definitions
  4. 跨模式混用：Chat 会话无法执行 `/agent`；Agent 会话无需 `/agent` 前缀，直接输入即走工具循环

## Done Contract

* 什么算完成：

  1. `Conversation` 增加 `mode` 字段，序列化/反序列化兼容旧数据（旧会话默认为 Chat）
  2. `Engine` 对外只暴露 `chat()`，内部 `match mode` 分发实现
  3. `build_context()` 私有方法接受 mode 参数，Chat 模式过滤 tool 消息，Agent 模式保留
  4. Gateway + TUI 适配：创建会话时可选 mode、列表显示 mode、输入约束
  5. 单元测试覆盖：Chat 模式不发送 tools、Agent 模式发送 tools、旧数据兼容

* 由什么证明：
  单元测试覆盖新旧 conversation 读写、mode 分发；手动测试 `/new` 创建 Chat、`/new agent` 创建 Agent、跨模式混用被拒绝。

* 哪些情况仍算未完成：

  * 运行时动态切换 mode（明确不做，创建即锁定）

  * 未来新增模式（如 Streaming、Vision）需要加枚举变体

## Scope

* In:

  * 数据模型：`ConversationMode` 枚举 + `Conversation.mode` 字段

  * Engine 重构：`chat()` 统一入口，抽象 `build_context()` + `call_model_once()`

  * Agent 模式实现：`chat()` → Chat 分支复用现有逻辑；Agent 分支复用 `chat_with_tools()` 现有逻辑

  * Gateway：`create_conversation(mode)`，`send_agent_message` 并入 `send_model_message`

  * TUI：`/new` 默认 Chat，`/new agent` 创建 Agent；会话列表显示 mode 标记；输入按 mode 路由

* Out:

  * 模型 capabilities 校验（Agent 模式要求模型 `tools: true` — 后续再加）

  * 会话列表按 mode 筛选

  * 更多模式（Vision、Streaming）

## 数据模型变更

### ConversationMode 枚举

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMode {
    Chat,
    Agent,
}
```

### Conversation 增加 mode 字段

```rust
pub struct Conversation {
    pub id: String,
    pub mode: ConversationMode,          // ← 新增
    pub messages: Vec<Message>,
    pub created_at: u128,
    pub updated_at: u128,
}
```

**兼容性**：旧会话 JSON 没有 `mode` 字段，反序列化 `#[serde(default)]` 退化为 Chat。

### ConversationStore 适配

```rust
pub fn create_conversation(&self, id: Option<String>, mode: ConversationMode) -> AppResult<Conversation>;
```

## Engine 架构调整

```
Engine::chat(input, conversation_id, options)
  │
  ├─ 公共前置 (私有方法)
  │   ├─ load_conversation()
  │   ├─ get_context_window()
  │   ├─ ensure_fits()
  │   ├─ build_context(mode)       ← 按 mode 过滤消息
  │   │   ├─ Chat  → 跳过 tool_calls / tool_call_id 消息
  │   │   └─ Agent → 保留全部
  │   └─ save_user_message()
  │
  └─ match conversation.mode
       ├─ Chat  → call_model(tools: None) → save_assistant()
       └─ Agent → tool_loop()              → save_assistant()

tool_loop():
  loop (max 20):
    call_model(tools: Some(defs))
    if tool_calls:
      save_assistant(with tool_calls)
      for each tool_call: execute + save Tool result
      append to context
    else:
      save_assistant(text)
      break
```

### 关键变更

| 现方法                          | 目标                                         |
| ---------------------------- | ------------------------------------------ |
| `Engine::chat()`             | 保持签名不变，内部 `match mode`                     |
| `Engine::chat_with_tools()`  | 删除，逻辑内联到 Agent 分支                          |
| `message_to_model_message()` | 保留，Chat 分支在 `build_context()` 中过滤掉 tool 消息 |

### build\_context(mode)

```rust
fn build_context(conversation: &Conversation, mode: ConversationMode) -> Vec<ModelMessage> {
    let summarized = /* 同现有逻辑 */;
    conversation.messages.iter()
        .filter(|m| {
            if m.role == MessageRole::Compaction { return true; }
            !summarized.contains(&m.timestamp.to_string())
        })
        .filter(|m| {
            // Chat 模式：跳过 tool 相关消息
            if mode == ConversationMode::Chat {
                m.tool_calls.is_none() && m.tool_call_id.is_none()
            } else {
                true
            }
        })
        .map(message_to_model_message)
        .collect()
}
```

## Gateway 变化

```rust
impl Gateway {
    // 统一入口：不再区分 send_model_message / send_agent_message
    pub async fn send_message(
        &mut self,
        input: &str,
        options: ChatOptions,
    ) -> AppResult<ChatResponse> {
        // ... require_model ...
        // ... resolve_conversation_id ...
        self.engine.chat(input, conversation_id, options).await
    }

    pub fn create_conversation(&mut self, id: Option<String>, mode: ConversationMode) -> AppResult<String>;
}
```

## TUI 交互变化

### `/new` 命令扩展

```
/new          → 创建 Chat 模式会话（默认）
/new agent    → 创建 Agent 模式会话
```

### 会话列表

```
conv_abc123  [Chat]   12 messages
conv_def456  [Agent]  5 messages
```

会话列表底部新增创建入口（始终显示，不计入会话数）：

```
  conv_abc123  [Chat]   12 messages
  conv_def456  [Agent]  5 messages
  ─────────────────────
  + New Chat session        ← 选中后创建 Chat 模式会话
  + New Agent session       ← 选中后创建 Agent 模式会话
```

用户用上下键导航到这两个条目并确认，即可创建对应模式的会话。

### 输入约束

* Chat 会话：直接输入走 Chat 模式，`/agent` 命令被拒绝（"当前会话为 Chat 模式，请使用 /new agent 创建 Agent 会话"）

* Agent 会话：直接输入也走 Agent 模式（带 tools），不再需要 `/agent` 前缀

`/agent` 命令可以保留作为"在当前 session 强制走一次 agent"的 fallback（仅限 Agent 模式会话）。

## 实施顺序

1. **数据模型**：`ConversationMode` 枚举 + `Conversation.mode` 字段 + 默认值兼容
2. **ConversationStore**：`create_conversation()` 接受 mode
3. **Engine 重构**：`build_context(mode)` + `chat()` match 分发 + 删除 `chat_with_tools()`
4. **Gateway**：统一 `send_message()` + `create_conversation(mode)`
5. **TUI 命令**：`/new` 扩展 + 会话列表 mode 标记 + 输入约束
6. **测试**：单元测试 + 手动测试

## 旧代码清理

* 删除 `Engine::chat_with_tools()` 和 `Gateway::send_agent_message()`

* 删除 TUI 中 `Command::Agent`、`TuiApp::send_agent_message()`

* `/agent` 命令说明改为提示创建 Agent 会话

