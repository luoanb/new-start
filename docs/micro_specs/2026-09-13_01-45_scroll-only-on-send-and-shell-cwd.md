# Spec: 修复——仅发送时滚动 + shell 默认工作目录

来源：用户验收「用户介入轮耗时」时报告的两条缺陷（2026-09-13）。

## Goal

- 要解决什么问题：
  1. 助手模式一轮会追加多条消息（工具调用 / 工具结果 / 正文 / 下一轮），模型每次回复更新都把视图拽到底部，打断阅读。
  2. Agent 的 `execute_command` 未传 `cwd` 时用**进程 cwd**，没有回退到用户选中的工作区根。
- 验收结果：
  1. 视口只在用户主动发送时滚动（把新问题对齐顶部）；模型回复 / 工具推进一律不动视口。
  2. `execute_command` 未显式指定 `cwd` 时在同会话的工作区根下执行。

## Done Contract

- 什么算完成：滚动自动跟随分支移除、只保留发送吸顶；`ExecuteCommandTool` 注入工作区存储并在缺省时解析 active 工作区根。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；`cargo test --lib` 全绿；App 内发送一条输入 + 跑一次 shell 工具确认。
- 哪些情况仍算未完成：仅改代码未编译验证；或显式 `cwd` 被覆盖（显式必须优先）。

## Scope

- In：`ChatArea.svelte` 滚动状态机；`tools/cmd_exec.rs` 默认 cwd 解析；`FileToolContext` 暴露工作区句柄；`gateway::assemble_local_tools` 注入。
- Out：配置驱动的 `CommandTool`（`dynamic_tool.rs`）仍传 `cwd=None`，是独立通道，本次未动；滚动分页 / 虚拟滚动；终端面板 cwd（已正确）。

## Facts / Constraints

- 原滚动实现（`2026-08-15_chat-scroll-gemini-alignment`）×本次被修订：`stickyRound` 只在发送后的**第一条**回复到达时抑制滚动，第二条起落到 `scrollToNewest()`——多轮推进下必然反复拽底。
- 终端面板（`terminal/commands.rs`、`terminal/ws.rs`）已用 `resolve_spawn_cwd`：显式 `cwd` 优先，否则回退 active 工作区根，无工作区返回 `None`（沿用进程 cwd）。
- `ExecuteCommandTool` 是唯一未接工作区的 shell 入口；`Tool` trait 的 `execute(&self, args)` 无上下文，故需构建期注入。
- `docs/pulsar/terminal/index.md` 为 🔒 用户盖章文档，本次不改（新语义与其「可指定工作目录」约定不冲突）。

## Restated Understanding

- 我理解当前任务是：修两条验收缺陷——滚动不该被模型回复牵着走；shell 该落在用户选中的工作区。
- 当前核心目标是：让视口「只在用户说话时动」，让命令「在用户的工作区里跑」。
- 当前边界是：只动这两处；不碰终端域冻结文档、不动配置命令工具通道。

## 接口契约设计

- `ExecuteCommandTool` 新增构建期注入与解析：

  ```rust
  pub fn with_workspace(self, store: Arc<WorkspaceStore>) -> Self;
  fn resolve_cwd(&self, explicit: Option<String>) -> AppResult<Option<String>>;
  // Some(store) => resolve_spawn_cwd(explicit, store)   // 显式优先，否则 active 工作区根
  // None        => Ok(explicit)                          // 沿用进程 cwd（单测 / 未注入）

  /// 每次组装工具 schema 时把工作区根的具体路径写进 `cwd` 参数说明。
  fn cwd_description(&self) -> String;
  ```

- **模型必须知道路径**：`definitions_for` 在每次模型调用前现取 `Tool::parameters()`，因此把 active 工作区根的绝对路径放进 `cwd` 描述，模型不必猜（此前模型看不到环境，只能从历史/进程 cwd 里推，典型表现是硬编码 `cd <猜的路径>`）。

- `FileToolContext::workspace_store(&self) -> Arc<WorkspaceStore>`：把工作区句柄借给 shell 工具，避免重复解析。
- 前端：删除 `userScrolled` / `stickyRound` / `scrollToNewest`；`$effect` 仅在 `pendingAlignTop` 且消息数增长时吸顶。

## Checkpoint Summary

- 当前任务理解：修滚动被回复拽底 + shell 默认 cwd 错误。
- 当前核心目标：视口只随用户发送动；命令落在工作区根。
- 当前进度：代码完成，静态与单测验证通过，待 App 内人工确认。
- 涉及文件 / 模块：`ChatArea.svelte`、`tools/cmd_exec.rs`、`fileops/fs_tools.rs`、`application/gateway.rs`。
- 风险：滚动不再自动跟随到底属交互变更，长回答需用户自行滚动（与 Gemini 吸顶形态一致）。
- 验证方式：`pnpm --filter pulsar-app check`；`cargo test --lib`；App 内手动验证。
- Execution Approval: 用户报告即指令（2026-09-13）。

## Change Log

- 2026-09-13: 初始记录。滚动改为「仅发送时吸顶」；`execute_command` 缺省 cwd 回退 active 工作区根。
- 2026-09-13（修订）：验收发现模型仍硬编码 `cd <猜的路径>`（实测工作区 = `.../new-start`，模型跑 `.../new-start-wt`）。根因是**模型看不到工作区路径**——缺省 cwd 只在其不带 `cd` 时生效。补：把 active 工作区根的绝对路径写进 `cwd` 参数说明（`parameters()` 每次模型调用前现取）。

## Validation

- Self-check: 两处根因定位到行并修复；显式 `cwd` 优先级未变。
- Static checks: `pnpm --filter pulsar-app check` → 0 errors；`cargo check --lib --tests` 无 error。
- Runtime / Test: `cargo test --lib` → 485 passed / 0 failed。
- Human confirmation: 待用户 App 内确认（回复更新不再拽底；shell 落在工作区）。
- 结果汇总：自动化证据齐；人工确认待补。
- 核心目标是否已由证据证明完成：否（差人工确认）。
- 剩余风险：配置命令工具（`CommandTool`）仍沿用进程 cwd，如需对齐再单开一轮。

## Resume / Handoff

- 当前状态：修复完成，待人工确认。
- 当前卡点：App 内人工确认。
- 下一步唯一动作：发一条输入看是否只在发送时滚动；再让 Agent 跑一次 `execute_command` 看 `pwd`。
- 下一轮核心目标：如需，统一配置命令工具的工作目录口径。
