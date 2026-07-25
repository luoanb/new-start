# Lifecycle / 生命周期: ratatui-tui-redesign

```yaml
status: done
result: success
created_at: 2026-07-25 19:22
updated_at: 2026-07-25 19:50
owner: user
```

## Current Summary / 当前摘要

- 批准状态：用户已批准按 Option A 执行，Q1 选方案 A (不做 streaming)，Q2 选方案 B (删除 `/use` 用 `/model`/`/provider` 替代)。
- 当前状态：已按技术方案完成代码实现与编译验证。
- 核心变更：已将 `std::io::read_line` 行式 REPL 升级为基于 ratatui 的全屏 TUI 会话聊天工作台。

## Execution Log / 执行记录

- 1. 2026-07-25 19:22: 用户指定 TUI 技术栈使用 `ratatui`，并给出目标交互形态；按 `sdd-lab` 创建需求与技术方案文档，保持不执行代码。
- 2. 2026-07-25 19:50: 用户审阅技术方案，批准按 Option A 执行；Q1 接受方案 A（不做 token streaming），Q2 选择方案 B（用 `/model`/`/provider` 替代 `/use`）。
- 3. 2026-07-25 19:50: 完成 Step 1-6 代码实现：
  - `Cargo.toml` 新增 ratatui 0.30 / crossterm 0.28 / ratatui-textarea 0.9 依赖。
  - `lib.rs` 新增 `pub mod tui`。
  - 创建 `tui/mod.rs`：终端初始化、恢复、TerminalGuard。
  - 创建 `tui/task.rs`：TuiTaskBlock / TuiTaskKind / TuiTaskStatus。
  - 创建 `tui/error_view.rs`：AppError → TuiErrorView 映射(what/causes/actions)。
  - 创建 `tui/commands.rs`：Command 枚举、解析、帮助/提供者/模型/会话文本生成。
  - 创建 `tui/event.rs`：crossterm 事件 → TuiAction 转换(Enter 提交 / Ctrl+J 换行 / Tab 切换焦点等)。
  - 创建 `tui/render.rs`：全屏布局(顶栏状态 / 聊天流 / 任务块 / 错误横幅 / 输入区 / 帮助弹窗 / 会话列表)。
  - 创建 `tui/app.rs`：TuiApp 状态管理、消息列表、焦点切换、会话加载与切换、模型调用(带 running→done/failed 状态)、错误展示。
  - 重写 `bin/agent-app-tui.rs`：终端初始化 → TuiApp::run → 终端恢复。
- 4. 2026-07-25 19:52: 编译通过，`cargo check --bins` 零错误零警告。`pnpm --filter agent-app check` 零错误。

## Deviation Log / 偏差记录

- Q2 采纳用户选择的方案 B（删除 `/use`），与技术方案推荐的方案 A（保留兼容）不同。已在代码中实现 `/model <provider> <model>` 替代 ` /use`。
- ratatui-textarea 0.7 不可用，升级至 0.9（兼容 ratatui 0.30）。
- 取消 `set_enter_inserts_new_line` / `set_max_width` 调用（ratatui-textarea 0.9 API 变化，Enter 行为在事件层控制）。
