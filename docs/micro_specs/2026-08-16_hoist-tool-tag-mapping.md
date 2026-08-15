# Spec: 上移模式→标签工具映射（call_service 去业务语义）

## Goal

- 消除 `call_service`（通用模型调用服务）中对 `ConversationMode` 的业务判断：`call_service` 不应感知「什么模式注入什么标签工具」。
- 把「模式 → 标签工具集」的映射收敛到领域模型（`ConversationMode`）上，由会话/路由层消费，`call_service` 只做数据驱动的工具组装。
- 纯重构：行为完全等价，不改任何运行语义。

## 现状与问题

`call_service.rs` converse 工具授权段硬编码：

```rust
if let Some(mode) = input.mode.as_ref() {
    if *mode != ConversationMode::Chat {
        final_ids.extend(guard.tools_with_tag(ToolTag::Core));
        if *mode == ConversationMode::System {
            final_ids.extend(guard.tools_with_tag(ToolTag::System));
        }
    }
}
```

问题：

- `call_service` 是所有模式（Chat / Agent / Assistant / System）与无模式裁决（judgement / match_topic / complete_scope）共用的通用层，却替各模式决定了「注入哪些标签工具」。
- 模式与标签的耦合散落在 callee 内部；新增对话模式（或新增标签）时 `call_service` 必须跟着改，且 gateway 路由注释（`System = Assistant + 系统工具`）与这里双重维护。
- `mode: Option<ConversationMode>` 的语义被重载（`None` 表示"非对话调用不注入"），调用方意图藏在 callee 里。

## 方案

把「模式 → 标签集」做成领域映射 `ConversationMode::tool_tags()`，`RoundInput.mode` 改为 `RoundInput.tool_tags: Vec<ToolTag>`（纯数据），`call_service` 遍历并入。

| 模式 | tool_tags | 与原行为等价 |
|---|---|---|
| Chat | `[]` | 原 `mode=Chat` 不并入 ✓ |
| Agent / Assistant | `[Core]` | 原非 Chat 并入 Core ✓ |
| System | `[Core, System]` | 原 System 再并入 System ✓ |
| 裁决/非对话（原 `mode=None`） | `[]` | 原不注入 ✓ |

## Done Contract

- `models.rs`：`ConversationMode::tool_tags(&self) -> Vec<ToolTag>`（模式→标签领域映射；`Chat` → `[]`，`Agent`/`Assistant` → `[Core]`，`System` → `[Core, System]`）。
- `call_service.rs`：
  - `RoundInput.mode: Option<ConversationMode>` 删除，新增 `pub tool_tags: Vec<ToolTag>`（默认空）。
  - converse 标签并入段改为：`for tag in &input.tool_tags { final_ids.extend(guard.tools_with_tag(*tag)); }`，删除全部 `ConversationMode` 判断；注释改为「标签工具由调用方按模式注入（`ConversationMode::tool_tags`），service 仅并入」。
- `conversation_runner.rs`：`RoundInput` 构造处 `mode: Some(ctx.mode.clone())` → `tool_tags: ctx.mode.tool_tags()`。
- `assistant_session.rs`：`call_judgement` 构造处 `mode: None` → `tool_tags: Vec::new()`（裁决禁工具语义不变）。
- 测试构造点同步：`call_service.rs` tests 中 `mode: None` → `tool_tags: Vec::new()`，`mode: Some(Chat)` → `tool_tags: Vec::new()`，`mode: Some(System)` → `tool_tags: vec![ToolTag::Core, ToolTag::System]`。
- gateway 路由注释（`System = Assistant 附加系统工具，工具并入在 call_service 授权段完成`）更新为「标签并入由 `ConversationMode::tool_tags` 决定，会话层透传」。

## 兼容性

- 不改工具 wire 内容：任何模式下最终 `final_ids`（标签工具 + 策略工具去重保序）与重构前逐位一致。
- `RoundInput` 无序列化要求（内存结构），字段更名无迁移。
- `mode` 字段若仍有他处使用（如 hooks 读 `ctx.mode` 判断触发语义），保留在 `RoundContext`，仅从 `RoundInput` 移除。

## Validation

- `cargo test`（218+ 用例）全部通过。
- 可选冒烟：真实服务（deepseek-v4-flash）Assistant 会话一轮，确认 wire 工具与重构前一致（8 个授权工具，含 Core/System 标签项）。

## 改动点

| 文件 | 改动 |
|---|---|
| `src/core/models.rs` | 新增 `ConversationMode::tool_tags()` |
| `src/core/call_service.rs` | `RoundInput` 字段替换；标签并入段数据驱动；tests 构造点同步（~18 处） |
| `src/core/conversation_runner.rs` | `RoundInput` 构造改用 `tool_tags` |
| `src/core/assistant_session.rs` | `call_judgement` 构造改用 `tool_tags` |
| `src/core/gateway.rs` | 注释同步 |

## Change Log / Validation（2026-08-16）

- 实现摘要：
  - `models.rs`：`ConversationMode::tool_tags()`——Chat→`[]`，Agent/Assistant→`[Core]`，System→`[Core,System]`。
  - `call_service.rs`：`RoundInput.mode: Option<ConversationMode>` → `tool_tags: Vec<ToolTag>`；标签并入段改为 `for tag in &input.tool_tags` 遍历；日志字段 `mode`→`tool_tags`；移除未用 import。
  - `conversation_runner.rs`：构造处 `mode: Some(ctx.mode.clone())` → `tool_tags: ctx.mode.tool_tags()`。
  - `assistant_session.rs`：`call_judgement` 构造 `mode: None` → `tool_tags: Vec::new()`。
  - `gateway.rs`：System 路由注释同步。
- 验证：`cargo check` 通过；`cargo test`：218 passed, 0 failed。工具标签注入测试（Chat 空 / System=[core_echo,sys_echo] / 非对话空）原样通过，确认行为等价。
