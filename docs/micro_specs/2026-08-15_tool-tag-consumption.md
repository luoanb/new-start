# Spec: 工具标签消费（Core 注入 / System 模式）

## Goal

- 要解决什么问题：第一版已完成打标（`ToolTag`：Core / System / Normal），但消费语义未落地——发起对话时 tools wire 不会自动带上 Core / System 标签工具。本批实现消费逻辑。
- 消费语义（既定契约，用户定义）：
  - **Core**：带工具能力的对话（Agent / Assistant / System）都得带上的工具 → 并入这些会话的 tools wire；**Chat 模式禁用工具，不注入**。
  - **System**：系统对话自动带上的工具 → 仅"系统模式"会话（发起会话时可选，= 助手模式附加系统工具）并入 tools wire。
  - **Normal**（默认）：由神经元管理 → 神经元持有哪些就带哪些（现状 `tool_ids` 白名单逻辑，行为不变）。
- 验收结果：发起 Agent / Assistant / System 对话，Core 工具自动进入 wire；发起"系统模式"会话，Core + System 工具自动进入 wire；**Chat 模式不注入任何工具**；Normal 工具仍由神经元 / override 决定；系统模式成为会话创建的可选项（前后端 + TUI）。

## Done Contract

- 什么算完成：
  1. `ConversationMode` 新增 `System` 枚举值（前后端 + TUI 可创建/识别）。
  2. `RoundInput` 携带 `mode`；`converse()` 授权段消费标签：
     - Core 工具并入 Agent / Assistant / System（**Chat 模式不注入，禁用工具**）；
     - System 工具仅 `mode == System` 并入；
     - 其余按现状 `tool_override` / `behavior.tools`（∩ 注册表）。
  3. `ToolRegistry` 新增 `tools_with_tag(tag) -> Vec<String>`；工具执行授权校验自动跟随（wire 内即可调）。
  4. 前端：新建会话模式选项增加"系统模式"，并**隐藏 Agent 选项**（先隐藏，后端与历史会话保留）；会话列表/徽标支持 System。
  5. TUI：模式解析与渲染支持 System；NewSession 入口同样隐藏 Agent。
- 由什么证明：`cargo test`（converse 单测：Core 全模式进 wire、System 仅系统模式进 wire、Normal 不受影响）；前端 `pnpm run check` 无新增 error/warning。
- 哪些情况仍算未完成：运行时改标、标签组合（多标签数组）、系统模式与神经元组合的进一步语义（见 Open Questions）。

## Scope

- In：`models.rs`（`ConversationMode::System`、`RoundInput.mode`）、`tool_registry.rs`（`tools_with_tag`）、`call_service.rs`（授权段并入标签）、`conversation_runner.rs` + `assistant_session.rs`（构造 RoundInput 传 mode）、`lib.rs`（mode 解析 "system"、open_session seed）、`tui/app.rs` + `tui/render.rs`（System 分支）、前端 `types.ts` + `SessionCreateModal.svelte` + `SessionList.svelte` + `i18n`。
- Out：运行时改标、标签组合、Normal 管理逻辑改动。

## Facts / Constraints

- 现状授权链（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L229-L254)）：`tool_override` 优先，否则 `behavior.tools`（None/FromNeuron/Allowlist）→ `filter_authorized_tool_ids` ∩ 注册表 → `definitions_for` → wire。
- `RoundInput`（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L138-L152)）现无 mode；构造点 = `conversation_runner.rs:105`（生产主路径）+ `assistant_session.rs:116` + 测试 14 处。
- 会话模式解析（[lib.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L86-L90)）：`create_conversation` / `open_session` 按字符串映射；`open_session` 空 spec + Assistant → `SessionSeed::Global`（[lib.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/lib.rs#L628-L632)）。
- 前端模式选择：`SessionCreateModal.svelte`（MODES 数组 chat/agent/assistant）→ `dataStore.createConversation(mode)` → `create_conversation`；会话徽标 `SessionList.svelte`（mode-badge）。
- TUI：`render.rs:375-377` 模式徽标、`app.rs` 三处 NewSession 分支。
- 执行授权校验（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L301-L306)）：校验 `authorized_tool_ids` 包含被调工具名——只要 wire 并入时该集合已含 Core/System，无需额外改动。

## 接口契约设计

### 1. ConversationMode（models.rs）

```rust
pub enum ConversationMode {
    Chat,
    Agent,
    Assistant,
    /// 系统模式：发起会话时可选（= 助手模式附加系统工具），自动并入 System 标签工具。
    System,
}
```

### 2. RoundInput（call_service.rs）

```rust
pub struct RoundInput {
    pub seed: Option<SessionSeed>,
    pub state: SessionState,
    pub messages: Vec<ModelMessage>,
    pub tool_override: Option<Vec<String>>,
    pub reselect: bool,
    /// 会话模式：converse 依此并入 Core / System 标签工具。
    pub mode: ConversationMode,
}
```

`RoundInput` 派生 `Default`（seed/messages/tool_override 为 None/空、state 默认、mode=Chat），测试构造点改用 `..Default::default()` 或补字段。

### 3. 注册表（tool_registry.rs）

```rust
/// 返回所有带指定标签的工具名（注册序）。
pub fn tools_with_tag(&self, tag: ToolTag) -> Vec<String> {
    self.tools
        .iter()
        .filter(|(_, tb)| tb.tag == tag)
        .map(|(name, _)| name.clone())
        .collect()
}
```

### 4. converse 授权段（call_service.rs L242-254 改写）

```rust
let (authorized_tool_ids, tools) = {
    let guard = self.tool_registry.read()...;
    // 标签消费：Core 无条件并入；System 仅系统模式并入。
    let mut final_ids = guard.tools_with_tag(ToolTag::Core);
    if input.mode == ConversationMode::System {
        final_ids.extend(guard.tools_with_tag(ToolTag::System));
    }
    // 现状策略（override / behavior）结果照旧并入。
    let authorized_tool_ids = filter_authorized_tool_ids(&guard, &tool_ids);
    final_ids.extend(authorized_tool_ids);
    // 去重保序：Core 在前，策略工具在后；同名的按首个出现。
    final_ids.sort();
    final_ids.dedup();
    let tools = if final_ids.is_empty() {
        None
    } else {
        Some(guard.definitions_for(&final_ids))
    };
    (final_ids, tools)
};
```

> 说明：Core/System 来自注册表本身，天然在"∩ 注册表"内；`final_ids` 即执行授权集合（L301 校验沿用）。

### 5. mode 解析与 seed（lib.rs）

- `create_conversation` / `open_session`：`"system" => ConversationMode::System`。
- `open_session` seed：`"" if mode == Assistant || mode == System => Some(SessionSeed::Global)`（系统模式沿用助手模式的全域选型，仅附加 System 工具）。

### 6. 前端

- `types.ts`：`Conversation.mode: "chat" | "agent" | "assistant" | "system"`（agent 保留，历史会话可读）。
- `SessionCreateModal.svelte`：MODES 增加 `{ id: "system", label: t("createModal.systemLabel"), desc: t("createModal.systemDesc") }`；**移除 agent 项（先隐藏，Agent 不提供新建入口）**，保留 chat / assistant / system。
- `SessionList.svelte`：`.mode-badge.system` 样式（复用 warning 系或独立色）；agent 徽标样式保留（历史会话仍显示）。
- i18n：`createModal.systemLabel / systemDesc` 中英文。

### 7. TUI

- `app.rs` NewSession 分支：隐藏 Agent（不再提供新建 Agent 会话入口），增加 System；`render.rs` 徽标加 `[System]`（agent 徽标保留）。

## Open Questions

- [ ] 系统模式 + 显式神经元（`spec_neuron_id` 非空）：seed 走 `Neuron(id)`，System 标签仍并入（Core ∪ System ∪ 神经元 FromNeuron）——语义是否要限制（System 模式仅允许空 spec）？默认不限制。
- [ ] `assistant_session.rs` 构造 RoundInput 的 mode 取会话实际 mode 还是固定 Assistant：默认取会话实际 mode。
- [ ] Agent 模式 `tool_override = 全部工具`：Core 并入无冲突（已在其中）——不特殊处理。

## Restated Understanding

- 我理解当前任务是：落地消费语义——Core 无条件并入所有会话 wire，System 仅系统模式会话并入，Normal 保持神经元管理；"系统模式"成为会话创建可选项（= 助手模式附加系统工具）。
- 当前核心目标是：`ConversationMode::System` + `RoundInput.mode` + `tools_with_tag` + 授权段标签并入 + 前端/TUI 模式入口。
- 当前边界是：不改 Normal 管理、不做运行时改标、不做标签组合。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：标签消费落地（Core 注入 / System 模式）。
- 当前核心目标：消费逻辑 + 系统模式入口（前后端 + TUI）。
- 当前进度：**已实现并验证完成**。
  - 后端：`ConversationMode::System`、`RoundInput.mode: Option<ConversationMode>`（非对话调用传 `None` 不注入）、`ToolRegistry::tools_with_tag`、converse 授权段并入（Core 无条件 / System 仅系统模式 / 去重保序）、gateway 路由 System → assistant.converse、lib.rs / net/rpc.rs 解析 `"system"` + open_session seed（空 spec 与 Assistant 同为 Global）、TUI（app.rs NewSystem 分支、render.rs `[System]` 徽标 + 列表条目、select_current_session 索引映射）。
  - 测试：`tag_consumption_into_wire` 单测（Chat 仅 core / System 含 sys / 非对话无工具）+ 既有 14 处 RoundInput 构造点补 `mode: None`。
  - 前端：types.ts `"system"` 类型、SessionCreateModal（隐 agent、加 system）、SessionList 徽标、i18n 中英。
- 验证：`cargo test -p pulsar-app` 218 通过；`pnpm run check` 仅 vite.config.js 既有 5 error（与本次无关），本次改动文件 0 error/warning。
