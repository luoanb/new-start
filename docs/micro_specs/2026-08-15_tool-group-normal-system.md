# Spec: 工具标签 ToolTag（Core / System / Normal）

## Goal

- 要解决什么问题：工具注册目前只有一个 `ToolSource`（native/config/mcp，来源维度）治理字段，缺少"用途/行为"维度的标注——无法表达"这个工具该进哪些对话"。分组的本意是**给工具打一个标**（ToolTag），并让每个标签的消费语义明确。
- 消费语义（既定契约，用户定义）：
  - **Core**：任何对话都得带上的工具 → 无条件进入所有会话的 tools wire。
  - **System**：系统对话自动带上的工具（调整系统运行本身行为）→ 发起会话时可选"系统模式"（= 助手模式附加系统工具），该模式自动带上 System 标签工具（形态已定：会话创建时的模式选项；wire 注入实现排期后续）。
  - **Normal**（默认）：不由系统自动带，由神经元管理 → 神经元持有哪些就带哪些（= 现状 `tool_ids` 白名单逻辑，行为不变）。
- 验收结果：工具有 `ToolTag` 标签（Core / System / Normal，默认 Normal）；注册时可打标，外部工具默认 Normal、**面板注册时用户可自由指定 tag**；三标签消费语义随治理视图与前端展示落地；现有工具完成初步打标（内置工具全部 Core）。

## Done Contract

- 什么算完成：
  1. 新增 `ToolTag` 枚举（`core` / `system` / `normal`，默认 `normal`），枚举可扩展（后续可加正交标签，如 Dangerous / Network）。
  2. 注册 API 支持打标：`register()`（默认 Normal，向后兼容）、`register_core()`、`register_system()`；Config / MCP 等**外部注册默认 Normal**。
  3. 存储与透传：`ToolBox` 存 `tag`；`ToolDefinition` 加 `tag` 治理字段（同 `source`：`skip_serializing` 不进模型 wire）；`ToolInfo`（前端 `list_tool_info`）带 `tag`。
  4. 内置工具全部打标 **Core**（`execute_command`、`get_current_time`；用户决策，后续开发可再调整）。
  5. **面板注册可指定 tag**：工具配置（`dynamic_tools.json` / MCP server 配置）支持 `tag` 字段；面板注册工具时用户可自由指定（默认 Normal），注册时按配置打标。
  6. 前端 ToolPanel 按标签显示徽标（复用现有 source 徽标样式体系）。
- 由什么证明：`cargo test` 覆盖默认 Normal、显式打标、外部注册默认 Normal、wire 不含 `tag`；前端 `pnpm run check` 无新增 error/warning。
- 哪些情况仍算未完成：消费语义的 **wire 注入实现**（Core 并入所有对话 / System 并入系统模式会话）与"系统模式"会话创建入口，属后续排期（消费语义契约已定，见 Goal）。

## Scope

- In：`models.rs`（`ToolTag`、`ToolDefinition`/`ToolInfo` 加字段）、`tool_registry.rs`（打标注册 API）、`gateway.rs`（注册点打标）、`tool_config.rs` + `dynamic_tool.rs` + `mcp.rs`（配置 tag 字段与注册透传）、`types.ts` + `ToolPanel.svelte` + 工具配置编辑器（前端标签展示与指定）。
- Out：`call_service.rs` 授权链 wire 消费注入、系统模式会话创建入口、动态改标、标签组合（多标签数组）。

## Facts / Constraints

- 现状：`ToolRegistry` = `HashMap<String, ToolBox>`，`ToolBox` 仅 `tool + source`（[tool_registry.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/tool_registry.rs#L24-L34)）；`ToolDefinition` 已有 `source` 治理字段先例（`skip_serializing` 不进 wire，[models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L362-L370)）。
- 注册集中点：[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L964-L977)（native+config）与 `assemble_mcp_progressive`（MCP）。
- 会话模式现有 Chat / Agent / Assistant（[models.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/models.rs#L91-L95)），**无"系统对话"**——System 标签的消费入口属于后续排期。
- wire 硬约束：`tag` 只能做治理字段（同 `source`），不进 OpenAI 协议 payload。
- 现状工具授权链（[call_service.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/call_service.rs#L229-L254)）为白名单制：`tool_override` / `behavior.tools`（None / FromNeuron / Allowlist）→ ∩ 注册表 → wire。Normal 标签的语义与现状一致，无需改动。

## 接口契约设计

### 1. 标签枚举（models.rs）

```rust
/// 工具标签（用途/行为维度）。与 ToolSource（来源维度）正交。
/// Core：任何对话都得带上；System：系统对话自动带上；Normal：由神经元管理（默认）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolTag {
    #[default]
    Normal,
    System,
    Core,
}
```

### 2. 治理字段（models.rs）

`ToolDefinition` 与 `ToolInfo` 增加（对齐 `source`，不进 wire）：

```rust
#[serde(skip_serializing, default)]
pub tag: ToolTag,
```

### 3. 注册 API（tool_registry.rs）

```rust
impl ToolRegistry {
    /// 普通标签（默认，向后兼容）：Native 需 inserts/<name>.md 门禁。
    pub fn register(&mut self, tool: impl Tool + 'static);
    /// 系统标签：系统对话自动带上的工具。
    pub fn register_system(&mut self, tool: impl Tool + 'static);
    /// Core 标签：任何对话都得带上的工具。
    pub fn register_core(&mut self, tool: impl Tool + 'static);
    /// 底层：tag + source + 门禁。外部（Config/Mcp）注册未显式指定时默认 Normal。
    pub fn register_tagged(&mut self, tag: ToolTag, tool: impl Tool + 'static, source: ToolSource);
}
```

### 4. 内置工具打标（gateway.rs）

| 工具 | 标签 | 依据 |
|------|------|------|
| `execute_command` | Core | 用户决策：内置工具全部 Core（后续开发可再调整） |
| `get_current_time` | Core | 同上 |
| Config `HttpTool` / `CommandTool` | 面板指定，默认 Normal | 外部工具默认 Normal，面板注册可指定 |
| MCP 工具 | 面板指定，默认 Normal | 同上 |

### 5. 前端展示（ToolPanel）

工具行在现有 `source` 徽标旁增加 `tag` 徽标（core=primary / system=warning 色系，normal 不显式显示），样式复用现有 source-dot 体系。

### 6. 面板注册指定 tag（配置层）

- `dynamic_tools.json`：`HttpToolConfig` / `CommandToolConfig` 增加 `tag?: ToolTag`（默认 normal）；前端工具配置编辑器提供 tag 选择控件。
- MCP server 配置：`McpServerConfig` 增加 `tag?: ToolTag`（该 server 下全部工具打此标）。
- 注册时：配置有 `tag` → 按配置打标；无 → 默认 Normal（[gateway.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L964-L977) 与 `assemble_mcp_progressive` 注册点透传）。

## Open Questions

- [x] "系统模式"形态：发起会话时可选系统模式（= 助手模式附加系统工具）——已定，见 Goal。
- [x] 正交附加标签（Dangerous / Network）：暂不需要，`ToolTag` 枚举可扩展——已定。
- [x] 内置工具标签：全部 Core——已定。
- [ ] 面板注册指定 tag 的配置形态细节：字段名 / 编辑器控件 / 校验（配置 `tag` 非法值时回退 Normal 并 warn）——实现时确认。

## Restated Understanding

- 我理解当前任务是：给工具打标（ToolTag：Core / System / Normal），三标签的**消费语义已由用户定义清楚**（Core 任何对话都带、System 系统模式会话自动带、Normal 由神经元管理 = 现状逻辑），作为本 spec 的既定契约；第一版交付打标机制 + 语义落地（治理字段 + 面板可指定 tag + 前端展示），wire 注入消费实现排期后续。
- 当前核心目标是：`ToolTag` 枚举 + 注册打标 API + 治理字段透传 + 内置工具全部 Core + 面板注册指定 tag + 前端标签展示。
- 当前边界是：不做 wire 消费注入、不做系统模式会话入口、不做运行时改标。

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是，落盘 micro-spec 是执行前必要 spec。
- 若否：N/A。

## Checkpoint Summary

- 当前任务理解：工具打标（ToolTag 三值），消费语义为既定契约（系统模式形态已定），wire 注入实现排期后续。
- 当前核心目标：打标机制 + 默认 Normal + 面板可指定 tag + 内置工具 Core + 前端展示。
- 当前进度：方案经多轮讨论收敛，用户四项决策（系统模式 / 枚举可扩展 / 内置工具 Core / 面板指定 tag）已并入，落盘待评审。
- 下一步 1：用户确认方案 → 实现 `ToolTag` + 注册 API + 配置透传 + 前端徽标 → `cargo test` / `pnpm run check`。
